SET NAMES utf8mb4;

CREATE TABLE IF NOT EXISTS `field_value_catalog` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键ID',
  `field_name` VARCHAR(64) NOT NULL COMMENT '字段名，例如location、device_type',
  `field_code` VARCHAR(100) NOT NULL COMMENT '字段编码，例如WH_CP_Zone01、Conveyor',
  `field_label` VARCHAR(255) NOT NULL COMMENT '字段中文含义',
  `enabled` TINYINT(1) NOT NULL DEFAULT 1 COMMENT '启用状态，1启用，0停用',
  `sort_order` INT UNSIGNED NOT NULL DEFAULT 0 COMMENT '显示排序',
  `remark` TEXT DEFAULT NULL COMMENT '备注',
  `created_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) COMMENT '创建时间',
  `updated_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3) COMMENT '更新时间',
  PRIMARY KEY (`id`),
  UNIQUE KEY `uq_field_value_catalog` (`field_name`, `field_code`),
  KEY `idx_field_value_catalog_enabled` (`field_name`, `enabled`, `sort_order`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='字段编码含义字典表';

INSERT INTO `field_value_catalog` (`field_name`, `field_code`, `field_label`, `sort_order`) VALUES
  ('location', 'FSC1', '出库分拣机', 10),
  ('location', 'FSC2', '入库分拣机', 20),
  ('location', 'WH_CP_Zone01', '成品库（输送设备）', 30),
  ('location', 'WH_CP_Zone02', '成品库（德玛分拣01）', 40),
  ('location', 'WH_CP_Zone03', '成品库（德玛分拣02）', 50),
  ('location', 'WH_FLK_Zone01', '辅料库', 60),
  ('location', 'WH_YLK_Zone01', '原料库', 70),
  ('device_type', 'AreaControl', '片区控制', 10),
  ('device_type', 'ControlBox', '控制柜', 20),
  ('device_type', 'Conveyor', '输送线', 30),
  ('device_type', 'CSC01', '穿梭车', 40),
  ('device_type', 'CSC02', '穿梭车', 50),
  ('device_type', 'DM_CD', '拆垛机器人与站台交互', 60),
  ('device_type', 'DM_MD', '码垛机器人与站台交互', 70),
  ('device_type', 'SRM', '堆垛机与站台交互', 80),
  ('device_type', 'TapePunch', '打带机', 90),
  ('device_type', 'GROSSING1', '分拣输送线', 101),
  ('device_type', 'GROSSING2', '分拣输送线', 102),
  ('device_type', 'GROSSING3', '分拣输送线', 103),
  ('device_type', 'GROSSING4', '分拣输送线', 104),
  ('device_type', 'GROSSING5', '分拣输送线', 105),
  ('device_type', 'GROSSING6', '分拣输送线', 106),
  ('device_type', 'GROSSING7', '分拣输送线', 107),
  ('device_type', 'GROSSING8', '分拣输送线', 108),
  ('device_type', 'GROSSING9', '分拣输送线', 109),
  ('device_type', 'GROSSING10', '分拣输送线', 110),
  ('device_type', 'GROSSING11', '分拣输送线', 111),
  ('device_type', 'GROSSING12', '分拣输送线', 112),
  ('device_type', 'GROSSING13', '分拣输送线', 113),
  ('device_type', 'gjk_conveyor', '高架库输送线', 130),
  ('device_type', 'hck_conveyor', '缓存库输送线', 140),
  ('device_type', 'ClampingMachine', '夹包机', 150),
  ('device_type', 'Hoister', '提升机', 160),
  ('device_type', 'Crane', '堆垛机', 170),
  ('device_type', 'RGV', '穿梭车', 180)
ON DUPLICATE KEY UPDATE
  `field_label` = VALUES(`field_label`),
  `sort_order` = VALUES(`sort_order`),
  `enabled` = 1,
  `updated_at` = CURRENT_TIMESTAMP(3);

CREATE OR REPLACE VIEW `cpk_alarm_log_enriched` AS
SELECT
  h.*,
  loc.`field_label` AS `location_label`,
  dev.`field_label` AS `device_type_label`
FROM `cpk_alarm_log` h
LEFT JOIN `field_value_catalog` loc
  ON loc.`field_name` = 'location'
 AND loc.`field_code` = h.`location`
 AND loc.`enabled` = 1
LEFT JOIN `field_value_catalog` dev
  ON dev.`field_name` = 'device_type'
 AND dev.`field_code` = h.`device`
 AND dev.`enabled` = 1;

CREATE OR REPLACE VIEW `flk_alarm_log_enriched` AS
SELECT
  h.*,
  loc.`field_label` AS `location_label`,
  dev.`field_label` AS `device_type_label`
FROM `flk_alarm_log` h
LEFT JOIN `field_value_catalog` loc
  ON loc.`field_name` = 'location'
 AND loc.`field_code` = h.`location`
 AND loc.`enabled` = 1
LEFT JOIN `field_value_catalog` dev
  ON dev.`field_name` = 'device_type'
 AND dev.`field_code` = h.`device`
 AND dev.`enabled` = 1;

CREATE OR REPLACE VIEW `ylk_alarm_log_enriched` AS
SELECT
  h.*,
  loc.`field_label` AS `location_label`,
  dev.`field_label` AS `device_type_label`
FROM `ylk_alarm_log` h
LEFT JOIN `field_value_catalog` loc
  ON loc.`field_name` = 'location'
 AND loc.`field_code` = h.`location`
 AND loc.`enabled` = 1
LEFT JOIN `field_value_catalog` dev
  ON dev.`field_name` = 'device_type'
 AND dev.`field_code` = h.`device`
 AND dev.`enabled` = 1;

CREATE OR REPLACE VIEW `device_realtime_status_enriched` AS
SELECT
  r.*,
  loc.`field_label` AS `location_label`,
  dev.`field_label` AS `device_type_label`
FROM `device_realtime_status` r
LEFT JOIN `field_value_catalog` loc
  ON loc.`field_name` = 'location'
 AND loc.`field_code` = r.`location`
 AND loc.`enabled` = 1
LEFT JOIN `field_value_catalog` dev
  ON dev.`field_name` = 'device_type'
 AND dev.`field_code` = r.`device_type`
 AND dev.`enabled` = 1;
