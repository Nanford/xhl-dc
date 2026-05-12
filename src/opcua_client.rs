use std::collections::HashMap;
use std::fs;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use opcua::client::transport::TcpConnector;
use opcua::client::{ClientBuilder, DataChangeCallback, IdentityToken, Session, SessionEventLoop};
use opcua::crypto::SecurityPolicy;
use opcua::types::{
    AttributeId, DataValue, EndpointDescription, MessageSecurityMode, MonitoredItemCreateRequest,
    NodeId, ReadValueId, TimestampsToReturn, UAString, Variant,
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

    let external_descriptions = load_description_map(opcua_config.description_map_path.as_deref())
        .context("failed to load OPC UA description map")?;

    for subscription in subscriptions {
        create_subscription(&session, subscription, sender.clone(), &external_descriptions).await?;
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
    external_descriptions: &HashMap<String, String>,
) -> anyhow::Result<()> {
    let aliases = alias_map(&subscription.tags);
    let devices = device_map(&subscription.tags);
    let device_ids = device_id_map(&subscription.tags);
    let server_descriptions = read_server_description_map(session, &subscription.tags).await;
    let descriptions =
        build_description_map(&subscription.tags, external_descriptions, server_descriptions);
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
        let device_id = device_ids
            .get(&node_id)
            .cloned()
            .unwrap_or_default();
        let description = descriptions
            .get(&node_id)
            .cloned()
            .unwrap_or_default();

        match TagSample::from_data_value(
            node_id,
            alias,
            &area,
            device,
            device_id,
            description,
            data_value,
        ) {
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

fn device_id_map(tags: &[TagConfig]) -> HashMap<String, String> {
    tags.iter()
        .map(|tag| (tag.node_id.clone(), tag.device_id.clone()))
        .collect()
}

async fn read_server_description_map(
    session: &Session,
    tags: &[TagConfig],
) -> HashMap<String, String> {
    let mut descriptions = read_kepware_description_properties(session, tags).await;
    let attribute_descriptions =
        read_opcua_description_attributes(session, tags, &descriptions).await;
    descriptions.extend(attribute_descriptions);
    descriptions
}

async fn read_kepware_description_properties(
    session: &Session,
    tags: &[TagConfig],
) -> HashMap<String, String> {
    let nodes = tags
        .iter()
        .filter(|tag| tag.description.trim().is_empty())
        .filter_map(|tag| match kepware_description_property_node_id(&tag.node_id) {
            Some(node_id) => Some((tag.node_id.clone(), ReadValueId::new_value(node_id))),
            None => {
                warn!(
                    node_id = %tag.node_id,
                    "skipping Kepware _Description read for non-string NodeId"
                );
                None
            }
        })
        .collect::<Vec<_>>();

    read_description_values(
        session,
        &nodes,
        "failed to read Kepware _Description property tags",
    )
    .await
}

async fn read_opcua_description_attributes(
    session: &Session,
    tags: &[TagConfig],
    known_descriptions: &HashMap<String, String>,
) -> HashMap<String, String> {
    let nodes = tags
        .iter()
        .filter(|tag| {
            tag.description.trim().is_empty() && !known_descriptions.contains_key(&tag.node_id)
        })
        .filter_map(|tag| match NodeId::from_str(&tag.node_id) {
            Ok(node_id) => Some((
                tag.node_id.clone(),
                ReadValueId::new(node_id, AttributeId::Description),
            )),
            Err(err) => {
                warn!(
                    node_id = %tag.node_id,
                    error = %err,
                    "skipping OPC UA description read for invalid NodeId"
                );
                None
            }
        })
        .collect::<Vec<_>>();

    read_description_values(session, &nodes, "failed to read OPC UA node descriptions").await
}

async fn read_description_values(
    session: &Session,
    nodes: &[(String, ReadValueId)],
    failure_message: &'static str,
) -> HashMap<String, String> {
    let mut descriptions = HashMap::new();
    for chunk in nodes.chunks(100) {
        let read_values = chunk
            .iter()
            .map(|(_, read_value)| read_value.clone())
            .collect::<Vec<_>>();
        match session
            .read(&read_values, TimestampsToReturn::Neither, 0.0)
            .await
        {
            Ok(values) => {
                for ((node_id, _), value) in chunk.iter().zip(values.iter()) {
                    if let Some(description) = description_text_from_data_value(value) {
                        descriptions.insert(node_id.clone(), description);
                    }
                }
            }
            Err(err) => {
                warn!(
                    error = %err,
                    nodes = chunk.len(),
                    failure_message,
                    "OPC UA description read failed"
                );
            }
        }
    }

    descriptions
}

fn kepware_description_property_node_id(node_id: &str) -> Option<NodeId> {
    let tag_path = opcua_string_node_id(node_id)?.trim_end_matches("._Description");
    let namespace = node_id
        .strip_prefix("ns=")?
        .split_once(';')?
        .0
        .parse::<u16>()
        .ok()?;
    Some(NodeId::new(namespace, format!("{tag_path}._Description")))
}

fn build_description_map(
    tags: &[TagConfig],
    external_descriptions: &HashMap<String, String>,
    server_descriptions: HashMap<String, String>,
) -> HashMap<String, String> {
    tags.iter()
        .map(|tag| {
            let description = if tag.description.trim().is_empty() {
                mapped_description(tag, external_descriptions)
                    .or_else(|| server_descriptions.get(&tag.node_id).cloned())
                    .unwrap_or_default()
            } else {
                tag.description.clone()
            };
            (tag.node_id.clone(), description)
        })
        .collect()
}

fn mapped_description(
    tag: &TagConfig,
    external_descriptions: &HashMap<String, String>,
) -> Option<String> {
    [Some(tag.node_id.as_str()), opcua_string_node_id(&tag.node_id)]
        .into_iter()
        .flatten()
        .find_map(|key| {
            external_descriptions
                .get(key)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

fn load_description_map(path: Option<&str>) -> anyhow::Result<HashMap<String, String>> {
    let Some(path) = path.map(str::trim).filter(|path| !path.is_empty()) else {
        return Ok(HashMap::new());
    };

    let content = fs::read_to_string(path).with_context(|| format!("failed to read {path}"))?;
    let values: HashMap<String, String> =
        serde_yaml::from_str(content.trim_start_matches('\u{feff}'))
            .with_context(|| format!("failed to parse {path}"))?;

    Ok(values
        .into_iter()
        .filter_map(|(key, value)| {
            let key = key.trim().to_string();
            let value = value.trim().to_string();
            if key.is_empty() || value.is_empty() {
                None
            } else {
                Some((key, value))
            }
        })
        .collect())
}

fn opcua_string_node_id(node_id: &str) -> Option<&str> {
    node_id
        .split_once(";s=")
        .map(|(_, tag)| tag)
        .filter(|tag| !tag.trim().is_empty())
}

fn description_text_from_data_value(value: &DataValue) -> Option<String> {
    if value.status.is_some_and(|status| !status.is_good()) {
        return None;
    }

    let description = match value.value.as_ref()? {
        Variant::LocalizedText(text) => text.text.as_ref().trim(),
        Variant::String(text) => text.as_ref().trim(),
        _ => {
            return None;
        }
    };

    if description.is_empty() {
        None
    } else {
        Some(description.to_string())
    }
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use opcua::types::{DataValue, LocalizedText, Variant};

    use super::*;

    #[test]
    fn extracts_description_text_from_localized_text_value() {
        let value = DataValue {
            value: Some(Variant::LocalizedText(Box::new(LocalizedText::new(
                "",
                "Drive fault alarm",
            )))),
            ..Default::default()
        };

        assert_eq!(
            description_text_from_data_value(&value),
            Some("Drive fault alarm".to_string())
        );
    }

    #[test]
    fn extracts_description_text_from_kepware_property_string_value() {
        let value = DataValue {
            value: Some(Variant::String("错误1".into())),
            ..Default::default()
        };

        assert_eq!(
            description_text_from_data_value(&value),
            Some("错误1".to_string())
        );
    }

    #[test]
    fn builds_kepware_description_property_node_id() {
        let node_id = kepware_description_property_node_id(
            "ns=2;s=WH_CPK_Zone01.convey.conveyor.M5001.Alarm.error1",
        )
        .expect("string NodeId should support Kepware description property");

        assert_eq!(
            node_id.to_string(),
            "ns=2;s=WH_CPK_Zone01.convey.conveyor.M5001.Alarm.error1._Description"
        );
    }

    #[test]
    fn builds_description_map_with_config_overriding_server_metadata() {
        let tags = vec![
            TagConfig {
                node_id: "ns=2;s=Configured".to_string(),
                alias: "configured".to_string(),
                device: String::new(),
                device_id: String::new(),
                description: "Configured description".to_string(),
            },
            TagConfig {
                node_id: "ns=2;s=FromServer".to_string(),
                alias: "from_server".to_string(),
                device: String::new(),
                device_id: String::new(),
                description: String::new(),
            },
        ];
        let external_descriptions = HashMap::from([
            (
                "ns=2;s=FromExternal".to_string(),
                "External description".to_string(),
            ),
            (
                "PathOnly.Tag".to_string(),
                "Path-only external description".to_string(),
            ),
        ]);
        let server_descriptions = HashMap::from([
            (
                "ns=2;s=Configured".to_string(),
                "Server configured description".to_string(),
            ),
            (
                "ns=2;s=FromServer".to_string(),
                "Server description".to_string(),
            ),
            (
                "ns=2;s=FromExternal".to_string(),
                "Server external description".to_string(),
            ),
        ]);

        let mut tags = tags;
        tags.push(TagConfig {
            node_id: "ns=2;s=FromExternal".to_string(),
            alias: "from_external".to_string(),
            device: String::new(),
            device_id: String::new(),
            description: String::new(),
        });
        tags.push(TagConfig {
            node_id: "ns=2;s=PathOnly.Tag".to_string(),
            alias: "path_only".to_string(),
            device: String::new(),
            device_id: String::new(),
            description: String::new(),
        });

        let descriptions =
            build_description_map(&tags, &external_descriptions, server_descriptions);

        assert_eq!(
            descriptions.get("ns=2;s=Configured"),
            Some(&"Configured description".to_string())
        );
        assert_eq!(
            descriptions.get("ns=2;s=FromServer"),
            Some(&"Server description".to_string())
        );
        assert_eq!(
            descriptions.get("ns=2;s=FromExternal"),
            Some(&"External description".to_string())
        );
        assert_eq!(
            descriptions.get("ns=2;s=PathOnly.Tag"),
            Some(&"Path-only external description".to_string())
        );
    }
}
