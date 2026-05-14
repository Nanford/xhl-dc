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
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub description: String,
    pub value: ValueKind,
    pub source_ts: ChronoDateTime<Utc>,
    pub server_ts: ChronoDateTime<Utc>,
    pub quality: u32,
    #[serde(default = "default_source")]
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlarmLogFields {
    pub location: String,
    pub device: String,
    pub device_id: String,
    pub tag: String,
    pub description: String,
    pub remark: String,
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
        device_id: impl Into<String>,
        description: impl Into<String>,
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
            device_id: device_id.into(),
            description: description.into(),
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
        device_id: impl Into<String>,
        description: impl Into<String>,
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
            device_id: device_id.into(),
            description: description.into(),
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

    pub fn tag_name(&self) -> &str {
        opcua_string_node_id(&self.node_id).unwrap_or(self.alias.as_str())
    }

    pub fn tag_prefix(&self) -> Option<&str> {
        self.tag_name()
            .split('.')
            .next()
            .map(str::trim)
            .filter(|prefix| !prefix.is_empty())
    }

    pub fn alarm_log_fields(&self) -> AlarmLogFields {
        let parsed = parse_alarm_log_fields(self.tag_name());
        let description = first_non_empty(self.description.as_str(), self.tag_name(), "");
        AlarmLogFields {
            location: first_non_empty(&parsed.location, self.area.as_str(), ""),
            device: first_non_empty(&parsed.device, self.device.as_str(), ""),
            device_id: first_non_empty(&parsed.device_id, self.device_id.as_str(), ""),
            tag: first_non_empty(&parsed.tag, self.alias.as_str(), self.tag_name()),
            remark: self.alarm_log_remark(&description),
            description,
        }
    }

    pub fn tag_value(&self) -> String {
        if let Some(flag) = self.fsc_description_alarm_flag() {
            return flag.to_string();
        }

        match self.raw_numeric_value() {
            Some(value) if value == 0.0 => "0".to_string(),
            Some(_) => "1".to_string(),
            None => {
                if self.raw_value_string().trim().is_empty() {
                    "0".to_string()
                } else {
                    "1".to_string()
                }
            }
        }
    }

    pub fn tag_state(&self) -> String {
        self.raw_value_string()
    }

    fn raw_value_string(&self) -> String {
        match &self.value {
            ValueKind::Bool(value) => value.to_string(),
            ValueKind::Int(value) => value.to_string(),
            ValueKind::Float(value) => value.to_string(),
            ValueKind::Text(value) => value.clone(),
        }
    }

    fn raw_numeric_value(&self) -> Option<f64> {
        match &self.value {
            ValueKind::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            ValueKind::Int(value) => Some(*value as f64),
            ValueKind::Float(value) => Some(*value),
            ValueKind::Text(value) => parse_text_numeric_value(value),
        }
    }

    fn fsc_description_alarm_flag(&self) -> Option<u8> {
        let label = self.fsc_status_label()?;
        if is_non_alarm_status_label(label.trim()) {
            Some(0)
        } else {
            Some(1)
        }
    }

    fn alarm_log_remark(&self, description: &str) -> String {
        if let Some(label) = self.fsc_status_label() {
            if let Some(prefix) = description_status_prefix(description) {
                return format!("{prefix}{}", label.trim());
            }
        }
        description.to_string()
    }

    fn fsc_status_label(&self) -> Option<String> {
        let prefix = self.tag_prefix()?;
        if !(prefix.eq_ignore_ascii_case("FSC1") || prefix.eq_ignore_ascii_case("FSC2")) {
            return None;
        }

        let status_code = self.status_code_string();
        description_label_for_code(&self.description, status_code.trim())
    }

    fn status_code_string(&self) -> String {
        match &self.value {
            ValueKind::Bool(value) => {
                if *value {
                    "1".to_string()
                } else {
                    "0".to_string()
                }
            }
            ValueKind::Text(value) if value.trim().eq_ignore_ascii_case("true") => "1".to_string(),
            ValueKind::Text(value) if value.trim().eq_ignore_ascii_case("false") => "0".to_string(),
            _ => self.raw_value_string(),
        }
    }
}

fn opcua_string_node_id(node_id: &str) -> Option<&str> {
    node_id
        .split_once(";s=")
        .map(|(_, tag)| tag)
        .filter(|tag| !tag.trim().is_empty())
}

struct ParsedAlarmLogFields {
    location: String,
    device: String,
    device_id: String,
    tag: String,
}

fn parse_alarm_log_fields(tag_name: &str) -> ParsedAlarmLogFields {
    let segments = tag_name
        .split('.')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let location = segments.first().copied().unwrap_or_default().to_string();

    if let Some(alarm_index) = segments
        .iter()
        .position(|segment| segment.eq_ignore_ascii_case("Alarm"))
    {
        let equipment = if alarm_index > 2 {
            &segments[2..alarm_index]
        } else {
            &[][..]
        };
        let (device, device_id) = equipment_device(equipment);
        return ParsedAlarmLogFields {
            location,
            device: device.to_string(),
            device_id: device_id.to_string(),
            tag: join_segments(&segments[alarm_index + 1..], tag_name),
        };
    }

    if segments.len() >= 4 && segments[2].eq_ignore_ascii_case("Iscs") {
        let device = segments[3];
        return ParsedAlarmLogFields {
            location,
            device: device.to_string(),
            device_id: device_id_from_equipment(device).to_string(),
            tag: join_segments(&segments[2..], tag_name),
        };
    }

    if segments.len() >= 3 {
        let device = segments[2];
        return ParsedAlarmLogFields {
            location,
            device: device.to_string(),
            device_id: device_id_from_equipment(device).to_string(),
            tag: join_segments(&segments[2..], tag_name),
        };
    }

    ParsedAlarmLogFields {
        location,
        device: segments.get(1).copied().unwrap_or_default().to_string(),
        device_id: segments.get(1).copied().unwrap_or_default().to_string(),
        tag: tag_name.to_string(),
    }
}

fn equipment_device<'a>(equipment: &'a [&'a str]) -> (&'a str, &'a str) {
    match equipment {
        [] => ("", ""),
        [single] => (single, device_id_from_equipment(single)),
        [device, rest @ ..] => (device, rest.last().copied().unwrap_or(device)),
    }
}

fn device_id_from_equipment(equipment: &str) -> &str {
    let Some((_, rest)) = equipment.split_once('-') else {
        return equipment;
    };
    let id = rest.split('-').next().unwrap_or(rest);
    if id.is_empty() {
        equipment
    } else {
        id
    }
}

fn join_segments(segments: &[&str], fallback: &str) -> String {
    if segments.is_empty() {
        fallback.to_string()
    } else {
        segments.join(".")
    }
}

fn first_non_empty(first: &str, second: &str, fallback: &str) -> String {
    for value in [first, second, fallback] {
        if !value.trim().is_empty() {
            return value.to_string();
        }
    }
    String::new()
}

fn parse_text_numeric_value(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("true") {
        return Some(1.0);
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return Some(0.0);
    }
    trimmed.parse::<f64>().ok()
}

// FSC status descriptions encode value maps like `0、正常；1、错误；8192、运行中；`.
fn description_status_prefix(description: &str) -> Option<&str> {
    description
        .split_once(['：', ':'])
        .map(|(prefix, _)| prefix.trim())
        .filter(|prefix| !prefix.is_empty())
}

fn description_label_for_code(description: &str, code: &str) -> Option<String> {
    for segment in description.split(['；', ';']) {
        let mut search_start = 0;
        while search_start < segment.len() {
            let Some((relative_start, _)) = segment[search_start..]
                .char_indices()
                .find(|(_, ch)| ch.is_ascii_digit())
            else {
                break;
            };
            let digit_start = search_start + relative_start;
            let digit_end = digit_run_end(segment, digit_start);
            let Some((delimiter_start, delimiter, delimiter_end)) =
                next_non_whitespace_char(segment, digit_end)
            else {
                break;
            };

            if is_status_code_delimiter(delimiter) {
                if &segment[digit_start..digit_end] == code {
                    let label = trim_status_label(&segment[delimiter_end..]);
                    if !label.is_empty() {
                        return Some(label.to_string());
                    }
                }
                search_start = delimiter_end;
            } else {
                search_start = delimiter_start + delimiter.len_utf8();
            }
        }
    }
    None
}

fn digit_run_end(value: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in value[start..].char_indices() {
        if ch.is_ascii_digit() {
            end = start + offset + ch.len_utf8();
        } else {
            break;
        }
    }
    end
}

fn next_non_whitespace_char(value: &str, start: usize) -> Option<(usize, char, usize)> {
    value[start..].char_indices().find_map(|(offset, ch)| {
        if ch.is_whitespace() {
            None
        } else {
            let absolute = start + offset;
            Some((absolute, ch, absolute + ch.len_utf8()))
        }
    })
}

fn is_status_code_delimiter(ch: char) -> bool {
    matches!(ch, '、' | ':' | '：' | ',')
}

fn trim_status_label(value: &str) -> &str {
    value.trim_matches(|ch: char| {
        ch.is_whitespace() || matches!(ch, '；' | ';' | '，' | ',' | '。')
    })
}

fn is_non_alarm_status_label(label: &str) -> bool {
    matches!(label, "正常" | "运行中")
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
