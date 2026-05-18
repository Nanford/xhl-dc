use chrono::{NaiveDate, TimeZone, Utc};
use kepware_bridge::sink::{
    alarm_log_mysql_datetime, alarm_log_mysql_timestamps, build_insert_sql,
    validate_mysql_identifier, BatchBuilder, SinkTableRouter,
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

fn sample_with_description(
    node_id: &str,
    alias: &str,
    value: ValueKind,
    description: &str,
) -> TagSample {
    let mut sample = sample_with_node_id(node_id, alias, value);
    sample.description = description.to_string();
    sample
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
        "(`location`, `device`, `device_id`, `node_id`, `alias`, `tag`, `fault_type`, `tag_state`, `tag_value`, `description`, `remark`, `create_at`, `update_at`)"
    ));
    assert_eq!(
        sql.matches("(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .count(),
        2
    );
}

#[test]
fn converts_utc_sample_timestamp_to_beijing_mysql_datetime() {
    let ts = Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();

    let mysql_time = alarm_log_mysql_datetime(ts);

    assert_eq!(
        mysql_time,
        NaiveDate::from_ymd_opt(2026, 5, 6)
            .unwrap()
            .and_hms_opt(20, 0, 0)
            .unwrap()
    );
}

#[test]
fn uses_system_time_for_create_at_and_source_timestamp_for_update_at() {
    let source_ts = Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();
    let system_now = Utc.with_ymd_and_hms(2026, 5, 6, 13, 30, 0).unwrap();

    let (create_at, update_at) = alarm_log_mysql_timestamps(source_ts, system_now);

    assert_eq!(
        create_at,
        NaiveDate::from_ymd_opt(2026, 5, 6)
            .unwrap()
            .and_hms_opt(21, 30, 0)
            .unwrap()
    );
    assert_eq!(
        update_at,
        NaiveDate::from_ymd_opt(2026, 5, 6)
            .unwrap()
            .and_hms_opt(20, 0, 0)
            .unwrap()
    );
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
    assert_eq!(
        fields.remark,
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

    assert_eq!(active.tag_state(), "true");
    assert_eq!(active.tag_value(), "1");
    assert_eq!(inactive.tag_state(), "0");
    assert_eq!(inactive.tag_value(), "0");
}

#[test]
fn parses_fsc_numeric_descriptions_into_alarm_flags() {
    let normal = sample_with_description(
        "ns=2;s=FSC2.InBound.Iscs.BF-1_1_1.MTR-1_1_1_MTR.Details.DS",
        "FSC2.InBound.Iscs.BF-1_1_1.MTR-1_1_1_MTR.Details.DS",
        ValueKind::Int(0),
        "皮带输送机BF-1.1.1电机状态：0、正常；1、错误；",
    );
    let error = sample_with_description(
        "ns=2;s=FSC2.InBound.Iscs.BF-1_1_1.MTR-1_1_1_MTR.Details.DS",
        "FSC2.InBound.Iscs.BF-1_1_1.MTR-1_1_1_MTR.Details.DS",
        ValueKind::Int(1),
        "皮带输送机BF-1.1.1电机状态：0、正常；1、错误；",
    );
    let warning = sample_with_description(
        "ns=2;s=FSC2.InBound.Iscs.BF-3_1_7.PPI-3_1_7_PPI.Details.DS",
        "FSC2.InBound.Iscs.BF-3_1_7.PPI-3_1_7_PPI.Details.DS",
        ValueKind::Int(2),
        "皮带输送机BF-3.1.7PPI状态：0、正常；1、错误；2、警告；",
    );
    let running = sample_with_description(
        "ns=2;s=FSC2.InBound.Iscs.BF-1_5_3.BSC-1_5_3_BSC.GS",
        "FSC2.InBound.Iscs.BF-1_5_3.BSC-1_5_3_BSC.GS",
        ValueKind::Int(8192),
        "皮带输送机BF-1.5.3扫描仪状态：512、离线；1024、错误；8192、运行中；",
    );
    let offline = sample_with_description(
        "ns=2;s=FSC2.InBound.Iscs.BF-1_5_3.BSC-1_5_3_BSC.GS",
        "FSC2.InBound.Iscs.BF-1_5_3.BSC-1_5_3_BSC.GS",
        ValueKind::Int(512),
        "皮带输送机BF-1.5.3扫描仪状态：512、离线；1024、错误；8192、运行中；",
    );

    assert_eq!(normal.tag_state(), "0");
    assert_eq!(normal.tag_value(), "0");
    assert_eq!(error.tag_state(), "1");
    assert_eq!(error.tag_value(), "1");
    assert_eq!(warning.tag_state(), "2");
    assert_eq!(warning.tag_value(), "1");
    assert_eq!(running.tag_state(), "8192");
    assert_eq!(running.tag_value(), "0");
    assert_eq!(offline.tag_state(), "512");
    assert_eq!(offline.tag_value(), "1");
}

#[test]
fn renders_alarm_log_remark_from_fsc_description_code_map() {
    let normal = sample_with_description(
        "ns=2;s=FSC2.InBound.Iscs.BF-1_1_1.MTR-1_1_1_MTR.Details.DS",
        "FSC2.InBound.Iscs.BF-1_1_1.MTR-1_1_1_MTR.Details.DS",
        ValueKind::Bool(false),
        "皮带输送机BF-1.1.1电机状态：0、正常；1、错误；",
    );
    let error = sample_with_description(
        "ns=2;s=FSC2.InBound.Iscs.LRJ-3_12_1.MTR-3_12_1_MTR.Details.DS",
        "FSC2.InBound.Iscs.LRJ-3_12_1.MTR-3_12_1_MTR.Details.DS",
        ValueKind::Int(1),
        "滚筒汇流机LRJ-3.12.1电机状态：0、正常；1、错误；",
    );
    let running = sample_with_description(
        "ns=2;s=FSC2.InBound.Iscs.BF-1_5_3.BSC-1_5_3_BSC.GS",
        "FSC2.InBound.Iscs.BF-1_5_3.BSC-1_5_3_BSC.GS",
        ValueKind::Int(8192),
        "皮带输送机BF-1.5.3扫描仪状态：512、离线；1024、错误；8192、运行中；",
    );
    let warning = sample_with_description(
        "ns=2;s=FSC2.InBound.Iscs.BF-3_1_7.PPI-3_1_7_PPI.Details.DS",
        "FSC2.InBound.Iscs.BF-3_1_7.PPI-3_1_7_PPI.Details.DS",
        ValueKind::Int(2),
        "皮带输送机BF-3.1.7PPI状态：0、正常；1、错误；2、警告；",
    );

    assert_eq!(
        normal.alarm_log_fields().remark,
        "皮带输送机BF-1.1.1电机状态正常"
    );
    assert_eq!(
        error.alarm_log_fields().remark,
        "滚筒汇流机LRJ-3.12.1电机状态错误"
    );
    assert_eq!(
        running.alarm_log_fields().remark,
        "皮带输送机BF-1.5.3扫描仪状态运行中"
    );
    assert_eq!(
        warning.alarm_log_fields().remark,
        "皮带输送机BF-3.1.7PPI状态警告"
    );
}
