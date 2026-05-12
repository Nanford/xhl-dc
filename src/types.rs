use chrono::{DateTime as ChronoDateTime, Utc};
use opcua::types::{DataValue, DateTime, StatusCode, Variant};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TypeError {
    #[error("unsupported OPC UA variant for node_id {node_id}: {variant:?}")]
    UnsupportedVariant { node_id: String, variant: Variant },
    #[error("OPC UA data value for node_id {node_id} has no value")]
    MissingValue { node_id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ValueKind {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl ValueKind {
    pub fn value_type_code(&self) -> i8 {
        match self {
            ValueKind::Bool(_) => 0,
            ValueKind::Int(_) => 1,
            ValueKind::Float(_) => 2,
            ValueKind::Text(_) => 3,
        }
    }

    pub fn value_num(&self) -> Option<f64> {
        match self {
            ValueKind::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            ValueKind::Int(value) => Some(*value as f64),
            ValueKind::Float(value) => Some(*value),
            ValueKind::Text(_) => None,
        }
    }

    pub fn value_str(&self) -> Option<&str> {
        match self {
            ValueKind::Text(value) => Some(value.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TagSample {
    pub node_id: String,
    pub alias: String,
    #[serde(default)]
    pub area: String,
    #[serde(default)]
    pub device: String,
    pub value: ValueKind,
    pub source_ts: ChronoDateTime<Utc>,
    pub server_ts: ChronoDateTime<Utc>,
    pub quality: u32,
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_source() -> String {
    "opcua".to_string()
}

impl TagSample {
    pub fn from_data_value(
        node_id: impl Into<String>,
        alias: impl Into<String>,
        area: impl Into<String>,
        device: impl Into<String>,
        data_value: DataValue,
    ) -> Result<Self, TypeError> {
        let node_id = node_id.into();
        let alias = alias.into();
        let now = Utc::now();
        let source_ts = data_value
            .source_timestamp
            .map(opcua_datetime_to_chrono)
            .unwrap_or(now);
        let server_ts = data_value
            .server_timestamp
            .map(opcua_datetime_to_chrono)
            .unwrap_or(now);
        let quality = data_value.status.unwrap_or(StatusCode::Good).bits();
        let variant = data_value
            .value
            .ok_or_else(|| TypeError::MissingValue { node_id: node_id.clone() })?;
        let value = ValueKind::try_from_variant(&node_id, variant)?;

        Ok(Self {
            node_id,
            alias,
            area: area.into(),
            device: device.into(),
            value,
            source_ts,
            server_ts,
            quality,
            source: "opcua".to_string(),
        })
    }

    pub fn new(
        node_id: impl Into<String>,
        alias: impl Into<String>,
        area: impl Into<String>,
        device: impl Into<String>,
        value: ValueKind,
        source_ts: ChronoDateTime<Utc>,
        server_ts: ChronoDateTime<Utc>,
        quality: u32,
        source: impl Into<String>,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            alias: alias.into(),
            area: area.into(),
            device: device.into(),
            value,
            source_ts,
            server_ts,
            quality,
            source: source.into(),
        }
    }

    pub fn value_type_code(&self) -> i8 {
        self.value.value_type_code()
    }

    pub fn value_num(&self) -> Option<f64> {
        self.value.value_num()
    }

    pub fn value_str(&self) -> Option<&str> {
        self.value.value_str()
    }
}

impl ValueKind {
    fn try_from_variant(node_id: &str, variant: Variant) -> Result<Self, TypeError> {
        let unsupported = |variant| TypeError::UnsupportedVariant {
            node_id: node_id.to_string(),
            variant,
        };

        match variant {
            Variant::Boolean(value) => Ok(ValueKind::Bool(value)),
            Variant::SByte(value) => Ok(ValueKind::Int(value as i64)),
            Variant::Byte(value) => Ok(ValueKind::Int(value as i64)),
            Variant::Int16(value) => Ok(ValueKind::Int(value as i64)),
            Variant::UInt16(value) => Ok(ValueKind::Int(value as i64)),
            Variant::Int32(value) => Ok(ValueKind::Int(value as i64)),
            Variant::UInt32(value) => Ok(ValueKind::Int(value as i64)),
            Variant::Int64(value) => Ok(ValueKind::Int(value)),
            Variant::UInt64(value) if value <= i64::MAX as u64 => Ok(ValueKind::Int(value as i64)),
            Variant::Float(value) => Ok(ValueKind::Float(value as f64)),
            Variant::Double(value) => Ok(ValueKind::Float(value)),
            Variant::String(value) => Ok(ValueKind::Text(value.to_string())),
            Variant::ByteString(value) => Ok(ValueKind::Text(bytes_to_hex(
                value.value.as_deref().unwrap_or_default(),
            ))),
            other => Err(unsupported(other)),
        }
    }
}

fn opcua_datetime_to_chrono(value: DateTime) -> ChronoDateTime<Utc> {
    value.as_chrono()
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0F) as usize] as char);
    }
    out
}

