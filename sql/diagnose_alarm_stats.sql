SET @lookback_days := 3;

SELECT
  DATABASE() AS `database_name`,
  NOW(3) AS `checked_at`,
  @@event_scheduler AS `event_scheduler`;

SELECT
  `EVENT_NAME`,
  `STATUS`,
  `STARTS`,
  `LAST_EXECUTED`,
  `LAST_ALTERED`
FROM INFORMATION_SCHEMA.EVENTS
WHERE `EVENT_SCHEMA` = DATABASE()
  AND `EVENT_NAME` IN (
    'ev_refresh_daily_alarm_stats',
    'ev_archive_alarm_month',
    'ev_process_wcs_alarm_inbox'
  )
ORDER BY `EVENT_NAME`;

SELECT
  `source_table`,
  `enabled`,
  `created_at`
FROM `alarm_archive_tables`
ORDER BY `source_table`;

SELECT
  'cpk_alarm_log' AS `source_table`,
  COUNT(*) AS `total_rows`,
  SUM(CASE WHEN `create_at` >= CURRENT_DATE - INTERVAL @lookback_days DAY THEN 1 ELSE 0 END) AS `recent_rows`,
  SUM(CASE WHEN `create_at` >= CURRENT_DATE - INTERVAL @lookback_days DAY AND COALESCE(NULLIF(TRIM(`tag_value`), ''), '0') <> '0' THEN 1 ELSE 0 END) AS `recent_fault_rows`,
  MIN(`create_at`) AS `first_create_at`,
  MAX(`create_at`) AS `last_create_at`
FROM `cpk_alarm_log`
UNION ALL
SELECT
  'flk_alarm_log',
  COUNT(*),
  SUM(CASE WHEN `create_at` >= CURRENT_DATE - INTERVAL @lookback_days DAY THEN 1 ELSE 0 END),
  SUM(CASE WHEN `create_at` >= CURRENT_DATE - INTERVAL @lookback_days DAY AND COALESCE(NULLIF(TRIM(`tag_value`), ''), '0') <> '0' THEN 1 ELSE 0 END),
  MIN(`create_at`),
  MAX(`create_at`)
FROM `flk_alarm_log`
UNION ALL
SELECT
  'ylk_alarm_log',
  COUNT(*),
  SUM(CASE WHEN `create_at` >= CURRENT_DATE - INTERVAL @lookback_days DAY THEN 1 ELSE 0 END),
  SUM(CASE WHEN `create_at` >= CURRENT_DATE - INTERVAL @lookback_days DAY AND COALESCE(NULLIF(TRIM(`tag_value`), ''), '0') <> '0' THEN 1 ELSE 0 END),
  MIN(`create_at`),
  MAX(`create_at`)
FROM `ylk_alarm_log`;

SELECT
  DATE(`create_at`) AS `stat_date`,
  `source_table`,
  COUNT(*) AS `rows`,
  SUM(CASE WHEN COALESCE(NULLIF(TRIM(`tag_value`), ''), '0') <> '0' THEN 1 ELSE 0 END) AS `fault_rows`
FROM (
  SELECT 'cpk_alarm_log' AS `source_table`, `create_at`, `tag_value` FROM `cpk_alarm_log`
  WHERE `create_at` >= CURRENT_DATE - INTERVAL @lookback_days DAY
  UNION ALL
  SELECT 'flk_alarm_log', `create_at`, `tag_value` FROM `flk_alarm_log`
  WHERE `create_at` >= CURRENT_DATE - INTERVAL @lookback_days DAY
  UNION ALL
  SELECT 'ylk_alarm_log', `create_at`, `tag_value` FROM `ylk_alarm_log`
  WHERE `create_at` >= CURRENT_DATE - INTERVAL @lookback_days DAY
) r
GROUP BY DATE(`create_at`), `source_table`
ORDER BY `stat_date`, `source_table`;

SELECT
  COUNT(*) AS `realtime_rows`,
  SUM(CASE WHEN COALESCE(NULLIF(TRIM(`tag_value`), ''), '0') <> '0' THEN 1 ELSE 0 END) AS `active_fault_rows`,
  SUM(`fault_count`) AS `fault_count_total`,
  MIN(`updated_at`) AS `first_updated_at`,
  MAX(`updated_at`) AS `last_updated_at`
FROM `device_realtime_status`;

SELECT
  'daily_area_fault_stats' AS `stats_table`,
  COUNT(*) AS `total_rows`,
  SUM(CASE WHEN `stat_date` >= CURRENT_DATE - INTERVAL @lookback_days DAY THEN 1 ELSE 0 END) AS `recent_rows`,
  SUM(CASE WHEN `stat_date` >= CURRENT_DATE - INTERVAL @lookback_days DAY THEN `fault_count` ELSE 0 END) AS `recent_fault_count`
FROM `daily_area_fault_stats`
UNION ALL
SELECT
  'daily_device_type_fault_stats',
  COUNT(*),
  SUM(CASE WHEN `stat_date` >= CURRENT_DATE - INTERVAL @lookback_days DAY THEN 1 ELSE 0 END),
  SUM(CASE WHEN `stat_date` >= CURRENT_DATE - INTERVAL @lookback_days DAY THEN `fault_count` ELSE 0 END)
FROM `daily_device_type_fault_stats`
UNION ALL
SELECT
  'daily_fault_type_stats',
  COUNT(*),
  SUM(CASE WHEN `stat_date` >= CURRENT_DATE - INTERVAL @lookback_days DAY THEN 1 ELSE 0 END),
  SUM(CASE WHEN `stat_date` >= CURRENT_DATE - INTERVAL @lookback_days DAY THEN `fault_count` ELSE 0 END)
FROM `daily_fault_type_stats`;

SELECT
  `id`,
  `archive_month`,
  `source_table`,
  `target_table`,
  `status`,
  `copied_rows`,
  `deleted_rows`,
  `started_at`,
  `finished_at`,
  `error_message`
FROM `alarm_archive_runs`
ORDER BY `started_at` DESC
LIMIT 20;
