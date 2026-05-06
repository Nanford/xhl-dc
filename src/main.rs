use std::env;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use kepware_bridge::buffer::SampleBuffer;
use kepware_bridge::config::AppConfig;
use kepware_bridge::metrics::install_prometheus;
use kepware_bridge::opcua_client::run_opcua_client;
use kepware_bridge::sink::{connect_mysql, SinkWorker};
use tokio::sync::{mpsc, watch};
use tracing::{error, info};
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
    let buffer = SampleBuffer::open(&config.buffer.path, config.buffer.max_size_mb)
        .context("failed to open sled buffer")?;

    let (sample_tx, sample_rx) = mpsc::channel::<kepware_bridge::types::TagSample>(10_000);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let sink_worker = SinkWorker::new(
        mysql_pool,
        config.sink.table.clone(),
        sample_rx,
        shutdown_rx.clone(),
        buffer,
        config.sink.batch_size,
        Duration::from_millis(config.sink.flush_interval_ms),
    );
    let sink_handle = tokio::spawn(sink_worker.run());

    let opcua_handle = tokio::spawn(run_opcua_client(config, sample_tx, shutdown_rx));

    shutdown_signal().await?;
    info!("shutdown signal received");
    let _ = shutdown_tx.send(true);

    let shutdown_result = tokio::time::timeout(Duration::from_secs(30), async {
        let opcua_result = opcua_handle.await;
        let sink_result = sink_handle.await;
        (opcua_result, sink_result)
    })
    .await;

    match shutdown_result {
        Ok((opcua_result, sink_result)) => {
            match opcua_result {
                Ok(Ok(())) => {}
                Ok(Err(err)) => error!(error = %err, "OPC UA task failed"),
                Err(err) => error!(error = %err, "OPC UA task join failed"),
            }
            match sink_result {
                Ok(Ok(())) => {}
                Ok(Err(err)) => error!(error = %err, "sink task failed"),
                Err(err) => error!(error = %err, "sink task join failed"),
            }
        }
        Err(_) => {
            error!("shutdown timed out after 30 seconds");
        }
    }

    Ok(())
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,opcua=warn"));
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
