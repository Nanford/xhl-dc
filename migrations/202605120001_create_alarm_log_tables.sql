CREATE TABLE IF NOT EXISTS `cpk_alarm_log` (
  `id` BIGINT NOT NULL AUTO_INCREMENT COMMENT '主键ID',
  `location` VARCHAR(255) DEFAULT NULL COMMENT '位置',
  `device` VARCHAR(255) DEFAULT NULL COMMENT '设备',
  `device_id` VARCHAR(100) DEFAULT NULL COMMENT '设备ID',
  `tag` VARCHAR(255) DEFAULT NULL COMMENT '标签',
  `tag_state` VARCHAR(100) DEFAULT NULL COMMENT '标签状态',
  `tag_value` VARCHAR(255) DEFAULT NULL COMMENT '标签值',
  `description` TEXT DEFAULT NULL COMMENT '故障描述',
  `create_at` DATETIME DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间',
  `update_at` DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间',
  PRIMARY KEY (`id`),
  KEY `idx_device_id` (`device_id`),
  KEY `idx_tag` (`tag`),
  KEY `idx_create_at` (`create_at`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='CPK报警日志表';

CREATE TABLE IF NOT EXISTS `flk_alarm_log` LIKE `cpk_alarm_log`;
ALTER TABLE `flk_alarm_log` COMMENT='FLK报警日志表';

CREATE TABLE IF NOT EXISTS `ylk_alarm_log` LIKE `cpk_alarm_log`;
ALTER TABLE `ylk_alarm_log` COMMENT='YLK报警日志表';
