use std::fs;

fn read_text(path: &str) -> String {
    let text = fs::read_to_string(path).unwrap_or_else(|err| panic!("failed to read {path}: {err}"));
    // SQL snapshots are text-matched below; normalize checkout-dependent CRLFs.
    text.replace("\r\n", "\n")
}

#[test]
fn migration_adds_realtime_fault_count_and_updates_wcs_proc() {
    let sql = read_text("migrations/202605200001_add_realtime_fault_count.sql");

    assert!(sql.contains("sp_add_column_if_missing"));
    assert!(sql.contains("'device_realtime_status'"));
    assert!(sql.contains("'fault_count'"));
    assert!(sql.contains("BIGINT UNSIGNED NOT NULL DEFAULT 0"));
    assert!(sql.contains("DROP PROCEDURE IF EXISTS `sp_process_wcs_alarm_event`"));
    assert!(sql.contains("CREATE PROCEDURE `sp_process_wcs_alarm_event`"));
}

#[test]
fn wcs_realtime_insert_counts_fault_edges_before_state_update() {
    let sql = read_text("migrations/202605200001_add_realtime_fault_count.sql");

    assert!(
        sql.contains("`fault_count`,\n     `description`, `status_description`, `last_fault_at`")
    );
    assert!(sql.contains(
        "CASE WHEN COALESCE(NULLIF(TRIM(v_tag_value), ''), '0') <> '0' THEN 1 ELSE 0 END"
    ));
    assert!(sql.contains(
        "`fault_count` = IF(COALESCE(NULLIF(TRIM(`tag_value`), ''), '0') = '0' AND COALESCE(NULLIF(TRIM(VALUES(`tag_value`)), ''), '0') <> '0', `fault_count` + 1, `fault_count`)"
    ));
    assert!(
        sql.find("`fault_count` = IF")
            .expect("fault_count update should exist")
            < sql
                .find("`tag_value` = VALUES(`tag_value`)")
                .expect("tag_value update should exist")
    );
}

#[test]
fn stats_diagnostic_sql_checks_scheduler_events_history_and_stats() {
    let sql = read_text("sql/diagnose_alarm_stats.sql");

    assert!(sql.contains("@@event_scheduler"));
    assert!(sql.contains("INFORMATION_SCHEMA.EVENTS"));
    assert!(sql.contains("ev_refresh_daily_alarm_stats"));
    assert!(sql.contains("cpk_alarm_log"));
    assert!(sql.contains("flk_alarm_log"));
    assert!(sql.contains("ylk_alarm_log"));
    assert!(sql.contains("daily_area_fault_stats"));
    assert!(sql.contains("daily_device_type_fault_stats"));
    assert!(sql.contains("daily_fault_type_stats"));
}

#[test]
fn stats_event_sql_enables_daily_refresh_for_three_stats_tables() {
    let sql = read_text("sql/ensure_alarm_stats_events.sql");

    assert!(sql.contains("SET GLOBAL event_scheduler = ON"));
    assert!(sql.contains("ev_refresh_daily_alarm_stats"));
    assert!(sql.contains("CALL `sp_refresh_daily_fault_stats`"));
    assert!(sql.contains("cpk_alarm_log"));
    assert!(sql.contains("flk_alarm_log"));
    assert!(sql.contains("ylk_alarm_log"));
}

#[test]
fn field_value_catalog_migration_seeds_dimensions_and_views() {
    let sql = read_text("migrations/202605210001_add_field_value_catalog.sql");

    assert!(sql.contains("CREATE TABLE IF NOT EXISTS `field_value_catalog`"));
    assert!(sql.contains("UNIQUE KEY `uq_field_value_catalog` (`field_name`, `field_code`)"));
    assert!(sql.contains("('location', 'WH_CP_Zone03', '成品库（德玛分拣02）', 50)"));
    assert!(sql.contains("('device_type', 'GROSSING13', '分拣输送线', 113)"));
    assert!(sql.contains("('device_type', 'RGV', '穿梭车', 180)"));
    assert!(sql.contains("CREATE OR REPLACE VIEW `cpk_alarm_log_enriched`"));
    assert!(sql.contains("CREATE OR REPLACE VIEW `flk_alarm_log_enriched`"));
    assert!(sql.contains("CREATE OR REPLACE VIEW `ylk_alarm_log_enriched`"));
    assert!(sql.contains("CREATE OR REPLACE VIEW `device_realtime_status_enriched`"));
    assert!(sql.contains("loc.`field_name` = 'location'"));
    assert!(sql.contains("dev.`field_name` = 'device_type'"));
}
