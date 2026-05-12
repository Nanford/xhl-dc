use std::collections::HashMap;
use std::sync::Arc;
use std::str::FromStr;
use std::time::Duration;

use anyhow::Context;
use opcua::client::{ClientBuilder, DataChangeCallback, IdentityToken, Session, SessionEventLoop};
use opcua::client::transport::TcpConnector;
use opcua::crypto::SecurityPolicy;
use opcua::types::{
    EndpointDescription, MessageSecurityMode, MonitoredItemCreateRequest, NodeId, UAString,
    TimestampsToReturn,
};
use tokio::sync::{mpsc, watch};
use tracing::{error, info, warn};

use crate::config::{
    IdentityConfig, OpcuaConfig, SecurityPolicyConfig, SubscriptionConfig, TagConfig,
};
use crate::types::TagSample;

pub async fn run_opcua_client(
    opcua_config: OpcuaConfig,
    subscriptions: Vec<SubscriptionConfig>,
    sender: mpsc::Sender<TagSample>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    loop {
        if *shutdown.borrow() {
            return Ok(());
        }

        match connect_and_subscribe(&opcua_config, &subscriptions, sender.clone(), shutdown.clone()).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                metrics::gauge!("opcua_connected").set(0.0);
                error!(error = %format_error_chain(&err), "OPC UA session failed");
                if opcua_config.session_retry_limit == 0 {
                    return Err(err);
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
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

async fn connect_and_subscribe(
    opcua_config: &OpcuaConfig,
    subscriptions: &[SubscriptionConfig],
    sender: mpsc::Sender<TagSample>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut client = ClientBuilder::new()
        .application_name("Kepware Bridge")
        .application_uri(&opcua_config.application_uri)
        .create_sample_keypair(true)
        .trust_server_certs(true)
        .session_retry_limit(opcua_config.session_retry_limit)
        .client()
        .map_err(|errors| anyhow::anyhow!("failed to build OPC UA client: {}", errors.join("; ")))?;

    let (security_policy, security_mode) = security_options(opcua_config.security_policy);
    let fallback_endpoint: EndpointDescription = (
        opcua_config.endpoint.as_str(),
        security_policy.to_uri(),
        security_mode,
    )
        .into();
    let identity = identity_token(&opcua_config.identity);

    info!(
        endpoint = %opcua_config.endpoint,
        security_policy = ?opcua_config.security_policy,
        "connecting to OPC UA endpoint"
    );

    let endpoint = match discover_endpoint(
        &client,
        &opcua_config.endpoint,
        security_policy.to_uri(),
        security_mode,
    )
        .await
    {
        Ok(endpoint) => endpoint,
        Err(err) => {
            warn!(
                error = %err,
                "OPC UA endpoint discovery failed, trying configured endpoint directly"
            );
            fallback_endpoint
        }
    };

    let (session, event_loop): (Arc<Session>, SessionEventLoop<TcpConnector>) = match client
        .connect_to_endpoint_directly(endpoint.clone(), identity.clone())
    {
        Ok(connection) => connection,
        Err(err) => {
            warn!(
                error = %err,
                endpoint_url = %endpoint.endpoint_url,
                security_policy_uri = %endpoint.security_policy_uri,
                security_mode = ?endpoint.security_mode,
                "OPC UA direct session build failed, trying exact matching endpoint"
            );
            client
                .connect_to_matching_endpoint(endpoint, identity)
                .await
                .context("failed to connect OPC UA session")?
        }
    };
    let event_loop_handle = event_loop.spawn();
    session.wait_for_connection().await;
    metrics::gauge!("opcua_connected").set(1.0);
    info!("OPC UA session connected");

    for subscription in subscriptions {
        create_subscription(&session, subscription, sender.clone()).await?;
    }

    tokio::select! {
        result = event_loop_handle => {
            metrics::gauge!("opcua_connected").set(0.0);
            match result {
                Ok(status) if status.is_good() => Ok(()),
                Ok(status) => Err(anyhow::anyhow!("OPC UA event loop stopped with status {status}")),
                Err(err) => Err(anyhow::anyhow!("OPC UA event loop join error: {err}")),
            }
        }
        _ = shutdown.changed() => {
            info!("OPC UA client received shutdown signal");
            let _ = session.disconnect().await;
            metrics::gauge!("opcua_connected").set(0.0);
            Ok(())
        }
    }
}

async fn create_subscription(
    session: &Session,
    subscription: &SubscriptionConfig,
    sender: mpsc::Sender<TagSample>,
) -> anyhow::Result<()> {
    let aliases = alias_map(&subscription.tags);
    let devices = device_map(&subscription.tags);
    let area = subscription.area.clone();
    let callback = DataChangeCallback::new(move |data_value, monitored_item| {
        let node_id = monitored_item.item_to_monitor().node_id.to_string();
        let alias = aliases
            .get(&node_id)
            .cloned()
            .unwrap_or_else(|| node_id.clone());
        let device = devices
            .get(&node_id)
            .cloned()
            .unwrap_or_default();

        match TagSample::from_data_value(node_id, alias, &area, device, data_value) {
            Ok(sample) => {
                if let Err(err) = sender.try_send(sample) {
                    metrics::counter!("dropped_samples_total").increment(1);
                    warn!(error = %err, "OPC UA sample dropped because channel is full");
                }
            }
            Err(err) => {
                warn!(error = %err, "OPC UA sample ignored");
            }
        }
    });

    let subscription_id = session
        .create_subscription(
            Duration::from_millis(subscription.publishing_interval_ms),
            subscription.lifetime_count,
            subscription.keep_alive_count,
            subscription.max_notifications_per_publish,
            subscription.priority,
            true,
            callback,
        )
        .await
        .with_context(|| format!("failed to create subscription {}", subscription.name))?;

    let items = subscription
        .tags
        .iter()
        .map(|tag| {
            let node_id = NodeId::from_str(&tag.node_id)?;
            Ok(MonitoredItemCreateRequest::from(node_id))
        })
        .collect::<Result<Vec<_>, opcua::types::StatusCode>>()?;

    let created = session
        .create_monitored_items(subscription_id, TimestampsToReturn::Both, items)
        .await
        .with_context(|| {
            format!(
                "failed to create monitored items for subscription {}",
                subscription.name
            )
        })?;

    info!(
        subscription = %subscription.name,
        subscription_id,
        monitored_items = created.len(),
        "OPC UA subscription created"
    );

    Ok(())
}

fn alias_map(tags: &[TagConfig]) -> HashMap<String, String> {
    tags.iter()
        .map(|tag| (tag.node_id.clone(), tag.alias.clone()))
        .collect()
}

fn device_map(tags: &[TagConfig]) -> HashMap<String, String> {
    tags.iter()
        .map(|tag| (tag.node_id.clone(), tag.device.clone()))
        .collect()
}

fn security_options(policy: SecurityPolicyConfig) -> (SecurityPolicy, MessageSecurityMode) {
    match policy {
        SecurityPolicyConfig::None => (SecurityPolicy::None, MessageSecurityMode::None),
        SecurityPolicyConfig::Basic256Sha256 => (
            SecurityPolicy::Basic256Sha256,
            MessageSecurityMode::SignAndEncrypt,
        ),
    }
}

fn identity_token(identity: &IdentityConfig) -> IdentityToken {
    match identity {
        IdentityConfig::Anonymous(_) => IdentityToken::Anonymous,
        IdentityConfig::UserNamePassword { username, password } => {
            IdentityToken::new_user_name(username.clone(), password.clone())
        }
    }
}

async fn discover_endpoint(
    client: &opcua::client::Client,
    endpoint_url: &str,
    security_policy_uri: &str,
    security_mode: MessageSecurityMode,
) -> anyhow::Result<EndpointDescription> {
    let endpoints = client
        .get_server_endpoints_from_url(endpoint_url)
        .await
        .with_context(|| format!("failed to discover OPC UA endpoints from {endpoint_url}"))?;

    endpoints
        .into_iter()
        .find(|endpoint| {
            ua_string_eq(&endpoint.security_policy_uri, security_policy_uri)
                && endpoint.security_mode == security_mode
        })
        .with_context(|| {
            format!(
                "cannot find discovered endpoint for policy {security_policy_uri} and mode {security_mode:?}"
            )
        })
}

fn ua_string_eq(value: &UAString, expected: &str) -> bool {
    value.as_ref() == expected
}

fn format_error_chain(err: &anyhow::Error) -> String {
    err.chain()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}
