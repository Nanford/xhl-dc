SET NAMES utf8mb4;

DROP PROCEDURE IF EXISTS `sp_write_wcs_alarm_event`;

CREATE PROCEDURE `sp_write_wcs_alarm_event`(
  IN p_source_event_id VARCHAR(128),
  IN p_wcs_area_code VARCHAR(64),
  IN p_wcs_device_code VARCHAR(100),
  IN p_location VARCHAR(255),
  IN p_device_type VARCHAR(100),
  IN p_device_id VARCHAR(100),
  IN p_device_name VARCHAR(255),
  IN p_device_vendor VARCHAR(100),
  IN p_fault_code VARCHAR(64),
  IN p_fault_value VARCHAR(255),
  IN p_is_active TINYINT,
  IN p_fault_at DATETIME(3),
  IN p_raw_payload LONGTEXT
)
BEGIN
  DECLARE v_inbox_id BIGINT UNSIGNED;

  IF p_fault_code IS NULL OR TRIM(p_fault_code) = '' THEN
    SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'p_fault_code must not be empty';
  END IF;

  INSERT INTO `wcs_alarm_event_inbox`
    (`source_event_id`, `wcs_area_code`, `wcs_device_code`, `location`, `device_type`,
     `device_id`, `device_name`, `device_vendor`, `fault_code`, `fault_value`,
     `is_active`, `fault_at`, `raw_payload`)
  VALUES
    (NULLIF(TRIM(p_source_event_id), ''), NULLIF(TRIM(p_wcs_area_code), ''), NULLIF(TRIM(p_wcs_device_code), ''),
     NULLIF(TRIM(p_location), ''), NULLIF(TRIM(p_device_type), ''), NULLIF(TRIM(p_device_id), ''),
     NULLIF(TRIM(p_device_name), ''), NULLIF(TRIM(p_device_vendor), ''), TRIM(p_fault_code), NULLIF(TRIM(p_fault_value), ''),
     p_is_active, p_fault_at,
     CASE
       WHEN p_raw_payload IS NULL OR TRIM(p_raw_payload) = '' THEN NULL
       WHEN JSON_VALID(p_raw_payload) THEN p_raw_payload
       ELSE JSON_OBJECT('raw', p_raw_payload)
     END)
  ON DUPLICATE KEY UPDATE
    `id` = LAST_INSERT_ID(`id`);

  SET v_inbox_id = LAST_INSERT_ID();

  CALL `sp_process_wcs_alarm_event`(v_inbox_id);

  SELECT
    `id` AS `inbox_id`,
    `process_status`,
    `mapped_history_table`,
    `mapped_history_id`,
    `mapped_fault_type`,
    `error_message`
  FROM `wcs_alarm_event_inbox`
  WHERE `id` = v_inbox_id;
END;
