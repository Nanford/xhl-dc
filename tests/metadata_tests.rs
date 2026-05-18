use chrono::{TimeZone, Utc};
use kepware_bridge::metadata::TagMetadataCache;
use kepware_bridge::types::{TagSample, ValueKind};

fn sample(node_id: &str, alias: &str) -> TagSample {
    let ts = Utc.with_ymd_and_hms(2026, 5, 18, 10, 0, 0).unwrap();
    TagSample {
        node_id: node_id.to_string(),
        alias: alias.to_string(),
        area: String::new(),
        device: String::new(),
        device_id: String::new(),
        description: String::new(),
        value: ValueKind::Bool(true),
        source_ts: ts,
        server_ts: ts,
        quality: 0,
        source: "opcua".to_string(),
    }
}

#[test]
fn matches_metadata_by_node_id_first() {
    let cache = TagMetadataCache::from_rows([
        (
            Some("ns=2;s=WH_CP_Zone01.Convey.Conveyor.M5035.Alarm.DriveFault".to_string()),
            Some("other_alias".to_string()),
            "OtherTag".to_string(),
            Some("驱动故障".to_string()),
        ),
        (
            None,
            Some("WH_CP_Zone01.Convey.Conveyor.M5035.Alarm.DriveFault".to_string()),
            "DriveFault".to_string(),
            Some("货叉".to_string()),
        ),
    ]);
    let sample = sample(
        "ns=2;s=WH_CP_Zone01.Convey.Conveyor.M5035.Alarm.DriveFault",
        "WH_CP_Zone01.Convey.Conveyor.M5035.Alarm.DriveFault",
    );
    let fields = sample.alarm_log_fields();

    let metadata = cache.lookup(&sample, &fields).unwrap();

    assert_eq!(metadata.fault_type.as_deref(), Some("驱动故障"));
}

#[test]
fn matches_metadata_by_parsed_tag_when_node_and_alias_are_missing() {
    let cache = TagMetadataCache::from_rows([(
        None,
        None,
        "DriveFault".to_string(),
        Some("驱动故障".to_string()),
    )]);
    let sample = sample(
        "ns=2;s=WH_CP_Zone01.Convey.Conveyor.M5035.Alarm.DriveFault",
        "unmatched_alias",
    );
    let fields = sample.alarm_log_fields();

    let metadata = cache.lookup(&sample, &fields).unwrap();

    assert_eq!(metadata.fault_type.as_deref(), Some("驱动故障"));
}
