CREATE TABLE IF NOT EXISTS `device_catalog` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `location` VARCHAR(255) NOT NULL COMMENT '库区',
  `device_name` VARCHAR(255) NOT NULL COMMENT '设备名称',
  `device_type` VARCHAR(100) NOT NULL COMMENT '设备类型',
  `device_id` VARCHAR(100) NOT NULL COMMENT '设备编号',
  `device_code` VARCHAR(100) DEFAULT NULL COMMENT '设备编码',
  `model_spec` VARCHAR(255) DEFAULT NULL COMMENT '型号规格',
  `commissioned_at` DATE DEFAULT NULL COMMENT '投产时间',
  `install_location` VARCHAR(255) DEFAULT NULL COMMENT '投产区域或位置',
  `remark` TEXT DEFAULT NULL COMMENT '备注',
  `enabled` TINYINT(1) NOT NULL DEFAULT 1,
  `created_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
  `updated_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3),
  PRIMARY KEY (`id`),
  UNIQUE KEY `uq_device_catalog_code` (`device_code`),
  KEY `idx_device_catalog_lookup` (`location`, `device_type`, `device_id`),
  KEY `idx_device_catalog_enabled` (`enabled`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='设备档案基表';

CREATE TABLE IF NOT EXISTS `fault_type_catalog` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `fault_type_code` VARCHAR(64) NOT NULL COMMENT '故障类型编码',
  `fault_type_name` VARCHAR(100) NOT NULL COMMENT '故障类型名称',
  `enabled` TINYINT(1) NOT NULL DEFAULT 1,
  `created_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
  `updated_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3),
  PRIMARY KEY (`id`),
  UNIQUE KEY `uq_fault_type_code` (`fault_type_code`),
  UNIQUE KEY `uq_fault_type_name` (`fault_type_name`),
  KEY `idx_fault_type_enabled` (`enabled`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='故障类型基础表';

INSERT IGNORE INTO `fault_type_catalog` (`fault_type_code`, `fault_type_name`) VALUES
  ('fork', '货叉'),
  ('photoelectric', '光电'),
  ('safety_device', '安全装置'),
  ('breaker', '断路器'),
  ('oversize_offset', '超边超偏'),
  ('drive_fault', '驱动故障'),
  ('ppi_comm', '通讯故障PPI'),
  ('upper_signal', '通讯/上位信号'),
  ('drive_positioning', '驱动/定位控制');

CREATE TABLE IF NOT EXISTS `tag_catalog` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `location` VARCHAR(255) DEFAULT NULL COMMENT '库区',
  `device_type` VARCHAR(100) DEFAULT NULL COMMENT '设备类型',
  `device_id` VARCHAR(100) DEFAULT NULL COMMENT '设备编号',
  `node_id` VARCHAR(255) DEFAULT NULL COMMENT 'OPC UA NodeId',
  `alias` VARCHAR(255) DEFAULT NULL COMMENT '采集别名',
  `tag` VARCHAR(255) NOT NULL COMMENT '故障标签',
  `fault_type_id` BIGINT UNSIGNED DEFAULT NULL COMMENT '故障类型ID',
  `enabled` TINYINT(1) NOT NULL DEFAULT 1,
  `remark` TEXT DEFAULT NULL COMMENT '备注',
  `created_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
  `updated_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3),
  PRIMARY KEY (`id`),
  UNIQUE KEY `uq_tag_catalog_node_id` (`node_id`),
  KEY `idx_tag_catalog_alias` (`alias`),
  KEY `idx_tag_catalog_tag` (`tag`),
  KEY `idx_tag_catalog_device` (`location`, `device_type`, `device_id`),
  KEY `idx_tag_catalog_fault_type` (`fault_type_id`),
  KEY `idx_tag_catalog_enabled` (`enabled`),
  CONSTRAINT `fk_tag_catalog_fault_type`
    FOREIGN KEY (`fault_type_id`) REFERENCES `fault_type_catalog` (`id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='标签基础表';

CREATE TABLE IF NOT EXISTS `alarm_archive_tables` (
  `source_table` VARCHAR(64) NOT NULL,
  `enabled` TINYINT(1) NOT NULL DEFAULT 1,
  `created_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
  PRIMARY KEY (`source_table`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='故障历史归档源表';

INSERT IGNORE INTO `alarm_archive_tables` (`source_table`) VALUES
  ('cpk_alarm_log'),
  ('flk_alarm_log'),
  ('ylk_alarm_log');

CREATE TABLE IF NOT EXISTS `alarm_archive_runs` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `archive_month` CHAR(6) NOT NULL COMMENT '归档月份YYYYMM',
  `source_table` VARCHAR(64) NOT NULL COMMENT '热表',
  `target_table` VARCHAR(80) NOT NULL COMMENT '月度结存表',
  `range_start` DATETIME(3) NOT NULL,
  `range_end` DATETIME(3) NOT NULL,
  `started_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
  `finished_at` DATETIME(3) DEFAULT NULL,
  `copied_rows` BIGINT UNSIGNED NOT NULL DEFAULT 0,
  `deleted_rows` BIGINT UNSIGNED NOT NULL DEFAULT 0,
  `status` ENUM('running', 'completed', 'failed') NOT NULL DEFAULT 'running',
  `error_message` TEXT DEFAULT NULL,
  PRIMARY KEY (`id`),
  KEY `idx_alarm_archive_runs_month` (`archive_month`),
  KEY `idx_alarm_archive_runs_table` (`source_table`, `archive_month`),
  KEY `idx_alarm_archive_runs_status` (`status`, `started_at`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='故障历史月度归档记录';

CREATE TABLE IF NOT EXISTS `daily_area_fault_stats` (
  `stat_date` DATE NOT NULL,
  `location` VARCHAR(255) NOT NULL COMMENT '库区',
  `device_total` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '设备总数',
  `fault_count` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '故障次数',
  `updated_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3),
  PRIMARY KEY (`stat_date`, `location`),
  KEY `idx_daily_area_fault_count` (`stat_date`, `fault_count`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='区域故障统计表';

CREATE TABLE IF NOT EXISTS `daily_device_type_fault_stats` (
  `stat_date` DATE NOT NULL,
  `device_type` VARCHAR(100) NOT NULL COMMENT '设备类型',
  `device_total` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '设备总数',
  `fault_count` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '故障次数',
  `updated_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3),
  PRIMARY KEY (`stat_date`, `device_type`),
  KEY `idx_daily_device_type_fault_count` (`stat_date`, `fault_count`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='设备类型统计表';

CREATE TABLE IF NOT EXISTS `daily_fault_type_stats` (
  `stat_date` DATE NOT NULL,
  `fault_type` VARCHAR(100) NOT NULL COMMENT '故障类型',
  `device_count` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '涉及设备数',
  `fault_count` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '故障次数',
  `updated_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3),
  PRIMARY KEY (`stat_date`, `fault_type`),
  KEY `idx_daily_fault_type_count` (`stat_date`, `fault_count`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='故障类型统计表';

DROP PROCEDURE IF EXISTS `sp_add_alarm_history_column`;

CREATE PROCEDURE `sp_add_alarm_history_column`(
  IN p_table VARCHAR(64),
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

CALL `sp_add_alarm_history_column`('cpk_alarm_log', 'node_id', 'VARCHAR(255) DEFAULT NULL COMMENT ''OPC UA NodeId'' AFTER `device_id`');
CALL `sp_add_alarm_history_column`('cpk_alarm_log', 'alias', 'VARCHAR(255) DEFAULT NULL COMMENT ''采集别名'' AFTER `node_id`');
CALL `sp_add_alarm_history_column`('cpk_alarm_log', 'fault_type', 'VARCHAR(100) DEFAULT NULL COMMENT ''故障类型'' AFTER `tag`');
CALL `sp_add_alarm_history_column`('flk_alarm_log', 'node_id', 'VARCHAR(255) DEFAULT NULL COMMENT ''OPC UA NodeId'' AFTER `device_id`');
CALL `sp_add_alarm_history_column`('flk_alarm_log', 'alias', 'VARCHAR(255) DEFAULT NULL COMMENT ''采集别名'' AFTER `node_id`');
CALL `sp_add_alarm_history_column`('flk_alarm_log', 'fault_type', 'VARCHAR(100) DEFAULT NULL COMMENT ''故障类型'' AFTER `tag`');
CALL `sp_add_alarm_history_column`('ylk_alarm_log', 'node_id', 'VARCHAR(255) DEFAULT NULL COMMENT ''OPC UA NodeId'' AFTER `device_id`');
CALL `sp_add_alarm_history_column`('ylk_alarm_log', 'alias', 'VARCHAR(255) DEFAULT NULL COMMENT ''采集别名'' AFTER `node_id`');
CALL `sp_add_alarm_history_column`('ylk_alarm_log', 'fault_type', 'VARCHAR(100) DEFAULT NULL COMMENT ''故障类型'' AFTER `tag`');

DROP PROCEDURE IF EXISTS `sp_add_alarm_history_column`;

DROP PROCEDURE IF EXISTS `sp_add_alarm_history_index`;

CREATE PROCEDURE `sp_add_alarm_history_index`(
  IN p_table VARCHAR(64),
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

CALL `sp_add_alarm_history_index`('cpk_alarm_log', 'idx_alarm_archive_create_id', '(`create_at`, `id`)');
CALL `sp_add_alarm_history_index`('cpk_alarm_log', 'idx_alarm_location_create', '(`location`, `create_at`)');
CALL `sp_add_alarm_history_index`('cpk_alarm_log', 'idx_alarm_device_create', '(`device_id`, `create_at`)');
CALL `sp_add_alarm_history_index`('cpk_alarm_log', 'idx_alarm_fault_type_create', '(`fault_type`, `create_at`)');
CALL `sp_add_alarm_history_index`('flk_alarm_log', 'idx_alarm_archive_create_id', '(`create_at`, `id`)');
CALL `sp_add_alarm_history_index`('flk_alarm_log', 'idx_alarm_location_create', '(`location`, `create_at`)');
CALL `sp_add_alarm_history_index`('flk_alarm_log', 'idx_alarm_device_create', '(`device_id`, `create_at`)');
CALL `sp_add_alarm_history_index`('flk_alarm_log', 'idx_alarm_fault_type_create', '(`fault_type`, `create_at`)');
CALL `sp_add_alarm_history_index`('ylk_alarm_log', 'idx_alarm_archive_create_id', '(`create_at`, `id`)');
CALL `sp_add_alarm_history_index`('ylk_alarm_log', 'idx_alarm_location_create', '(`location`, `create_at`)');
CALL `sp_add_alarm_history_index`('ylk_alarm_log', 'idx_alarm_device_create', '(`device_id`, `create_at`)');
CALL `sp_add_alarm_history_index`('ylk_alarm_log', 'idx_alarm_fault_type_create', '(`fault_type`, `create_at`)');

DROP PROCEDURE IF EXISTS `sp_add_alarm_history_index`;

DROP PROCEDURE IF EXISTS `sp_refresh_daily_fault_stats`;

CREATE PROCEDURE `sp_refresh_daily_fault_stats`(IN p_stat_date DATE)
BEGIN
  DECLARE done BOOL DEFAULT FALSE;
  DECLARE v_source_table VARCHAR(64);
  DECLARE v_archive_table VARCHAR(80);
  DECLARE v_query_table VARCHAR(80);
  DECLARE v_table_exists INT DEFAULT 0;
  DECLARE v_month CHAR(6);
  DECLARE v_start DATETIME(3);
  DECLARE v_end DATETIME(3);
  DECLARE cur CURSOR FOR
    SELECT source_table
    FROM alarm_archive_tables
    WHERE enabled = 1
    ORDER BY source_table;
  DECLARE CONTINUE HANDLER FOR NOT FOUND SET done = TRUE;

  IF p_stat_date IS NULL THEN
    SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'p_stat_date must not be null';
  END IF;

  SET v_month = DATE_FORMAT(p_stat_date, '%Y%m');
  SET v_start = CAST(p_stat_date AS DATETIME(3));
  SET v_end = DATE_ADD(v_start, INTERVAL 1 DAY);
  SET @stat_start = v_start;
  SET @stat_end = v_end;

  DROP TEMPORARY TABLE IF EXISTS `tmp_daily_alarm_rows`;
  CREATE TEMPORARY TABLE `tmp_daily_alarm_rows` (
    `source_table` VARCHAR(64) NOT NULL,
    `location` VARCHAR(255) NOT NULL,
    `device_type` VARCHAR(100) NOT NULL,
    `device_id` VARCHAR(100) DEFAULT NULL,
    `fault_type` VARCHAR(100) NOT NULL,
    `tag_value` VARCHAR(255) DEFAULT NULL,
    KEY `idx_tmp_area` (`location`),
    KEY `idx_tmp_device_type` (`device_type`),
    KEY `idx_tmp_fault_type` (`fault_type`)
  ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

  OPEN cur;

  read_loop: LOOP
    FETCH cur INTO v_source_table;
    IF done THEN
      LEAVE read_loop;
    END IF;

    SET v_archive_table = CONCAT(v_source_table, '_', v_month);
    SELECT COUNT(*)
      INTO v_table_exists
    FROM INFORMATION_SCHEMA.TABLES
    WHERE TABLE_SCHEMA = DATABASE()
      AND TABLE_NAME = v_archive_table;

    IF p_stat_date < DATE_FORMAT(CURRENT_DATE(), '%Y-%m-01') AND v_table_exists > 0 THEN
      SET v_query_table = v_archive_table;
    ELSE
      SET v_query_table = v_source_table;
    END IF;

    SET @sql = CONCAT(
      'INSERT INTO `tmp_daily_alarm_rows` (`source_table`, `location`, `device_type`, `device_id`, `fault_type`, `tag_value`) ',
      'SELECT ',
      QUOTE(v_source_table), ', ',
      'COALESCE(NULLIF(TRIM(`location`), ''''), ''未分区''), ',
      'COALESCE(NULLIF(TRIM(`device`), ''''), ''未分类设备''), ',
      'NULLIF(TRIM(`device_id`), ''''), ',
      'COALESCE(NULLIF(TRIM(`fault_type`), ''''), NULLIF(TRIM(`tag`), ''''), ''未分类故障''), ',
      '`tag_value` ',
      'FROM `', v_query_table, '` ',
      'WHERE `create_at` >= ? AND `create_at` < ? ',
      'AND COALESCE(`tag_value`, ''0'') <> ''0'''
    );
    PREPARE stmt FROM @sql;
    EXECUTE stmt USING @stat_start, @stat_end;
    DEALLOCATE PREPARE stmt;
  END LOOP;

  CLOSE cur;

  DELETE FROM `daily_area_fault_stats` WHERE `stat_date` = p_stat_date;
  DELETE FROM `daily_device_type_fault_stats` WHERE `stat_date` = p_stat_date;
  DELETE FROM `daily_fault_type_stats` WHERE `stat_date` = p_stat_date;

  INSERT INTO `daily_area_fault_stats` (`stat_date`, `location`, `device_total`, `fault_count`)
  SELECT
    p_stat_date,
    r.`location`,
    COALESCE(
      NULLIF((SELECT COUNT(*) FROM `device_catalog` d WHERE d.`enabled` = 1 AND d.`location` = r.`location`), 0),
      COUNT(DISTINCT r.`device_id`)
    ) AS `device_total`,
    COUNT(*) AS `fault_count`
  FROM `tmp_daily_alarm_rows` r
  GROUP BY r.`location`;

  INSERT INTO `daily_device_type_fault_stats` (`stat_date`, `device_type`, `device_total`, `fault_count`)
  SELECT
    p_stat_date,
    r.`device_type`,
    COALESCE(
      NULLIF((SELECT COUNT(*) FROM `device_catalog` d WHERE d.`enabled` = 1 AND d.`device_type` = r.`device_type`), 0),
      COUNT(DISTINCT r.`device_id`)
    ) AS `device_total`,
    COUNT(*) AS `fault_count`
  FROM `tmp_daily_alarm_rows` r
  GROUP BY r.`device_type`;

  INSERT INTO `daily_fault_type_stats` (`stat_date`, `fault_type`, `device_count`, `fault_count`)
  SELECT
    p_stat_date,
    r.`fault_type`,
    COUNT(DISTINCT r.`device_id`) AS `device_count`,
    COUNT(*) AS `fault_count`
  FROM `tmp_daily_alarm_rows` r
  GROUP BY r.`fault_type`;

  DROP TEMPORARY TABLE IF EXISTS `tmp_daily_alarm_rows`;
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

    SET @sql = CONCAT(
      'INSERT INTO `', v_target_table, '` ',
      '(`id`, `location`, `device`, `device_id`, `node_id`, `alias`, `tag`, `fault_type`, `tag_state`, `tag_value`, `description`, `remark`, `create_at`, `update_at`) ',
      'SELECT s.`id`, s.`location`, s.`device`, s.`device_id`, s.`node_id`, s.`alias`, s.`tag`, s.`fault_type`, s.`tag_state`, s.`tag_value`, s.`description`, s.`remark`, s.`create_at`, s.`update_at` ',
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

DROP EVENT IF EXISTS `ev_refresh_daily_alarm_stats`;

CREATE EVENT `ev_refresh_daily_alarm_stats`
ON SCHEDULE EVERY 1 DAY
STARTS (CURRENT_DATE + INTERVAL 1 DAY + INTERVAL 10 MINUTE)
DO
  CALL `sp_refresh_daily_fault_stats`(CURRENT_DATE - INTERVAL 1 DAY);

DROP EVENT IF EXISTS `ev_archive_alarm_month`;

CREATE EVENT `ev_archive_alarm_month`
ON SCHEDULE EVERY 1 DAY
STARTS (CURRENT_DATE + INTERVAL 1 DAY + INTERVAL 1 HOUR)
DO
BEGIN
  IF DAYOFMONTH(CURRENT_DATE) = 1 THEN
    CALL `sp_refresh_daily_fault_stats`(CURRENT_DATE - INTERVAL 1 DAY);
    CALL `sp_archive_alarm_month`(DATE_FORMAT(CURRENT_DATE - INTERVAL 1 MONTH, '%Y%m'));
  END IF;
END;
