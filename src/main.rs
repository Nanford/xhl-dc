use std::env;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use kepware_bridge::buffer::SampleBuffer;
use kepware_bridge::config::AppConfig;
use kepware_bridge::metadata::TagMetadataCache;
use kepware_bridge::metrics::install_prometheus;
use kepware_bridge::opcua_client::run_opcua_client;
use kepware_bridge::sink::{connect_mysql, SinkTableRouter, SinkWorker, SinkWorkerSettings};
use kepware_bridge::wcs_client::run_wcs_client;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let config_path = config_path_from_args();
    let config = AppConfig::load(&config_path)
        .with_context(|| format!("failed to load config {}", config_path.display()))?;

    install_prometheus(&config.metrics.bind)?;

    let mysql_pool = connect_mysql(&config.mysql)
        .await
        .context("failed to connect MySQL")?;
    let metadata_cache = match TagMetadataCache::load(&mysql_pool).await {
        Ok(cache) => cache,
        Err(err) => {
            warn!(
                error = %err,
                "tag metadata cache unavailable; alarm history fault_type will be empty"
            );
            TagMetadataCache::default()
        }
    };
    let buffer = SampleBuffer::open(&config.buffer.path, config.buffer.max_size_mb)
        .context("failed to open sled buffer")?;

    let (sample_tx, sample_rx) = mpsc::channel::<kepware_bridge::types::TagSample>(10_000);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let sink_router =
        SinkTableRouter::from_config(&config.sink).context("failed to build sink table router")?;
    let sink_subscriptions = config.subscriptions.clone();
    let sink_worker = SinkWorker::new(
        mysql_pool,
        sink_router,
        sample_rx,
        shutdown_rx.clone(),
        buffer,
        metadata_cache,
        SinkWorkerSettings {
            batch_size: config.sink.batch_size,
            flush_interval: Duration::from_millis(config.sink.flush_interval_ms),
            subscriptions: sink_subscriptions,
        },
    );
    let sink_handle = tokio::spawn(sink_worker.run());

    let mut collectors = JoinSet::new();

    if let Some(opcua_config) = config.opcua {
        let subscriptions = config.subscriptions;
        let tx = sample_tx.clone();
        let rx = shutdown_rx.clone();
        collectors.spawn(async move {
            run_opcua_client(opcua_config, subscriptions, tx, rx)
                .await
                .context("OPC UA collector failed")
        });
        info!("OPC UA collector spawned");
    }

    if let Some(wcs_config) = config.wcs {
        let tx = sample_tx.clone();
        let rx = shutdown_rx.clone();
        collectors.spawn(async move {
            run_wcs_client(wcs_config, tx, rx)
                .await
                .context("WCS collector failed")
        });
        info!("WCS collector spawned");
    }

    drop(sample_tx);

    shutdown_signal().await?;
    info!("shutdown signal received");
    let _ = shutdown_tx.send(true);

    let shutdown_result = tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(result) = collectors.join_next().await {
            match result {
                Ok(Ok(())) => {}
                Ok(Err(err)) => error!(error = %err, "collector task failed"),
                Err(err) => error!(error = %err, "collector task join failed"),
            }
        }
        match sink_handle.await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => error!(error = %err, "sink task failed"),
            Err(err) => error!(error = %err, "sink task join failed"),
        }
    })
    .await;

    if shutdown_result.is_err() {
        error!("shutdown timed out after 30 seconds");
    }

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,opcua=warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn config_path_from_args() -> PathBuf {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--config" || arg == "-c" {
            if let Some(path) = args.next() {
                return PathBuf::from(path);
            }
        }
    }
    PathBuf::from("config.yaml")
}

async fn shutdown_signal() -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut terminate = signal(SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result?,
            _ = terminate.recv() => {}
        }
        Ok(())
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await?;
        Ok(())
    }
}
