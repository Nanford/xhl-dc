use kepware_bridge::config::WcsValueType;
use kepware_bridge::types::ValueKind;
use kepware_bridge::wcs_client::extract_value;
use serde_json::json;

#[test]
fn extracts_bool_value() {
    let body = json!({"running": true});
    let result = extract_value(&body, "running", WcsValueType::Bool).unwrap();
    assert_eq!(result, ValueKind::Bool(true));
}

#[test]
fn extracts_int_value() {
    let body = json!({"error_code": 42});
    let result = extract_value(&body, "error_code", WcsValueType::Int).unwrap();
    assert_eq!(result, ValueKind::Int(42));
}

#[test]
fn extracts_float_value() {
    let body = json!({"speed": 12.5});
    let result = extract_value(&body, "speed", WcsValueType::Float).unwrap();
    assert_eq!(result, ValueKind::Float(12.5));
}

#[test]
fn extracts_text_value() {
    let body = json!({"status": "OK"});
    let result = extract_value(&body, "status", WcsValueType::Text).unwrap();
    assert_eq!(result, ValueKind::Text("OK".to_string()));
}

#[test]
fn extracts_nested_dot_path() {
    let body = json!({"data": {"readings": {"temp": 25.3}}});
    let result = extract_value(&body, "data.readings.temp", WcsValueType::Float).unwrap();
    assert_eq!(result, ValueKind::Float(25.3));
}

#[test]
fn rejects_missing_field() {
    let body = json!({"speed": 10.0});
    let err = extract_value(&body, "missing", WcsValueType::Float)
        .expect_err("missing field should fail");
    assert!(err.to_string().contains("not found"));
}

#[test]
fn rejects_type_mismatch() {
    let body = json!({"speed": "not_a_number"});
    let err = extract_value(&body, "speed", WcsValueType::Float)
        .expect_err("type mismatch should fail");
    assert!(err.to_string().contains("expected float"));
}
