use std::net::SocketAddr;

use anyhow::Context;
use metrics::{describe_counter, describe_gauge};
use metrics_exporter_prometheus::PrometheusBuilder;

pub fn install_prometheus(bind: &str) -> anyhow::Result<()> {
    let addr = bind
        .parse::<SocketAddr>()
        .with_context(|| format!("invalid metrics bind address {bind}"))?;

    PrometheusBuilder::new()
        .with_http_listener(addr)
        .install()
        .context("failed to install prometheus metrics exporter")?;

    describe_counter!(
        "samples_received_total",
        "Samples accepted from all collectors"
    );
    describe_counter!(
        "dropped_samples_total",
        "Samples dropped because the in-memory channel was full"
    );
    describe_counter!(
        "mysql_inserted_samples_total",
        "Samples inserted into MySQL"
    );
    describe_counter!(
        "buffered_samples_total",
        "Samples written to the local sled failure buffer"
    );
    describe_counter!(
        "buffer_replayed_samples_total",
        "Buffered samples replayed into MySQL"
    );
    describe_gauge!(
        "opcua_connected",
        "OPC UA connection state, 1 for connected and 0 for disconnected"
    );
    describe_gauge!(
        "wcs_connected",
        "WCS poller state, 1 for polling and 0 for disconnected"
    );
    describe_counter!(
        "wcs_poll_errors_total",
        "WCS HTTP poll errors"
    );

    Ok(())
}

