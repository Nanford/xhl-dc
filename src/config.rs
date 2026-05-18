use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use opcua::types::NodeId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::sink::validate_mysql_identifier;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config file {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse yaml config: {0}")]
    Parse(#[from] serde_yaml::Error),
    #[error("failed to parse yaml config file {path}: {source}")]
    ParseFile {
        path: String,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("invalid config: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub opcua: Option<OpcuaConfig>,
    pub mysql: MysqlConfig,
    #[serde(default)]
    pub subscriptions: Vec<SubscriptionConfig>,
    pub sink: SinkConfig,
    pub buffer: BufferConfig,
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub wcs: Option<WcsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpcuaConfig {
    pub endpoint: String,
    pub security_policy: SecurityPolicyConfig,
    pub identity: IdentityConfig,
    #[serde(default = "default_retry_limit")]
    pub session_retry_limit: i32,
    pub application_uri: String,
    #[serde(default)]
    pub description_map_path: Option<String>,
    #[serde(default)]
    pub subscription_files: Vec<String>,
    #[serde(default = "default_monitored_item_create_batch_size_count")]
    pub monitored_item_create_batch_size_count: usize,
    #[serde(default)]
    pub discovery: Option<OpcuaDiscoveryConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpcuaDiscoveryConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_discovery_target_subscription")]
    pub target_subscription: String,
    #[serde(default)]
    pub root_node_ids: Vec<String>,
    #[serde(default)]
    pub include_paths: Vec<String>,
    #[serde(default)]
    pub exclude_paths: Vec<String>,
    #[serde(default = "default_discovery_min_namespace_index")]
    pub min_namespace_index: u16,
    #[serde(default = "default_discovery_max_depth_count")]
    pub max_depth_count: u32,
    #[serde(default = "default_discovery_max_tags_count")]
    pub max_tags_count: usize,
    #[serde(default)]
    pub include_system: bool,
    #[serde(default)]
    pub include_arrays: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityPolicyConfig {
    None,
    Basic256Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IdentityConfig {
    Anonymous(String),
    UserNamePassword { username: String, password: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MysqlConfig {
    pub url: String,
    #[serde(default = "default_mysql_connections")]
    pub max_connections: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionConfig {
    pub name: String,
    #[serde(default)]
    pub area: String,
    pub publishing_interval_ms: u64,
    #[serde(default = "default_keep_alive_count")]
    pub keep_alive_count: u32,
    #[serde(default = "default_lifetime_count")]
    pub lifetime_count: u32,
    #[serde(default)]
    pub max_notifications_per_publish: u32,
    #[serde(default)]
    pub priority: u8,
    pub tags: Vec<TagConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagConfig {
    pub node_id: String,
    pub alias: String,
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SinkConfig {
    pub table: String,
    #[serde(default)]
    pub tag_prefix_routes: HashMap<String, SinkTableRouteConfig>,
    pub batch_size: usize,
    pub flush_interval_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SinkTableRouteConfig {
    pub table: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferConfig {
    pub path: String,
    pub max_size_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub bind: String,
}

impl AppConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let content = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let mut config = Self::parse_yaml_str(&content)?;
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        config.load_subscription_files(base_dir)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_yaml_str(content: &str) -> Result<Self, ConfigError> {
        let config = Self::parse_yaml_str(content)?;
        config.validate()?;
        Ok(config)
    }

    fn parse_yaml_str(content: &str) -> Result<Self, ConfigError> {
        Ok(serde_yaml::from_str(
            content.trim_start_matches('\u{feff}'),
        )?)
    }

    fn load_subscription_files(&mut self, base_dir: &Path) -> Result<(), ConfigError> {
        let Some(opcua) = &self.opcua else {
            return Ok(());
        };
        for file in opcua.subscription_files.clone() {
            if file.trim().is_empty() {
                return invalid("opcua.subscription_files must not contain empty values");
            }
            let path = config_relative_path(base_dir, &file);
            let content = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
                path: path.display().to_string(),
                source,
            })?;
            let subscription_file: SubscriptionFile =
                serde_yaml::from_str(content.trim_start_matches('\u{feff}')).map_err(|source| {
                    ConfigError::ParseFile {
                        path: path.display().to_string(),
                        source,
                    }
                })?;
            self.subscriptions
                .extend(subscription_file.into_subscriptions());
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.opcua.is_none() && self.wcs.is_none() {
            return invalid("at least one collector (opcua or wcs) must be configured");
        }

        if let Some(opcua) = &self.opcua {
            if opcua.endpoint.trim().is_empty() {
                return invalid("opcua.endpoint must not be empty");
            }
            if opcua.application_uri.trim().is_empty() {
                return invalid("opcua.application_uri must not be empty");
            }
            if let Some(path) = &opcua.description_map_path {
                if path.trim().is_empty() {
                    return invalid("opcua.description_map_path must not be empty");
                }
            }
            for path in &opcua.subscription_files {
                if path.trim().is_empty() {
                    return invalid("opcua.subscription_files must not contain empty values");
                }
            }
            if opcua.session_retry_limit < -1 {
                return invalid("opcua.session_retry_limit must be -1 or greater");
            }
            if opcua.monitored_item_create_batch_size_count == 0 {
                return invalid(
                    "opcua.monitored_item_create_batch_size_count must be greater than 0",
                );
            }
            opcua.identity.validate()?;
            if let Some(discovery) = &opcua.discovery {
                discovery.validate()?;
            }

            if self.subscriptions.is_empty() {
                return invalid("subscriptions must not be empty when opcua is configured");
            }
            for subscription in &self.subscriptions {
                subscription.validate()?;
            }
            validate_unique_subscription_names(&self.subscriptions)?;
            if let Some(discovery) = &opcua.discovery {
                if discovery.enabled
                    && !self
                        .subscriptions
                        .iter()
                        .any(|subscription| subscription.name == discovery.target_subscription)
                {
                    return invalid(format!(
                        "opcua.discovery.target_subscription {} does not match any subscription",
                        discovery.target_subscription
                    ));
                }
            }
        }

        if let Some(wcs) = &self.wcs {
            wcs.validate()?;
        }

        if self.mysql.url.trim().is_empty() {
            return invalid("mysql.url must not be empty");
        }
        if self.mysql.max_connections == 0 {
            return invalid("mysql.max_connections must be greater than 0");
        }

        validate_mysql_identifier(&self.sink.table)
            .map_err(|err| ConfigError::Invalid(format!("sink.table: {err}")))?;
        for (prefix, route) in &self.sink.tag_prefix_routes {
            if prefix.trim().is_empty() {
                return invalid("sink.tag_prefix_routes key must not be empty");
            }
            if prefix.contains('.') {
                return invalid(format!(
                    "sink.tag_prefix_routes key {prefix} must be the first tag segment only"
                ));
            }
            validate_mysql_identifier(&route.table).map_err(|err| {
                ConfigError::Invalid(format!("sink.tag_prefix_routes.{prefix}.table: {err}"))
            })?;
        }
        if self.sink.batch_size == 0 {
            return invalid("sink.batch_size must be greater than 0");
        }
        if self.sink.flush_interval_ms == 0 {
            return invalid("sink.flush_interval_ms must be greater than 0");
        }

        if self.buffer.path.trim().is_empty() {
            return invalid("buffer.path must not be empty");
        }
        if self.buffer.max_size_mb == 0 {
            return invalid("buffer.max_size_mb must be greater than 0");
        }

        self.metrics
            .bind
            .parse::<SocketAddr>()
            .map_err(|err| ConfigError::Invalid(format!("metrics.bind is invalid: {err}")))?;

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum SubscriptionFile {
    Wrapped {
        subscriptions: Vec<SubscriptionConfig>,
    },
    List(Vec<SubscriptionConfig>),
}

impl SubscriptionFile {
    fn into_subscriptions(self) -> Vec<SubscriptionConfig> {
        match self {
            SubscriptionFile::Wrapped { subscriptions } => subscriptions,
            SubscriptionFile::List(subscriptions) => subscriptions,
        }
    }
}

impl OpcuaDiscoveryConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if !self.enabled {
            return Ok(());
        }
        if self.target_subscription.trim().is_empty() {
            return invalid("opcua.discovery.target_subscription must not be empty");
        }
        if self.max_depth_count == 0 {
            return invalid("opcua.discovery.max_depth_count must be greater than 0");
        }
        if self.max_tags_count == 0 {
            return invalid("opcua.discovery.max_tags_count must be greater than 0");
        }
        for root_node_id in &self.root_node_ids {
            if root_node_id.trim().is_empty() {
                return invalid("opcua.discovery.root_node_ids must not contain empty values");
            }
            NodeId::from_str(root_node_id).map_err(|err| {
                ConfigError::Invalid(format!(
                    "opcua.discovery.root_node_ids value {root_node_id} is invalid: {err}"
                ))
            })?;
        }
        validate_discovery_paths("include_paths", &self.include_paths)?;
        validate_discovery_paths("exclude_paths", &self.exclude_paths)?;
        Ok(())
    }
}

impl IdentityConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        match self {
            IdentityConfig::Anonymous(value) if value.eq_ignore_ascii_case("anonymous") => Ok(()),
            IdentityConfig::Anonymous(_) => invalid("opcua.identity string must be anonymous"),
            IdentityConfig::UserNamePassword { username, password } => {
                if username.trim().is_empty() {
                    return invalid("opcua.identity.username must not be empty");
                }
                if password.is_empty() {
                    return invalid("opcua.identity.password must not be empty");
                }
                Ok(())
            }
        }
    }
}

impl SubscriptionConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.name.trim().is_empty() {
            return invalid("subscription.name must not be empty");
        }
        if self.publishing_interval_ms == 0 {
            return invalid(format!(
                "subscription {} publishing_interval_ms must be greater than 0",
                self.name
            ));
        }
        if self.keep_alive_count == 0 {
            return invalid(format!(
                "subscription {} keep_alive_count must be greater than 0",
                self.name
            ));
        }
        if self.lifetime_count < self.keep_alive_count.saturating_mul(3) {
            return invalid(format!(
                "subscription {} lifetime_count must be at least 3 * keep_alive_count",
                self.name
            ));
        }
        if self.tags.is_empty() {
            return invalid(format!("subscription {} tags must not be empty", self.name));
        }
        for tag in &self.tags {
            tag.validate(&self.name)?;
        }
        Ok(())
    }
}

impl TagConfig {
    fn validate(&self, subscription_name: &str) -> Result<(), ConfigError> {
        if self.alias.trim().is_empty() {
            return invalid(format!(
                "subscription {subscription_name} tag alias must not be empty"
            ));
        }
        NodeId::from_str(&self.node_id).map_err(|err| {
            ConfigError::Invalid(format!(
                "subscription {subscription_name} node_id {} is invalid: {err}",
                self.node_id
            ))
        })?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WcsConfig {
    pub base_url: String,
    #[serde(default = "default_wcs_poll_interval_ms")]
    pub poll_interval_ms: u64,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default = "default_wcs_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_wcs_retry_interval_ms")]
    pub retry_interval_ms: u64,
    pub endpoints: Vec<WcsEndpointConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WcsEndpointConfig {
    pub path: String,
    #[serde(default)]
    pub area: String,
    #[serde(default)]
    pub method: WcsHttpMethod,
    pub tags: Vec<WcsTagConfig>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WcsHttpMethod {
    #[default]
    GET,
    POST,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WcsTagConfig {
    pub json_path: String,
    pub alias: String,
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub value_type: WcsValueType,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WcsValueType {
    #[default]
    Bool,
    Int,
    Float,
    Text,
}

impl WcsConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.base_url.trim().is_empty() {
            return invalid("wcs.base_url must not be empty");
        }
        if self.poll_interval_ms == 0 {
            return invalid("wcs.poll_interval_ms must be greater than 0");
        }
        if self.timeout_ms == 0 {
            return invalid("wcs.timeout_ms must be greater than 0");
        }
        if self.endpoints.is_empty() {
            return invalid("wcs.endpoints must not be empty");
        }
        for endpoint in &self.endpoints {
            endpoint.validate()?;
        }
        Ok(())
    }
}

impl WcsEndpointConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.path.trim().is_empty() {
            return invalid("wcs endpoint path must not be empty");
        }
        if self.tags.is_empty() {
            return invalid(format!("wcs endpoint {} tags must not be empty", self.path));
        }
        for tag in &self.tags {
            if tag.json_path.trim().is_empty() {
                return invalid(format!(
                    "wcs endpoint {} tag json_path must not be empty",
                    self.path
                ));
            }
            if tag.alias.trim().is_empty() {
                return invalid(format!(
                    "wcs endpoint {} tag alias must not be empty",
                    self.path
                ));
            }
        }
        Ok(())
    }
}

fn invalid<T>(message: impl Into<String>) -> Result<T, ConfigError> {
    Err(ConfigError::Invalid(message.into()))
}

fn config_relative_path(base_dir: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    }
}

fn validate_unique_subscription_names(
    subscriptions: &[SubscriptionConfig],
) -> Result<(), ConfigError> {
    let mut names = HashSet::new();
    for subscription in subscriptions {
        if !names.insert(subscription.name.as_str()) {
            return invalid(format!(
                "subscription name {} is duplicated",
                subscription.name
            ));
        }
    }
    Ok(())
}

fn default_retry_limit() -> i32 {
    -1
}

fn default_mysql_connections() -> u32 {
    8
}

fn default_monitored_item_create_batch_size_count() -> usize {
    500
}

fn default_keep_alive_count() -> u32 {
    10
}

fn default_lifetime_count() -> u32 {
    30
}

fn default_discovery_target_subscription() -> String {
    "fast".to_string()
}

fn default_discovery_min_namespace_index() -> u16 {
    2
}

fn default_discovery_max_depth_count() -> u32 {
    12
}

fn default_discovery_max_tags_count() -> usize {
    500
}

fn validate_discovery_paths(field: &str, paths: &[String]) -> Result<(), ConfigError> {
    for path in paths {
        if path.trim().is_empty() {
            return invalid(format!(
                "opcua.discovery.{field} must not contain empty values"
            ));
        }
    }
    Ok(())
}

fn default_wcs_poll_interval_ms() -> u64 {
    5000
}

fn default_wcs_timeout_ms() -> u64 {
    10_000
}

fn default_wcs_retry_interval_ms() -> u64 {
    5000
}
