SET @schema_name = DATABASE();

SET @sql = (
  SELECT IF(
    COUNT(*) = 0,
    'ALTER TABLE `cpk_alarm_log` ADD COLUMN `remark` TEXT DEFAULT NULL COMMENT ''故障备注'' AFTER `description`',
    'SELECT 1'
  )
  FROM INFORMATION_SCHEMA.COLUMNS
  WHERE TABLE_SCHEMA = @schema_name
    AND TABLE_NAME = 'cpk_alarm_log'
    AND COLUMN_NAME = 'remark'
);
PREPARE stmt FROM @sql;
EXECUTE stmt;
DEALLOCATE PREPARE stmt;

SET @sql = (
  SELECT IF(
    COUNT(*) = 0,
    'ALTER TABLE `flk_alarm_log` ADD COLUMN `remark` TEXT DEFAULT NULL COMMENT ''故障备注'' AFTER `description`',
    'SELECT 1'
  )
  FROM INFORMATION_SCHEMA.COLUMNS
  WHERE TABLE_SCHEMA = @schema_name
    AND TABLE_NAME = 'flk_alarm_log'
    AND COLUMN_NAME = 'remark'
);
PREPARE stmt FROM @sql;
EXECUTE stmt;
DEALLOCATE PREPARE stmt;

SET @sql = (
  SELECT IF(
    COUNT(*) = 0,
    'ALTER TABLE `ylk_alarm_log` ADD COLUMN `remark` TEXT DEFAULT NULL COMMENT ''故障备注'' AFTER `description`',
    'SELECT 1'
  )
  FROM INFORMATION_SCHEMA.COLUMNS
  WHERE TABLE_SCHEMA = @schema_name
    AND TABLE_NAME = 'ylk_alarm_log'
    AND COLUMN_NAME = 'remark'
);
PREPARE stmt FROM @sql;
EXECUTE stmt;
DEALLOCATE PREPARE stmt;
