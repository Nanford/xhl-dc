use kepware_bridge::types::{TagSample, ValueKind};
use opcua::types::{DataValue, DateTime, StatusCode, Variant};

fn data_value(value: Variant) -> DataValue {
    let ts = DateTime::ymd_hms_nano(2026, 5, 6, 12, 0, 1, 123_000_000);
    DataValue {
        value: Some(value),
        status: Some(StatusCode::Good),
        source_timestamp: Some(ts),
        source_picoseconds: Some(0),
        server_timestamp: Some(ts),
        server_picoseconds: Some(0),
    }
}

#[test]
fn converts_float_variant_to_numeric_sample() {
    let sample = TagSample::from_data_value(
        "ns=2;s=Channel1.Device1.Temperature",
        "temp_1",
        "原料库",
        "SSJ",
        data_value(Variant::Double(12.5)),
    )
    .expect("double values should be supported");

    assert_eq!(sample.value, ValueKind::Float(12.5));
    assert_eq!(sample.value_type_code(), 2);
    assert_eq!(sample.value_num(), Some(12.5));
    assert_eq!(sample.value_str(), None);
    assert_eq!(sample.quality, StatusCode::Good.bits());
    assert_eq!(sample.source, "opcua");
    assert_eq!(sample.area, "原料库");
    assert_eq!(sample.device, "SSJ");
}

#[test]
fn converts_bool_variant_to_bool_sample() {
    let sample = TagSample::from_data_value(
        "ns=2;s=Channel1.Device1.Running",
        "running_1",
        "",
        "",
        data_value(Variant::Boolean(true)),
    )
    .expect("bool values should be supported");

    assert_eq!(sample.value, ValueKind::Bool(true));
    assert_eq!(sample.value_type_code(), 0);
    assert_eq!(sample.value_num(), Some(1.0));
}

#[test]
fn rejects_empty_variant() {
    let err = TagSample::from_data_value(
        "ns=2;s=Channel1.Device1.Empty",
        "empty_1",
        "",
        "",
        data_value(Variant::Empty),
    )
    .expect_err("empty values should not be inserted");

    assert!(err.to_string().contains("unsupported"));
}
