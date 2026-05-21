SET NAMES utf8mb4;

CALL `sp_add_column_if_missing`(
  'device_realtime_status',
  'fault_count',
  CONCAT('BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT ', QUOTE('故障发生累计次数'), ' AFTER `tag_value`')
);

DROP PROCEDURE IF EXISTS `sp_process_wcs_alarm_event`;

CREATE PROCEDURE `sp_process_wcs_alarm_event`(IN p_inbox_id BIGINT UNSIGNED)
proc: BEGIN
  DECLARE v_no_data BOOL DEFAULT FALSE;
  DECLARE v_error TEXT DEFAULT NULL;
  DECLARE v_source_event_id VARCHAR(128);
  DECLARE v_wcs_area_code VARCHAR(64);
  DECLARE v_wcs_device_code VARCHAR(100);
  DECLARE v_location VARCHAR(255);
  DECLARE v_device_type VARCHAR(100);
  DECLARE v_device_id VARCHAR(100);
  DECLARE v_device_name VARCHAR(255);
  DECLARE v_device_vendor VARCHAR(100);
  DECLARE v_fault_code VARCHAR(64);
  DECLARE v_fault_value VARCHAR(255);
  DECLARE v_is_active TINYINT(1);
  DECLARE v_fault_at DATETIME(3);
  DECLARE v_received_at DATETIME(3);
  DECLARE v_map_source_table VARCHAR(64);
  DECLARE v_map_location VARCHAR(255);
  DECLARE v_map_device_type VARCHAR(100);
  DECLARE v_map_device_id VARCHAR(100);
  DECLARE v_map_device_name VARCHAR(255);
  DECLARE v_map_device_vendor VARCHAR(100);
  DECLARE v_source_table VARCHAR(64);
  DECLARE v_fault_name VARCHAR(255);
  DECLARE v_fault_tag VARCHAR(255);
  DECLARE v_fault_type VARCHAR(100);
  DECLARE v_default_tag_value VARCHAR(64);
  DECLARE v_tag_value VARCHAR(255);
  DECLARE v_tag_state VARCHAR(100);
  DECLARE v_description TEXT;
  DECLARE v_remark TEXT;
  DECLARE v_node_id VARCHAR(255);
  DECLARE v_alias VARCHAR(255);
  DECLARE v_event_time DATETIME(3);
  DECLARE v_external_device_code VARCHAR(100);
  DECLARE v_history_id BIGINT DEFAULT NULL;

  DECLARE CONTINUE HANDLER FOR NOT FOUND SET v_no_data = TRUE;
  DECLARE EXIT HANDLER FOR SQLEXCEPTION
  BEGIN
    GET DIAGNOSTICS CONDITION 1 v_error = MESSAGE_TEXT;
    UPDATE `wcs_alarm_event_inbox`
    SET `process_status` = 'failed',
        `error_message` = v_error,
        `processed_at` = CURRENT_TIMESTAMP(3)
    WHERE `id` = p_inbox_id;
  END;

  UPDATE `wcs_alarm_event_inbox`
  SET `process_status` = 'processing',
      `process_attempts` = `process_attempts` + 1,
      `error_message` = NULL
  WHERE `id` = p_inbox_id
    AND `process_status` IN ('pending', 'failed');

  IF ROW_COUNT() = 0 THEN
    LEAVE proc;
  END IF;

  SET v_no_data = FALSE;
  SELECT
    `source_event_id`, `wcs_area_code`, `wcs_device_code`, `location`, `device_type`,
    `device_id`, `device_name`, `device_vendor`, `fault_code`, `fault_value`,
    `is_active`, `fault_at`, `received_at`
  INTO
    v_source_event_id, v_wcs_area_code, v_wcs_device_code, v_location, v_device_type,
    v_device_id, v_device_name, v_device_vendor, v_fault_code, v_fault_value,
    v_is_active, v_fault_at, v_received_at
  FROM `wcs_alarm_event_inbox`
  WHERE `id` = p_inbox_id;

  IF v_no_data THEN
    LEAVE proc;
  END IF;

  SET v_external_device_code = COALESCE(NULLIF(TRIM(v_wcs_device_code), ''), NULLIF(TRIM(v_device_id), ''));

  SET v_no_data = FALSE;
  SET v_map_source_table = NULL;
  SET v_map_location = NULL;
  SET v_map_device_type = NULL;
  SET v_map_device_id = NULL;
  SET v_map_device_name = NULL;
  SET v_map_device_vendor = NULL;

  SELECT
    `source_table`, `location`, `device_type`, `device_id`, `device_name`, `device_vendor`
  INTO
    v_map_source_table, v_map_location, v_map_device_type, v_map_device_id, v_map_device_name, v_map_device_vendor
  FROM `wcs_device_catalog`
  WHERE `enabled` = 1
    AND (`wcs_area_code` = COALESCE(NULLIF(TRIM(v_wcs_area_code), ''), '') OR `wcs_area_code` = '')
    AND `wcs_device_code` IN (
      COALESCE(NULLIF(TRIM(v_wcs_device_code), ''), '__none__'),
      COALESCE(NULLIF(TRIM(v_device_id), ''), '__none__')
    )
  ORDER BY
    CASE WHEN `wcs_area_code` = COALESCE(NULLIF(TRIM(v_wcs_area_code), ''), '') THEN 0 ELSE 1 END,
    `id`
  LIMIT 1;

  IF NOT v_no_data THEN
    SET v_source_table = v_map_source_table;
    SET v_location = COALESCE(NULLIF(TRIM(v_location), ''), v_map_location);
    SET v_device_type = COALESCE(NULLIF(TRIM(v_device_type), ''), v_map_device_type);
    SET v_device_id = COALESCE(NULLIF(TRIM(v_device_id), ''), v_map_device_id);
    SET v_device_name = COALESCE(NULLIF(TRIM(v_device_name), ''), v_map_device_name);
    SET v_device_vendor = COALESCE(NULLIF(TRIM(v_device_vendor), ''), v_map_device_vendor);
  END IF;

  IF v_source_table IS NULL THEN
    SET v_source_table = CASE
      WHEN UPPER(COALESCE(v_wcs_area_code, '')) = 'CPK' OR COALESCE(v_location, '') LIKE '%成品%' THEN 'cpk_alarm_log'
      WHEN UPPER(COALESCE(v_wcs_area_code, '')) = 'FLK' OR COALESCE(v_location, '') LIKE '%辅料%' THEN 'flk_alarm_log'
      WHEN UPPER(COALESCE(v_wcs_area_code, '')) = 'YLK' OR COALESCE(v_location, '') LIKE '%原料%' THEN 'ylk_alarm_log'
      ELSE NULL
    END;
  END IF;

  IF v_source_table NOT IN ('cpk_alarm_log', 'flk_alarm_log', 'ylk_alarm_log') THEN
    SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'WCS event cannot be mapped to a history table';
  END IF;

  SET v_no_data = FALSE;
  SET v_fault_name = NULL;
  SET v_fault_tag = NULL;
  SET v_fault_type = NULL;
  SET v_default_tag_value = '1';

  SELECT
    c.`fault_name`,
    NULLIF(TRIM(c.`fault_tag`), ''),
    ft.`fault_type_name`,
    c.`default_tag_value`
  INTO
    v_fault_name, v_fault_tag, v_fault_type, v_default_tag_value
  FROM `wcs_fault_code_catalog` c
  LEFT JOIN `fault_type_catalog` ft ON ft.`id` = c.`fault_type_id`
  WHERE c.`enabled` = 1
    AND c.`fault_code` = v_fault_code
    AND (c.`location` IS NULL OR c.`location` = '' OR c.`location` = v_location)
    AND (c.`device_type` IS NULL OR c.`device_type` = '' OR c.`device_type` = v_device_type)
    AND (c.`device_vendor` IS NULL OR c.`device_vendor` = '' OR c.`device_vendor` = v_device_vendor)
    AND (c.`device_id` IS NULL OR c.`device_id` = '' OR c.`device_id` = v_device_id)
  ORDER BY
    CASE WHEN c.`device_id` = v_device_id THEN 0 ELSE 1 END,
    CASE WHEN c.`device_vendor` = v_device_vendor THEN 0 ELSE 1 END,
    CASE WHEN c.`device_type` = v_device_type THEN 0 ELSE 1 END,
    CASE WHEN c.`location` = v_location THEN 0 ELSE 1 END,
    c.`id`
  LIMIT 1;

  IF v_no_data THEN
    SET v_fault_name = CONCAT('未维护WCS故障代码：', v_fault_code);
    SET v_fault_tag = CONCAT('WCS_', v_fault_code);
    SET v_fault_type = NULL;
    SET v_default_tag_value = '1';
  END IF;

  SET v_event_time = COALESCE(v_fault_at, v_received_at, CURRENT_TIMESTAMP(3));

  IF v_is_active IS NOT NULL THEN
    SET v_tag_value = CASE WHEN v_is_active = 0 THEN '0' ELSE '1' END;
  ELSEIF v_fault_value IS NULL OR TRIM(v_fault_value) = '' THEN
    SET v_tag_value = COALESCE(NULLIF(TRIM(v_default_tag_value), ''), '1');
  ELSE
    SET v_tag_value = CASE
      WHEN LOWER(TRIM(v_fault_value)) IN ('0', 'false', 'off', 'normal', 'ok', '正常', '无') THEN '0'
      ELSE '1'
    END;
  END IF;

  SET v_tag_state = COALESCE(NULLIF(TRIM(v_fault_value), ''), v_fault_code);
  SET v_node_id = CONCAT('wcs://', v_source_table, '/', COALESCE(NULLIF(TRIM(v_device_id), ''), COALESCE(v_external_device_code, 'unknown')), '/', v_fault_code);
  SET v_alias = CONCAT('wcs.', COALESCE(NULLIF(TRIM(v_location), ''), 'unknown'), '.', COALESCE(NULLIF(TRIM(v_device_id), ''), COALESCE(v_external_device_code, 'unknown')), '.', v_fault_code);
  SET v_description = v_fault_name;
  SET v_remark = CONCAT('WCS直写；故障代码=', v_fault_code, '；原始值=', COALESCE(v_fault_value, ''), '；外部设备=', COALESCE(v_external_device_code, ''));

  SET @p_source_system = 'wcs';
  SET @p_source_event_id = v_source_event_id;
  SET @p_location = v_location;
  SET @p_device_type = v_device_type;
  SET @p_device_id = v_device_id;
  SET @p_external_device_code = v_external_device_code;
  SET @p_node_id = v_node_id;
  SET @p_alias = v_alias;
  SET @p_tag = COALESCE(v_fault_tag, CONCAT('WCS_', v_fault_code));
  SET @p_fault_type = v_fault_type;
  SET @p_fault_code = v_fault_code;
  SET @p_fault_name = v_fault_name;
  SET @p_tag_state = v_tag_state;
  SET @p_tag_value = v_tag_value;
  SET @p_description = v_description;
  SET @p_remark = v_remark;
  SET @p_create_at = v_event_time;
  SET @p_update_at = v_event_time;

  SET @sql = CONCAT(
    'INSERT INTO `', v_source_table, '` ',
    '(`source_system`, `source_event_id`, `location`, `device`, `device_id`, `external_device_code`, ',
    '`node_id`, `alias`, `tag`, `fault_type`, `fault_code`, `fault_name`, `tag_state`, `tag_value`, ',
    '`description`, `remark`, `create_at`, `update_at`) ',
    'VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)'
  );
  PREPARE stmt FROM @sql;
  EXECUTE stmt USING
    @p_source_system, @p_source_event_id, @p_location, @p_device_type, @p_device_id, @p_external_device_code,
    @p_node_id, @p_alias, @p_tag, @p_fault_type, @p_fault_code, @p_fault_name, @p_tag_state, @p_tag_value,
    @p_description, @p_remark, @p_create_at, @p_update_at;
  DEALLOCATE PREPARE stmt;

  SET v_history_id = LAST_INSERT_ID();

  INSERT INTO `device_realtime_status`
    (`source_system`, `source_event_id`, `source_table`, `location`, `device_type`, `device_id`, `external_device_code`,
     `device_name`, `node_id`, `alias`, `tag`, `fault_type`, `fault_code`, `fault_name`, `tag_state`, `tag_value`, `fault_count`,
     `description`, `status_description`, `last_fault_at`)
  VALUES
    ('wcs', v_source_event_id, v_source_table, v_location, v_device_type, v_device_id, v_external_device_code,
     v_device_name, v_node_id, v_alias, COALESCE(v_fault_tag, CONCAT('WCS_', v_fault_code)), v_fault_type,
     v_fault_code, v_fault_name, v_tag_state, v_tag_value, CASE WHEN COALESCE(NULLIF(TRIM(v_tag_value), ''), '0') <> '0' THEN 1 ELSE 0 END, v_description, v_remark, v_event_time)
  ON DUPLICATE KEY UPDATE
    `source_system` = VALUES(`source_system`),
    `source_event_id` = VALUES(`source_event_id`),
    `location` = VALUES(`location`),
    `device_type` = VALUES(`device_type`),
    `device_id` = VALUES(`device_id`),
    `external_device_code` = VALUES(`external_device_code`),
    `device_name` = VALUES(`device_name`),
    `alias` = VALUES(`alias`),
    `tag` = VALUES(`tag`),
    `fault_type` = VALUES(`fault_type`),
    `fault_code` = VALUES(`fault_code`),
    `fault_name` = VALUES(`fault_name`),
    `tag_state` = VALUES(`tag_state`),
    `fault_count` = IF(COALESCE(NULLIF(TRIM(`tag_value`), ''), '0') = '0' AND COALESCE(NULLIF(TRIM(VALUES(`tag_value`)), ''), '0') <> '0', `fault_count` + 1, `fault_count`),
    `tag_value` = VALUES(`tag_value`),
    `description` = VALUES(`description`),
    `status_description` = VALUES(`status_description`),
    `last_fault_at` = VALUES(`last_fault_at`),
    `updated_at` = CURRENT_TIMESTAMP(3);

  UPDATE `wcs_alarm_event_inbox`
  SET `process_status` = 'processed',
      `mapped_history_table` = v_source_table,
      `mapped_history_id` = v_history_id,
      `mapped_fault_type` = v_fault_type,
      `processed_at` = CURRENT_TIMESTAMP(3),
      `error_message` = CASE
        WHEN v_fault_name LIKE '未维护WCS故障代码：%' THEN v_fault_name
        ELSE NULL
      END
  WHERE `id` = p_inbox_id;
END;
