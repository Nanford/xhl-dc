use chrono::{NaiveDate, NaiveDateTime};
use kepware_bridge::history::{
    archive_table_name, build_history_query, select_history_tables, AlarmHistoryFilter,
};

fn dt(value: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").unwrap()
}

#[test]
fn builds_archive_table_name_from_month() {
    let month = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();

    let table = archive_table_name("cpk_alarm_log", month).unwrap();

    assert_eq!(table, "cpk_alarm_log_202605");
}

#[test]
fn selects_archive_and_hot_tables_for_cross_month_query() {
    let current_month = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();

    let tables = select_history_tables(
        "cpk_alarm_log",
        dt("2026-04-20 00:00:00"),
        dt("2026-06-02 00:00:00"),
        current_month,
    )
    .unwrap();

    assert_eq!(
        tables,
        vec![
            "cpk_alarm_log_202604".to_string(),
            "cpk_alarm_log_202605".to_string(),
            "cpk_alarm_log".to_string()
        ]
    );
}

#[test]
fn end_boundary_at_month_start_does_not_select_next_month() {
    let current_month = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();

    let tables = select_history_tables(
        "flk_alarm_log",
        dt("2026-05-01 00:00:00"),
        dt("2026-06-01 00:00:00"),
        current_month,
    )
    .unwrap();

    assert_eq!(tables, vec!["flk_alarm_log_202605".to_string()]);
}

#[test]
fn builds_union_query_with_filters() {
    let current_month = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
    let filter = AlarmHistoryFilter {
        location: Some("成品库".to_string()),
        device_id: Some("SRM01".to_string()),
        fault_type: Some("货叉".to_string()),
        limit: Some(100),
        offset: Some(0),
        ..AlarmHistoryFilter::default()
    };

    let plan = build_history_query(
        "cpk_alarm_log",
        dt("2026-05-15 00:00:00"),
        dt("2026-06-02 00:00:00"),
        current_month,
        &filter,
    )
    .unwrap();

    assert_eq!(
        plan.tables,
        vec![
            "cpk_alarm_log_202605".to_string(),
            "cpk_alarm_log".to_string()
        ]
    );
    assert!(plan.sql.contains("UNION ALL"));
    assert!(plan.sql.contains("FROM `cpk_alarm_log_202605`"));
    assert!(plan.sql.contains("FROM `cpk_alarm_log`"));
    assert!(plan.sql.contains("AND location = ?"));
    assert!(plan.sql.contains("AND device_id = ?"));
    assert!(plan.sql.contains("AND fault_type = ?"));
    assert!(plan.sql.ends_with("LIMIT ? OFFSET ?"));
}

#[test]
fn rejects_invalid_history_table_identifier() {
    let current_month = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();

    let err = select_history_tables(
        "cpk_alarm_log;drop",
        dt("2026-05-01 00:00:00"),
        dt("2026-06-01 00:00:00"),
        current_month,
    )
    .expect_err("unsafe table name must be rejected");

    assert!(err.to_string().contains("invalid"));
}
