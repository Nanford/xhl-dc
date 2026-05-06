use chrono::{TimeZone, Utc};
use kepware_bridge::sink::{build_insert_sql, validate_mysql_identifier, BatchBuilder};
use kepware_bridge::types::{TagSample, ValueKind};

fn sample(alias: &str, value: f64) -> TagSample {
    let ts = Utc.with_ymd_and_hms(2026, 5, 6, 12, 0, 0).unwrap();
    TagSample {
        node_id: format!("ns=2;s=Channel1.Device1.{alias}"),
        alias: alias.to_string(),
        value: ValueKind::Float(value),
        source_ts: ts,
        server_ts: ts,
        quality: 0,
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
    let sql = build_insert_sql("tag_log", 2).expect("sql should build");

    assert!(sql.starts_with("INSERT INTO `tag_log`"));
    assert_eq!(sql.matches("(?, ?, ?, ?, ?, ?, ?, ?)").count(), 2);
}
