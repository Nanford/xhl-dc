SET GLOBAL event_scheduler = ON;

INSERT INTO `alarm_archive_tables` (`source_table`, `enabled`)
VALUES
  ('cpk_alarm_log', 1),
  ('flk_alarm_log', 1),
  ('ylk_alarm_log', 1)
ON DUPLICATE KEY UPDATE
  `enabled` = VALUES(`enabled`);

DROP EVENT IF EXISTS `ev_refresh_daily_alarm_stats`;

CREATE EVENT `ev_refresh_daily_alarm_stats`
ON SCHEDULE EVERY 1 DAY
STARTS (CURRENT_DATE + INTERVAL 1 DAY + INTERVAL 10 MINUTE)
ENABLE
DO
  CALL `sp_refresh_daily_fault_stats`(CURRENT_DATE - INTERVAL 1 DAY);

SELECT
  @@event_scheduler AS `event_scheduler`;

SELECT
  `EVENT_NAME`,
  `STATUS`,
  `STARTS`,
  `LAST_EXECUTED`
FROM INFORMATION_SCHEMA.EVENTS
WHERE `EVENT_SCHEMA` = DATABASE()
  AND `EVENT_NAME` = 'ev_refresh_daily_alarm_stats';

SELECT
  `ROUTINE_NAME`,
  `ROUTINE_TYPE`,
  `CREATED`,
  `LAST_ALTERED`
FROM INFORMATION_SCHEMA.ROUTINES
WHERE `ROUTINE_SCHEMA` = DATABASE()
  AND `ROUTINE_NAME` = 'sp_refresh_daily_fault_stats';

SELECT
  `source_table`,
  `enabled`
FROM `alarm_archive_tables`
ORDER BY `source_table`;
