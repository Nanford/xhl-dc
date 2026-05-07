use std::collections::HashMap;
use std::time::Duration;

use anyhow::Context;
use chrono::Utc;
use serde_json::Value as JsonValue;
use tokio::sync::{mpsc, watch};
use tracing::{error, info, warn};

use crate::config::{WcsConfig, WcsEndpointConfig, WcsHttpMethod, WcsValueType};
use crate::types::{TagSample, ValueKind};

pub async fn run_wcs_client(
    config: WcsConfig,
    sender: mpsc::Sender<TagSample>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    loop {
        if *shutdown.borrow() {
            return Ok(());
        }

        match poll_loop(&config, sender.clone(), shutdown.clone()).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                metrics::gauge!("wcs_connected").set(0.0);
                error!(error = %err, "WCS poll loop failed");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(config.retry_interval_ms)) => {}
                    _ = shutdown.changed() => {
                        if *shutdown.borrow() {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
}

async fn poll_loop(
    config: &WcsConfig,
    sender: mpsc::Sender<TagSample>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(config.timeout_ms))
        .build()
        .context("failed to build HTTP client")?;

    let mut interval = tokio::time::interval(Duration::from_millis(config.poll_interval_ms));

    info!(
        base_url = %config.base_url,
        poll_interval_ms = config.poll_interval_ms,
        endpoints = config.endpoints.len(),
        "WCS poller started"
    );
    metrics::gauge!("wcs_connected").set(1.0);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                for endpoint in &config.endpoints {
                    if let Err(err) = poll_endpoint(
                        &client,
                        &config.base_url,
                        &config.headers,
                        endpoint,
                        &sender,
                    ).await {
                        metrics::counter!("wcs_poll_errors_total").increment(1);
                        warn!(
                            error = %err,
                            endpoint = %endpoint.path,
                            "WCS endpoint poll failed"
                        );
                    }
                }
            }
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    info!("WCS poller received shutdown signal");
                    metrics::gauge!("wcs_connected").set(0.0);
                    return Ok(());
                }
            }
        }
    }
}

async fn poll_endpoint(
    client: &reqwest::Client,
    base_url: &str,
    headers: &HashMap<String, String>,
    endpoint: &WcsEndpointConfig,
    sender: &mpsc::Sender<TagSample>,
) -> anyhow::Result<()> {
    let url = format!("{}{}", base_url.trim_end_matches('/'), &endpoint.path);

    let mut request = match endpoint.method {
        WcsHttpMethod::GET => client.get(&url),
        WcsHttpMethod::POST => client.post(&url),
    };

    for (key, value) in headers {
        request = request.header(key, value);
    }

    let response = request.send().await.context("HTTP request failed")?;

    if !response.status().is_success() {
        anyhow::bail!("HTTP {} from {}", response.status(), url);
    }

    let body: JsonValue = response.json().await.context("failed to parse JSON response")?;
    let now = Utc::now();

    for tag in &endpoint.tags {
        match extract_value(&body, &tag.json_path, tag.value_type) {
            Ok(value) => {
                let node_id = format!("wcs:{}.{}", endpoint.path, tag.json_path);
                let sample = TagSample::new(
                    node_id,
                    &tag.alias,
                    value,
                    now,
                    now,
                    0,
                    "wcs",
                );

                if let Err(err) = sender.try_send(sample) {
                    metrics::counter!("dropped_samples_total").increment(1);
                    warn!(error = %err, alias = %tag.alias, "WCS sample dropped");
                }
                metrics::counter!("samples_received_total").increment(1);
            }
            Err(err) => {
                warn!(
                    error = %err,
                    alias = %tag.alias,
                    json_path = %tag.json_path,
                    "WCS field extraction failed"
                );
            }
        }
    }

    Ok(())
}

pub fn extract_value(
    body: &JsonValue,
    json_path: &str,
    value_type: WcsValueType,
) -> anyhow::Result<ValueKind> {
    let mut current = body;
    for segment in json_path.split('.') {
        current = current
            .get(segment)
            .with_context(|| format!("field '{segment}' not found in JSON"))?;
    }

    match value_type {
        WcsValueType::Bool => {
            let v = current
                .as_bool()
                .with_context(|| format!("expected bool at '{json_path}', got {current}"))?;
            Ok(ValueKind::Bool(v))
        }
        WcsValueType::Int => {
            let v = current
                .as_i64()
                .with_context(|| format!("expected int at '{json_path}', got {current}"))?;
            Ok(ValueKind::Int(v))
        }
        WcsValueType::Float => {
            let v = current
                .as_f64()
                .with_context(|| format!("expected float at '{json_path}', got {current}"))?;
            Ok(ValueKind::Float(v))
        }
        WcsValueType::Text => {
            let v = current
                .as_str()
                .with_context(|| format!("expected string at '{json_path}', got {current}"))?;
            Ok(ValueKind::Text(v.to_string()))
        }
    }
}
