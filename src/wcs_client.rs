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
    metrics::gauge!("wcs_connected").set(0.0);

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
    let result = poll_endpoint_inner(client, base_url, headers, endpoint, sender).await;
    metrics::gauge!("wcs_connected").set(if result.is_ok() { 1.0 } else { 0.0 });
    result
}

async fn poll_endpoint_inner(
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
                    &endpoint.area,
                    &tag.device,
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use metrics::{
        Counter, CounterFn, Gauge, GaugeFn, Histogram, Key, KeyName, Metadata, Recorder,
        SharedString, Unit,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;
    use crate::config::WcsTagConfig;

    #[tokio::test(flavor = "current_thread")]
    async fn poll_endpoint_does_not_increment_received_samples_metric() {
        let recorder = TestRecorder::default();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let base_url = serve_once("200 OK", r#"{"running":true}"#).await;
        let endpoint = test_endpoint("running", WcsValueType::Bool);
        let client = reqwest::Client::new();
        let (sender, mut receiver) = mpsc::channel(1);

        poll_endpoint(&client, &base_url, &HashMap::new(), &endpoint, &sender)
            .await
            .unwrap();

        let sample = receiver.try_recv().unwrap();
        assert_eq!(sample.alias, "running");
        assert_eq!(recorder.counter_value("samples_received_total"), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn poll_endpoint_marks_wcs_disconnected_after_http_failure() {
        let recorder = TestRecorder::default();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let base_url = serve_once("500 Internal Server Error", r#"{"error":"down"}"#).await;
        let endpoint = test_endpoint("running", WcsValueType::Bool);
        let client = reqwest::Client::new();
        let (sender, _receiver) = mpsc::channel(1);

        let result = poll_endpoint(&client, &base_url, &HashMap::new(), &endpoint, &sender).await;

        assert!(result.is_err());
        assert_eq!(recorder.gauge_value("wcs_connected"), Some(0.0));
    }

    async fn serve_once(status: &'static str, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 1024];
            let _ = stream.read(&mut request).await.unwrap();
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        format!("http://{addr}")
    }

    fn test_endpoint(json_path: &str, value_type: WcsValueType) -> WcsEndpointConfig {
        WcsEndpointConfig {
            path: "/status".to_string(),
            area: String::new(),
            method: WcsHttpMethod::GET,
            tags: vec![WcsTagConfig {
                json_path: json_path.to_string(),
                alias: json_path.to_string(),
                device: String::new(),
                value_type,
            }],
        }
    }

    #[derive(Clone, Default)]
    struct TestRecorder {
        counters: Arc<Mutex<HashMap<String, Arc<TestCounter>>>>,
        gauges: Arc<Mutex<HashMap<String, Arc<TestGauge>>>>,
    }

    impl TestRecorder {
        fn counter_value(&self, name: &str) -> u64 {
            self.counters
                .lock()
                .unwrap()
                .get(name)
                .map(|counter| counter.value())
                .unwrap_or(0)
        }

        fn gauge_value(&self, name: &str) -> Option<f64> {
            self.gauges
                .lock()
                .unwrap()
                .get(name)
                .map(|gauge| gauge.value())
        }
    }

    impl Recorder for TestRecorder {
        fn describe_counter(
            &self,
            _key: KeyName,
            _unit: Option<Unit>,
            _description: SharedString,
        ) {
        }

        fn describe_gauge(
            &self,
            _key: KeyName,
            _unit: Option<Unit>,
            _description: SharedString,
        ) {
        }

        fn describe_histogram(
            &self,
            _key: KeyName,
            _unit: Option<Unit>,
            _description: SharedString,
        ) {
        }

        fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
            let counter = self
                .counters
                .lock()
                .unwrap()
                .entry(key.name().to_string())
                .or_default()
                .clone();
            Counter::from_arc(counter)
        }

        fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
            let gauge = self
                .gauges
                .lock()
                .unwrap()
                .entry(key.name().to_string())
                .or_default()
                .clone();
            Gauge::from_arc(gauge)
        }

        fn register_histogram(&self, _key: &Key, _metadata: &Metadata<'_>) -> Histogram {
            Histogram::noop()
        }
    }

    #[derive(Default)]
    struct TestCounter {
        value: Mutex<u64>,
    }

    impl TestCounter {
        fn value(&self) -> u64 {
            *self.value.lock().unwrap()
        }
    }

    impl CounterFn for TestCounter {
        fn increment(&self, value: u64) {
            *self.value.lock().unwrap() += value;
        }

        fn absolute(&self, value: u64) {
            let mut current = self.value.lock().unwrap();
            *current = (*current).max(value);
        }
    }

    #[derive(Default)]
    struct TestGauge {
        value: Mutex<f64>,
    }

    impl TestGauge {
        fn value(&self) -> f64 {
            *self.value.lock().unwrap()
        }
    }

    impl GaugeFn for TestGauge {
        fn increment(&self, value: f64) {
            *self.value.lock().unwrap() += value;
        }

        fn decrement(&self, value: f64) {
            *self.value.lock().unwrap() -= value;
        }

        fn set(&self, value: f64) {
            *self.value.lock().unwrap() = value;
        }
    }
}
