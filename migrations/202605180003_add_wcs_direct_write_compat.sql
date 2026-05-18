SET NAMES utf8mb4;

DROP PROCEDURE IF EXISTS `sp_add_column_if_missing`;

CREATE PROCEDURE `sp_add_column_if_missing`(
  IN p_table VARCHAR(80),
  IN p_column VARCHAR(64),
  IN p_definition TEXT
)
BEGIN
  DECLARE v_exists INT DEFAULT 0;

  SELECT COUNT(*)
    INTO v_exists
  FROM INFORMATION_SCHEMA.COLUMNS
  WHERE TABLE_SCHEMA = DATABASE()
    AND TABLE_NAME = p_table
    AND COLUMN_NAME = p_column;

  IF v_exists = 0 THEN
    SET @sql = CONCAT('ALTER TABLE `', p_table, '` ADD COLUMN `', p_column, '` ', p_definition);
    PREPARE stmt FROM @sql;
    EXECUTE stmt;
    DEALLOCATE PREPARE stmt;
  END IF;
END;

DROP PROCEDURE IF EXISTS `sp_add_index_if_missing`;

CREATE PROCEDURE `sp_add_index_if_missing`(
  IN p_table VARCHAR(80),
  IN p_index VARCHAR(64),
  IN p_columns TEXT
)
BEGIN
  DECLARE v_exists INT DEFAULT 0;

  SELECT COUNT(*)
    INTO v_exists
  FROM INFORMATION_SCHEMA.STATISTICS
  WHERE TABLE_SCHEMA = DATABASE()
    AND TABLE_NAME = p_table
    AND INDEX_NAME = p_index;

  IF v_exists = 0 THEN
    SET @sql = CONCAT('CREATE INDEX `', p_index, '` ON `', p_table, '` ', p_columns);
    PREPARE stmt FROM @sql;
    EXECUTE stmt;
    DEALLOCATE PREPARE stmt;
  END IF;
END;

DROP PROCEDURE IF EXISTS `sp_ensure_alarm_history_wcs_columns`;

CREATE PROCEDURE `sp_ensure_alarm_history_wcs_columns`(IN p_table VARCHAR(80))
BEGIN
  CALL `sp_add_column_if_missing`(
    p_table,
    'source_system',
    CONCAT('VARCHAR(32) NOT NULL DEFAULT ', QUOTE('opcua'), ' COMMENT ', QUOTE('数据来源系统：opcua、wcs等'), ' AFTER `id`')
  );
  CALL `sp_add_column_if_missing`(
    p_table,
    'source_event_id',
    CONCAT('VARCHAR(128) DEFAULT NULL COMMENT ', QUOTE('外部系统事件ID，WCS直写时用于幂等追踪'), ' AFTER `source_system`')
  );
  CALL `sp_add_column_if_missing`(
    p_table,
    'external_device_code',
    CONCAT('VARCHAR(100) DEFAULT NULL COMMENT ', QUOTE('外部系统设备编码，例如WCS设备编码'), ' AFTER `device_id`')
  );
  CALL `sp_add_column_if_missing`(
    p_table,
    'fault_code',
    CONCAT('VARCHAR(64) DEFAULT NULL COMMENT ', QUOTE('外部故障代码，WCS直写时保存原始故障码'), ' AFTER `fault_type`')
  );
  CALL `sp_add_column_if_missing`(
    p_table,
    'fault_name',
    CONCAT('VARCHAR(255) DEFAULT NULL COMMENT ', QUOTE('外部故障名称，通常来自WCS故障代码基础表'), ' AFTER `fault_code`')
  );

  CALL `sp_add_index_if_missing`(p_table, 'idx_alarm_source_system_create', '(`source_system`, `create_at`)');
  CALL `sp_add_index_if_missing`(p_table, 'idx_alarm_fault_code_create', '(`fault_code`, `create_at`)');
  CALL `sp_add_index_if_missing`(p_table, 'idx_alarm_source_event', '(`source_system`, `source_event_id`)');
END;

CALL `sp_ensure_alarm_history_wcs_columns`('cpk_alarm_log');
CALL `sp_ensure_alarm_history_wcs_columns`('flk_alarm_log');
CALL `sp_ensure_alarm_history_wcs_columns`('ylk_alarm_log');

CALL `sp_add_column_if_missing`(
  'device_realtime_status',
  'source_system',
  CONCAT('VARCHAR(32) NOT NULL DEFAULT ', QUOTE('opcua'), ' COMMENT ', QUOTE('数据来源系统：opcua、wcs等'), ' AFTER `id`')
);
CALL `sp_add_column_if_missing`(
  'device_realtime_status',
  'source_event_id',
  CONCAT('VARCHAR(128) DEFAULT NULL COMMENT ', QUOTE('外部系统事件ID'), ' AFTER `source_system`')
);
CALL `sp_add_column_if_missing`(
  'device_realtime_status',
  'external_device_code',
  CONCAT('VARCHAR(100) DEFAULT NULL COMMENT ', QUOTE('外部系统设备编码'), ' AFTER `device_id`')
);
CALL `sp_add_column_if_missing`(
  'device_realtime_status',
  'fault_code',
  CONCAT('VARCHAR(64) DEFAULT NULL COMMENT ', QUOTE('外部故障代码'), ' AFTER `fault_type`')
);
CALL `sp_add_column_if_missing`(
  'device_realtime_status',
  'fault_name',
  CONCAT('VARCHAR(255) DEFAULT NULL COMMENT ', QUOTE('外部故障名称'), ' AFTER `fault_code`')
);

CALL `sp_add_index_if_missing`('device_realtime_status', 'idx_device_realtime_source_system', '(`source_system`, `updated_at`)');
CALL `sp_add_index_if_missing`('device_realtime_status', 'idx_device_realtime_fault_code', '(`fault_code`, `updated_at`)');

CREATE TABLE IF NOT EXISTS `wcs_device_catalog` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键ID',
  `wcs_area_code` VARCHAR(64) NOT NULL DEFAULT '' COMMENT 'WCS库区编码，例如CPK、FLK、YLK；为空表示通用',
  `wcs_device_code` VARCHAR(100) NOT NULL COMMENT 'WCS设备编码，WCS写入收件箱时优先使用此字段匹配设备',
  `source_table` VARCHAR(64) NOT NULL COMMENT '映射后的历史热表，仅允许cpk_alarm_log、flk_alarm_log、ylk_alarm_log',
  `location` VARCHAR(255) NOT NULL COMMENT '标准库区',
  `device_type` VARCHAR(100) NOT NULL COMMENT '标准设备类型',
  `device_id` VARCHAR(100) NOT NULL COMMENT '标准设备编号',
  `device_name` VARCHAR(255) DEFAULT NULL COMMENT '设备名称',
  `device_vendor` VARCHAR(100) DEFAULT NULL COMMENT '设备厂家或系统来源',
  `enabled` TINYINT(1) NOT NULL DEFAULT 1 COMMENT '启用状态，1启用，0停用',
  `remark` TEXT DEFAULT NULL COMMENT '备注，记录WCS设备编码来源或特殊匹配规则',
  `created_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) COMMENT '创建时间',
  `updated_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3) COMMENT '更新时间',
  PRIMARY KEY (`id`),
  UNIQUE KEY `uq_wcs_device_catalog_code` (`wcs_area_code`, `wcs_device_code`),
  KEY `idx_wcs_device_catalog_standard` (`location`, `device_type`, `device_id`),
  KEY `idx_wcs_device_catalog_enabled` (`enabled`),
  CONSTRAINT `chk_wcs_device_catalog_source_table`
    CHECK (`source_table` IN ('cpk_alarm_log', 'flk_alarm_log', 'ylk_alarm_log'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='WCS设备映射表：把WCS设备编码转换为标准库区、设备类型、设备编号和历史表';

CREATE TABLE IF NOT EXISTS `wcs_fault_code_catalog` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键ID',
  `source_sheet` VARCHAR(255) NOT NULL COMMENT '来源故障代码表名称，便于追溯Excel原始表',
  `location` VARCHAR(255) DEFAULT NULL COMMENT '适用库区；为空表示不限制库区',
  `device_type` VARCHAR(100) DEFAULT NULL COMMENT '适用设备类型；为空表示不限制设备类型',
  `device_vendor` VARCHAR(100) DEFAULT NULL COMMENT '适用厂家；为空表示不限制厂家',
  `device_id` VARCHAR(100) DEFAULT NULL COMMENT '适用设备编号；为空表示该库区/设备类型通用',
  `device_id_scope` VARCHAR(100) DEFAULT NULL COMMENT '来源表里的设备范围说明，例如SC01-SC06',
  `fault_code` VARCHAR(64) NOT NULL COMMENT 'WCS或设备侧故障代码',
  `fault_name` VARCHAR(255) NOT NULL COMMENT '故障内容或错误信息',
  `fault_tag` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '英文Tag或人工维护的标准故障标签；没有时可为空',
  `fault_type_id` BIGINT UNSIGNED DEFAULT NULL COMMENT '故障类型ID，关联fault_type_catalog',
  `default_tag_value` VARCHAR(64) NOT NULL DEFAULT '1' COMMENT 'WCS只传故障代码时默认转换的故障值，1故障，0正常/无故障',
  `enabled` TINYINT(1) NOT NULL DEFAULT 1 COMMENT '启用状态，1启用，0停用',
  `remark` TEXT DEFAULT NULL COMMENT '备注，记录分类依据或现场维护说明',
  `created_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) COMMENT '创建时间',
  `updated_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3) COMMENT '更新时间',
  PRIMARY KEY (`id`),
  UNIQUE KEY `uq_wcs_fault_code_catalog_code` (`source_sheet`, `fault_code`, `fault_tag`),
  KEY `idx_wcs_fault_code_lookup` (`location`, `device_type`, `device_vendor`, `device_id`, `fault_code`),
  KEY `idx_wcs_fault_code_type` (`fault_type_id`),
  KEY `idx_wcs_fault_code_enabled` (`enabled`),
  CONSTRAINT `fk_wcs_fault_code_fault_type`
    FOREIGN KEY (`fault_type_id`) REFERENCES `fault_type_catalog` (`id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='WCS故障代码基础表：保存WCS或设备侧故障码、故障内容、故障类型和默认故障值';

CREATE TABLE IF NOT EXISTS `wcs_alarm_event_inbox` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键ID',
  `source_event_id` VARCHAR(128) DEFAULT NULL COMMENT 'WCS事件ID；有值时用于防重',
  `wcs_area_code` VARCHAR(64) DEFAULT NULL COMMENT 'WCS库区编码，例如CPK、FLK、YLK',
  `wcs_device_code` VARCHAR(100) DEFAULT NULL COMMENT 'WCS设备编码，优先通过wcs_device_catalog匹配',
  `location` VARCHAR(255) DEFAULT NULL COMMENT '库区；WCS无法提供设备映射时可直接写标准库区',
  `device_type` VARCHAR(100) DEFAULT NULL COMMENT '设备类型',
  `device_id` VARCHAR(100) DEFAULT NULL COMMENT '设备编号',
  `device_name` VARCHAR(255) DEFAULT NULL COMMENT '设备名称',
  `device_vendor` VARCHAR(100) DEFAULT NULL COMMENT '设备厂家',
  `fault_code` VARCHAR(64) NOT NULL COMMENT 'WCS或设备侧故障代码',
  `fault_value` VARCHAR(255) DEFAULT NULL COMMENT 'WCS原始故障值；为空时按故障代码基础表的默认故障值处理',
  `is_active` TINYINT(1) DEFAULT NULL COMMENT '是否故障中：1故障，0恢复；为空时由fault_value或默认故障值推断',
  `fault_at` DATETIME(3) DEFAULT NULL COMMENT 'WCS侧故障时间；为空时使用收到时间',
  `received_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) COMMENT '数据库收到时间',
  `raw_payload` JSON DEFAULT NULL COMMENT 'WCS原始报文，仅用于审计，不作为核心查询字段',
  `process_status` ENUM('pending', 'processing', 'processed', 'failed') NOT NULL DEFAULT 'pending' COMMENT '处理状态',
  `process_attempts` INT UNSIGNED NOT NULL DEFAULT 0 COMMENT '处理尝试次数',
  `error_message` TEXT DEFAULT NULL COMMENT '处理失败或未完全匹配时的说明',
  `mapped_history_table` VARCHAR(64) DEFAULT NULL COMMENT '已写入的历史热表',
  `mapped_history_id` BIGINT DEFAULT NULL COMMENT '写入历史热表后的主键ID',
  `mapped_fault_type` VARCHAR(100) DEFAULT NULL COMMENT '映射后的故障类型',
  `processed_at` DATETIME(3) DEFAULT NULL COMMENT '处理完成时间',
  `created_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) COMMENT '创建时间',
  PRIMARY KEY (`id`),
  UNIQUE KEY `uq_wcs_alarm_event_id` (`source_event_id`),
  KEY `idx_wcs_alarm_inbox_pending` (`process_status`, `id`),
  KEY `idx_wcs_alarm_inbox_device` (`wcs_area_code`, `wcs_device_code`, `fault_code`),
  KEY `idx_wcs_alarm_inbox_fault_at` (`fault_at`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='WCS报警事件收件箱：WCS直写本表，数据库再统一映射到历史热表和实时状态表';

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
     `device_name`, `node_id`, `alias`, `tag`, `fault_type`, `fault_code`, `fault_name`, `tag_state`, `tag_value`,
     `description`, `status_description`, `last_fault_at`)
  VALUES
    ('wcs', v_source_event_id, v_source_table, v_location, v_device_type, v_device_id, v_external_device_code,
     v_device_name, v_node_id, v_alias, COALESCE(v_fault_tag, CONCAT('WCS_', v_fault_code)), v_fault_type,
     v_fault_code, v_fault_name, v_tag_state, v_tag_value, v_description, v_remark, v_event_time)
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

DROP PROCEDURE IF EXISTS `sp_process_wcs_alarm_inbox`;

CREATE PROCEDURE `sp_process_wcs_alarm_inbox`(IN p_batch_size INT)
BEGIN
  DECLARE done BOOL DEFAULT FALSE;
  DECLARE v_id BIGINT UNSIGNED;
  DECLARE cur CURSOR FOR
    SELECT `id` FROM `tmp_wcs_alarm_event_ids` ORDER BY `id`;
  DECLARE CONTINUE HANDLER FOR NOT FOUND SET done = TRUE;

  IF p_batch_size IS NULL OR p_batch_size < 1 THEN
    SET p_batch_size = 500;
  END IF;

  DROP TEMPORARY TABLE IF EXISTS `tmp_wcs_alarm_event_ids`;
  CREATE TEMPORARY TABLE `tmp_wcs_alarm_event_ids` (
    `id` BIGINT UNSIGNED NOT NULL PRIMARY KEY
  ) ENGINE=MEMORY;

  INSERT INTO `tmp_wcs_alarm_event_ids` (`id`)
  SELECT `id`
  FROM `wcs_alarm_event_inbox`
  WHERE `process_status` IN ('pending', 'failed')
    AND `process_attempts` < 5
  ORDER BY `id`
  LIMIT p_batch_size;

  OPEN cur;

  read_loop: LOOP
    FETCH cur INTO v_id;
    IF done THEN
      LEAVE read_loop;
    END IF;

    CALL `sp_process_wcs_alarm_event`(v_id);
  END LOOP;

  CLOSE cur;
  DROP TEMPORARY TABLE IF EXISTS `tmp_wcs_alarm_event_ids`;
END;

DROP PROCEDURE IF EXISTS `sp_archive_alarm_month`;

CREATE PROCEDURE `sp_archive_alarm_month`(IN p_month CHAR(6))
BEGIN
  DECLARE done BOOL DEFAULT FALSE;
  DECLARE v_source_table VARCHAR(64);
  DECLARE v_target_table VARCHAR(80);
  DECLARE v_run_id BIGINT UNSIGNED DEFAULT NULL;
  DECLARE v_start DATETIME(3);
  DECLARE v_end DATETIME(3);
  DECLARE v_rows BIGINT DEFAULT 0;
  DECLARE v_copied BIGINT DEFAULT 0;
  DECLARE v_deleted BIGINT DEFAULT 0;
  DECLARE v_error TEXT DEFAULT NULL;
  DECLARE cur CURSOR FOR
    SELECT source_table
    FROM alarm_archive_tables
    WHERE enabled = 1
    ORDER BY source_table;
  DECLARE CONTINUE HANDLER FOR NOT FOUND SET done = TRUE;
  DECLARE EXIT HANDLER FOR SQLEXCEPTION
  BEGIN
    GET DIAGNOSTICS CONDITION 1 v_error = MESSAGE_TEXT;
    IF v_run_id IS NOT NULL THEN
      UPDATE `alarm_archive_runs`
      SET `status` = 'failed',
          `finished_at` = CURRENT_TIMESTAMP(3),
          `copied_rows` = v_copied,
          `deleted_rows` = v_deleted,
          `error_message` = v_error
      WHERE `id` = v_run_id;
    END IF;
    RESIGNAL;
  END;

  IF p_month IS NULL OR p_month NOT REGEXP '^[0-9]{6}$' THEN
    SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'p_month must use YYYYMM format';
  END IF;

  SET v_start = STR_TO_DATE(CONCAT(p_month, '01'), '%Y%m%d');
  SET v_end = DATE_ADD(v_start, INTERVAL 1 MONTH);

  IF v_start >= DATE_FORMAT(CURRENT_DATE(), '%Y-%m-01') THEN
    SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'current or future month must not be archived';
  END IF;

  SET @archive_start = v_start;
  SET @archive_end = v_end;

  OPEN cur;

  archive_loop: LOOP
    FETCH cur INTO v_source_table;
    IF done THEN
      LEAVE archive_loop;
    END IF;

    SET v_target_table = CONCAT(v_source_table, '_', p_month);
    SET v_run_id = NULL;
    SET v_copied = 0;
    SET v_deleted = 0;

    INSERT INTO `alarm_archive_runs`
      (`archive_month`, `source_table`, `target_table`, `range_start`, `range_end`, `status`)
    VALUES
      (p_month, v_source_table, v_target_table, v_start, v_end, 'running');
    SET v_run_id = LAST_INSERT_ID();

    SET @sql = CONCAT('CREATE TABLE IF NOT EXISTS `', v_target_table, '` LIKE `', v_source_table, '`');
    PREPARE stmt FROM @sql;
    EXECUTE stmt;
    DEALLOCATE PREPARE stmt;

    CALL `sp_ensure_alarm_history_wcs_columns`(v_target_table);

    SET @sql = CONCAT(
      'INSERT INTO `', v_target_table, '` ',
      '(`id`, `source_system`, `source_event_id`, `location`, `device`, `device_id`, `external_device_code`, ',
      '`node_id`, `alias`, `tag`, `fault_type`, `fault_code`, `fault_name`, `tag_state`, `tag_value`, `description`, `remark`, `create_at`, `update_at`) ',
      'SELECT s.`id`, s.`source_system`, s.`source_event_id`, s.`location`, s.`device`, s.`device_id`, s.`external_device_code`, ',
      's.`node_id`, s.`alias`, s.`tag`, s.`fault_type`, s.`fault_code`, s.`fault_name`, s.`tag_state`, s.`tag_value`, s.`description`, s.`remark`, s.`create_at`, s.`update_at` ',
      'FROM `', v_source_table, '` s ',
      'WHERE s.`create_at` >= ? AND s.`create_at` < ? ',
      'AND NOT EXISTS (SELECT 1 FROM `', v_target_table, '` t WHERE t.`id` = s.`id`)'
    );
    PREPARE stmt FROM @sql;
    EXECUTE stmt USING @archive_start, @archive_end;
    SET v_copied = ROW_COUNT();
    DEALLOCATE PREPARE stmt;

    SET @sql = CONCAT('SELECT COUNT(*) INTO @archive_source_count FROM `', v_source_table, '` WHERE `create_at` >= ? AND `create_at` < ?');
    PREPARE stmt FROM @sql;
    EXECUTE stmt USING @archive_start, @archive_end;
    DEALLOCATE PREPARE stmt;

    SET @sql = CONCAT('SELECT COUNT(*) INTO @archive_target_count FROM `', v_target_table, '` WHERE `create_at` >= ? AND `create_at` < ?');
    PREPARE stmt FROM @sql;
    EXECUTE stmt USING @archive_start, @archive_end;
    DEALLOCATE PREPARE stmt;

    IF @archive_target_count < @archive_source_count THEN
      SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'archive target row count is lower than source row count';
    END IF;

    delete_loop: LOOP
      SET @sql = CONCAT(
        'DELETE FROM `', v_source_table, '` ',
        'WHERE `create_at` >= ? AND `create_at` < ? ',
        'ORDER BY `id` LIMIT 5000'
      );
      PREPARE stmt FROM @sql;
      EXECUTE stmt USING @archive_start, @archive_end;
      SET v_rows = ROW_COUNT();
      DEALLOCATE PREPARE stmt;

      SET v_deleted = v_deleted + v_rows;
      IF v_rows = 0 THEN
        LEAVE delete_loop;
      END IF;
    END LOOP;

    UPDATE `alarm_archive_runs`
    SET `status` = 'completed',
        `finished_at` = CURRENT_TIMESTAMP(3),
        `copied_rows` = v_copied,
        `deleted_rows` = v_deleted,
        `error_message` = NULL
    WHERE `id` = v_run_id;
  END LOOP;

  CLOSE cur;
END;

CREATE OR REPLACE VIEW `wcs_fault_code_design_view` AS
SELECT
  c.`source_sheet` AS `来源表`,
  c.`location` AS `库区`,
  c.`device_type` AS `设备类型`,
  c.`device_vendor` AS `设备厂家`,
  c.`device_id_scope` AS `设备范围`,
  c.`device_id` AS `设备编号`,
  c.`fault_code` AS `故障代码`,
  c.`fault_name` AS `故障内容`,
  c.`fault_tag` AS `英文Tag`,
  ft.`fault_type_name` AS `故障类型`,
  c.`default_tag_value` AS `默认故障值`,
  c.`enabled` AS `启用状态`,
  c.`remark` AS `备注`
FROM `wcs_fault_code_catalog` c
LEFT JOIN `fault_type_catalog` ft ON ft.`id` = c.`fault_type_id`;

DROP EVENT IF EXISTS `ev_process_wcs_alarm_inbox`;

CREATE EVENT `ev_process_wcs_alarm_inbox`
ON SCHEDULE EVERY 1 MINUTE
STARTS (CURRENT_TIMESTAMP + INTERVAL 1 MINUTE)
DO
  CALL `sp_process_wcs_alarm_inbox`(500);
