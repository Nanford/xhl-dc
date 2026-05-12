use chrono::{TimeZone, Utc};
use kepware_bridge::sink::{
    build_insert_sql, validate_mysql_identifier, BatchBuilder, SinkTableRouter,
};
use kepware_bridge::types::{TagSample, ValueKind};

fn sample(alias: &str, value: f64) -> TagSample {
    sample_with_node_id(
        &format!("ns=2;s=Channel1.Device1.{alias}"),
        alias,
        ValueKind::Float(value),
    )
}

fn sample_with_node_id(node_id: &str, alias: &str, value: ValueKind) -> TagSample {
    let ts = Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();
    TagSample {
        node_id: node_id.to_string(),
        alias: alias.to_string(),
        area: String::new(),
        device: String::new(),
        device_id: String::new(),
        description: String::new(),
        value,
        source_ts: ts,
        server_ts: ts,
        quality: 0,
        source: "opcua".to_string(),
    }
}

#[test]
fn batch_builder_emits_full_batches() {
    let mut batcher = BatchBuilder::new(2);

    assert!(batcher.push(sample("Temperature", 10.0)).is_none());
    let full = batcher
        .push(sample("Pressure", 20.0))
        .expect("second sample should fill batch");

    assert_eq!(full.len(), 2);
    assert_eq!(batcher.len(), 0);
}

#[test]
fn batch_builder_flushes_partial_batch() {
    let mut batcher = BatchBuilder::new(10);
    batcher.push(sample("Temperature", 10.0));

    let flushed = batcher.flush();

    assert_eq!(flushed.len(), 1);
    assert_eq!(batcher.len(), 0);
}

#[test]
fn validates_mysql_table_identifier() {
    assert!(validate_mysql_identifier("tag_log_202605").is_ok());
    assert!(validate_mysql_identifier("tag-log").is_err());
    assert!(validate_mysql_identifier("tag_log;drop").is_err());
}

#[test]
fn builds_multi_row_insert_sql() {
    let sql = build_insert_sql("cpk_alarm_log", 2).expect("sql should build");

    assert!(sql.starts_with("INSERT INTO `cpk_alarm_log`"));
    assert!(sql.contains(
        "(`location`, `device`, `device_id`, `tag`, `tag_state`, `tag_value`, `description`, `create_at`, `update_at`)"
    ));
    assert_eq!(sql.matches("(?, ?, ?, ?, ?, ?, ?, ?, ?)").count(), 2);
}

#[test]
fn parses_wh_cp_alarm_tags_for_alarm_log_columns() {
    let sample = sample_with_node_id(
        "ns=2;s=WH_CP_Zone01.Convey.Conveyor.M5035.Alarm.DriveFault",
        "WH_CP_Zone01_Convey_Conveyor_M5035_Alarm_DriveFault",
        ValueKind::Bool(false),
    );
    let fields = sample.alarm_log_fields();

    assert_eq!(fields.location, "WH_CP_Zone01");
    assert_eq!(fields.device, "Conveyor");
    assert_eq!(fields.device_id, "M5035");
    assert_eq!(fields.tag, "DriveFault");
    assert_eq!(
        fields.description,
        "WH_CP_Zone01.Convey.Conveyor.M5035.Alarm.DriveFault"
    );
}

#[test]
fn parses_fsc_iscs_tags_for_alarm_log_columns() {
    let sample = sample_with_node_id(
        "ns=2;s=FSC2.InBound.Iscs.BF-1_1_1.MTR-1_1_1_MTR.Details.DS",
        "FSC2_InBound_Iscs_BF_1_1_1_MTR_1_1_1_MTR_Details_DS",
        ValueKind::Int(0),
    );
    let fields = sample.alarm_log_fields();

    assert_eq!(fields.location, "FSC2");
    assert_eq!(fields.device, "BF-1_1_1");
    assert_eq!(fields.device_id, "1_1_1");
    assert_eq!(fields.tag, "Iscs.BF-1_1_1.MTR-1_1_1_MTR.Details.DS");
}

#[test]
fn routes_samples_by_location_prefix() {
    let router = SinkTableRouter::from_routes(
        "ylk_alarm_log",
        [("FSC2", "cpk_alarm_log"), ("WH_CP_Zone01", "cpk_alarm_log")],
    )
    .expect("routes should be valid");
    let sample = sample_with_node_id(
        "ns=2;s=FSC2.InBound.Iscs.BF-1_1_1.MTR-1_1_1_MTR.Details.DS",
        "FSC2_InBound_Iscs_BF_1_1_1_MTR_1_1_1_MTR_Details_DS",
        ValueKind::Int(0),
    );

    assert_eq!(sample.tag_prefix(), Some("FSC2"));
    assert_eq!(router.table_for_sample(&sample), "cpk_alarm_log");
}

#[test]
fn renders_alarm_log_value_and_state() {
    let active = sample_with_node_id("ns=2;s=FSC1.Fault", "FSC1.Fault", ValueKind::Bool(true));
    let inactive = sample_with_node_id("ns=2;s=FSC1.Fault", "FSC1.Fault", ValueKind::Int(0));

    assert_eq!(active.tag_value(), "true");
    assert_eq!(active.tag_state(), "active");
    assert_eq!(inactive.tag_value(), "0");
    assert_eq!(inactive.tag_state(), "inactive");
}
