use std::fs;
use std::net::SocketAddr;
use std::path::Path;
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
    #[error("invalid config: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub opcua: OpcuaConfig,
    pub mysql: MysqlConfig,
    pub subscriptions: Vec<SubscriptionConfig>,
    pub sink: SinkConfig,
    pub buffer: BufferConfig,
    pub metrics: MetricsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpcuaConfig {
    pub endpoint: String,
    pub security_policy: SecurityPolicyConfig,
    pub identity: IdentityConfig,
    #[serde(default = "default_retry_limit")]
    pub session_retry_limit: i32,
    pub application_uri: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SinkConfig {
    pub table: String,
    pub batch_size: usize,
    pub flush_interval_ms: u64,
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
        Self::from_yaml_str(&content)
    }

    pub fn from_yaml_str(content: &str) -> Result<Self, ConfigError> {
        let config: Self = serde_yaml::from_str(content.trim_start_matches('\u{feff}'))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.opcua.endpoint.trim().is_empty() {
            return invalid("opcua.endpoint must not be empty");
        }
        if self.opcua.application_uri.trim().is_empty() {
            return invalid("opcua.application_uri must not be empty");
        }
        if self.opcua.session_retry_limit < -1 {
            return invalid("opcua.session_retry_limit must be -1 or greater");
        }
        self.opcua.identity.validate()?;

        if self.mysql.url.trim().is_empty() {
            return invalid("mysql.url must not be empty");
        }
        if self.mysql.max_connections == 0 {
            return invalid("mysql.max_connections must be greater than 0");
        }

        if self.subscriptions.is_empty() {
            return invalid("subscriptions must not be empty");
        }
        for subscription in &self.subscriptions {
            subscription.validate()?;
        }

        validate_mysql_identifier(&self.sink.table)
            .map_err(|err| ConfigError::Invalid(format!("sink.table: {err}")))?;
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

fn invalid<T>(message: impl Into<String>) -> Result<T, ConfigError> {
    Err(ConfigError::Invalid(message.into()))
}

fn default_retry_limit() -> i32 {
    -1
}

fn default_mysql_connections() -> u32 {
    8
}

fn default_keep_alive_count() -> u32 {
    10
}

fn default_lifetime_count() -> u32 {
    30
}
