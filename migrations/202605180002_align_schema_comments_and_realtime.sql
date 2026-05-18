SET NAMES utf8mb4;

ALTER TABLE `cpk_alarm_log` COMMENT = '设备故障历史热表-成品库/分拣线：保存采集到的故障事实，当月在线写入，历史月份归档到月度结存表';
ALTER TABLE `flk_alarm_log` COMMENT = '设备故障历史热表-辅料库：保存采集到的故障事实，当月在线写入，历史月份归档到月度结存表';
ALTER TABLE `ylk_alarm_log` COMMENT = '设备故障历史热表-原料库：保存采集到的故障事实，当月在线写入，历史月份归档到月度结存表';

ALTER TABLE `cpk_alarm_log`
  MODIFY COLUMN `id` BIGINT NOT NULL AUTO_INCREMENT COMMENT '主键ID',
  MODIFY COLUMN `location` VARCHAR(255) DEFAULT NULL COMMENT '库区或业务区域，例如成品库、辅料库、原料库、FSC1、FSC2',
  MODIFY COLUMN `device` VARCHAR(255) DEFAULT NULL COMMENT '设备类型或设备名称，来自点位路径或设备档案',
  MODIFY COLUMN `device_id` VARCHAR(100) DEFAULT NULL COMMENT '设备ID/编号，用于关联设备档案和统计维度',
  MODIFY COLUMN `node_id` VARCHAR(255) DEFAULT NULL COMMENT 'OPC UA NodeId，标签基础表优先按此字段匹配',
  MODIFY COLUMN `alias` VARCHAR(255) DEFAULT NULL COMMENT '采集别名，通常为不带 ns 前缀的完整点位路径',
  MODIFY COLUMN `tag` VARCHAR(255) DEFAULT NULL COMMENT '故障标签，通常为 Alarm 后的标签名或解析后的业务标签',
  MODIFY COLUMN `fault_type` VARCHAR(100) DEFAULT NULL COMMENT '故障类型，由标签基础表映射得到，例如货叉、光电、安全装置、断路器、超边超偏、驱动故障、通讯故障PPI、通讯/上位信号、驱动/定位控制',
  MODIFY COLUMN `tag_state` VARCHAR(100) DEFAULT NULL COMMENT '标签原始状态值，保留 OPC UA 采集到的原始值',
  MODIFY COLUMN `tag_value` VARCHAR(255) DEFAULT NULL COMMENT '故障值，统一表达是否故障；0为非故障，1为故障，特殊状态按规则转换',
  MODIFY COLUMN `description` TEXT DEFAULT NULL COMMENT '故障描述，来自 Kepware 描述或 description_map 映射',
  MODIFY COLUMN `create_at` DATETIME DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间/入库时间',
  MODIFY COLUMN `update_at` DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间，优先承载采集源时间或后续更新操作时间',
  MODIFY COLUMN `remark` TEXT DEFAULT NULL COMMENT '备注或解析后的状态说明，例如数字状态码对应的正常、错误、警告';

ALTER TABLE `flk_alarm_log`
  MODIFY COLUMN `id` BIGINT NOT NULL AUTO_INCREMENT COMMENT '主键ID',
  MODIFY COLUMN `location` VARCHAR(255) DEFAULT NULL COMMENT '库区或业务区域，例如成品库、辅料库、原料库、FSC1、FSC2',
  MODIFY COLUMN `device` VARCHAR(255) DEFAULT NULL COMMENT '设备类型或设备名称，来自点位路径或设备档案',
  MODIFY COLUMN `device_id` VARCHAR(100) DEFAULT NULL COMMENT '设备ID/编号，用于关联设备档案和统计维度',
  MODIFY COLUMN `node_id` VARCHAR(255) DEFAULT NULL COMMENT 'OPC UA NodeId，标签基础表优先按此字段匹配',
  MODIFY COLUMN `alias` VARCHAR(255) DEFAULT NULL COMMENT '采集别名，通常为不带 ns 前缀的完整点位路径',
  MODIFY COLUMN `tag` VARCHAR(255) DEFAULT NULL COMMENT '故障标签，通常为 Alarm 后的标签名或解析后的业务标签',
  MODIFY COLUMN `fault_type` VARCHAR(100) DEFAULT NULL COMMENT '故障类型，由标签基础表映射得到，例如货叉、光电、安全装置、断路器、超边超偏、驱动故障、通讯故障PPI、通讯/上位信号、驱动/定位控制',
  MODIFY COLUMN `tag_state` VARCHAR(100) DEFAULT NULL COMMENT '标签原始状态值，保留 OPC UA 采集到的原始值',
  MODIFY COLUMN `tag_value` VARCHAR(255) DEFAULT NULL COMMENT '故障值，统一表达是否故障；0为非故障，1为故障，特殊状态按规则转换',
  MODIFY COLUMN `description` TEXT DEFAULT NULL COMMENT '故障描述，来自 Kepware 描述或 description_map 映射',
  MODIFY COLUMN `create_at` DATETIME DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间/入库时间',
  MODIFY COLUMN `update_at` DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间，优先承载采集源时间或后续更新操作时间',
  MODIFY COLUMN `remark` TEXT DEFAULT NULL COMMENT '备注或解析后的状态说明，例如数字状态码对应的正常、错误、警告';

ALTER TABLE `ylk_alarm_log`
  MODIFY COLUMN `id` BIGINT NOT NULL AUTO_INCREMENT COMMENT '主键ID',
  MODIFY COLUMN `location` VARCHAR(255) DEFAULT NULL COMMENT '库区或业务区域，例如成品库、辅料库、原料库、FSC1、FSC2',
  MODIFY COLUMN `device` VARCHAR(255) DEFAULT NULL COMMENT '设备类型或设备名称，来自点位路径或设备档案',
  MODIFY COLUMN `device_id` VARCHAR(100) DEFAULT NULL COMMENT '设备ID/编号，用于关联设备档案和统计维度',
  MODIFY COLUMN `node_id` VARCHAR(255) DEFAULT NULL COMMENT 'OPC UA NodeId，标签基础表优先按此字段匹配',
  MODIFY COLUMN `alias` VARCHAR(255) DEFAULT NULL COMMENT '采集别名，通常为不带 ns 前缀的完整点位路径',
  MODIFY COLUMN `tag` VARCHAR(255) DEFAULT NULL COMMENT '故障标签，通常为 Alarm 后的标签名或解析后的业务标签',
  MODIFY COLUMN `fault_type` VARCHAR(100) DEFAULT NULL COMMENT '故障类型，由标签基础表映射得到，例如货叉、光电、安全装置、断路器、超边超偏、驱动故障、通讯故障PPI、通讯/上位信号、驱动/定位控制',
  MODIFY COLUMN `tag_state` VARCHAR(100) DEFAULT NULL COMMENT '标签原始状态值，保留 OPC UA 采集到的原始值',
  MODIFY COLUMN `tag_value` VARCHAR(255) DEFAULT NULL COMMENT '故障值，统一表达是否故障；0为非故障，1为故障，特殊状态按规则转换',
  MODIFY COLUMN `description` TEXT DEFAULT NULL COMMENT '故障描述，来自 Kepware 描述或 description_map 映射',
  MODIFY COLUMN `create_at` DATETIME DEFAULT CURRENT_TIMESTAMP COMMENT '创建时间/入库时间',
  MODIFY COLUMN `update_at` DATETIME DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP COMMENT '更新时间，优先承载采集源时间或后续更新操作时间',
  MODIFY COLUMN `remark` TEXT DEFAULT NULL COMMENT '备注或解析后的状态说明，例如数字状态码对应的正常、错误、警告';

ALTER TABLE `device_catalog`
  COMMENT = '01_设备档案基表：保存库区、设备名称、设备类型、设备编号、设备编码等相对稳定的主数据，不保存实时故障值或历史事实',
  MODIFY COLUMN `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键ID',
  MODIFY COLUMN `location` VARCHAR(255) NOT NULL COMMENT '库区，例如成品库、辅料库、原料库、分拣线',
  MODIFY COLUMN `device_name` VARCHAR(255) NOT NULL COMMENT '设备名称，用于页面展示和运维识别',
  MODIFY COLUMN `device_type` VARCHAR(100) NOT NULL COMMENT '设备类型，例如堆垛机、输送机、PPI通讯模块',
  MODIFY COLUMN `device_id` VARCHAR(100) NOT NULL COMMENT '设备编号，和历史表/实时表的 device_id 对应',
  MODIFY COLUMN `device_code` VARCHAR(100) DEFAULT NULL COMMENT '设备编码，作为后续系统对接的稳定ID',
  MODIFY COLUMN `model_spec` VARCHAR(255) DEFAULT NULL COMMENT '型号规格，可先为空',
  MODIFY COLUMN `commissioned_at` DATE DEFAULT NULL COMMENT '投产时间，可先为空',
  MODIFY COLUMN `install_location` VARCHAR(255) DEFAULT NULL COMMENT '投产区域或现场安装位置',
  MODIFY COLUMN `remark` TEXT DEFAULT NULL COMMENT '备注，记录该设备下挂载的标签类型或维护说明',
  MODIFY COLUMN `enabled` TINYINT(1) NOT NULL DEFAULT 1 COMMENT '启用状态，1启用，0停用',
  MODIFY COLUMN `created_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) COMMENT '创建时间',
  MODIFY COLUMN `updated_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3) COMMENT '更新时间';

ALTER TABLE `fault_type_catalog`
  COMMENT = '故障类型基础表：保存标签可归属的主分类，供标签基础表和统计层引用',
  MODIFY COLUMN `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键ID',
  MODIFY COLUMN `fault_type_code` VARCHAR(64) NOT NULL COMMENT '故障类型编码，例如 fork、photoelectric、safety_device',
  MODIFY COLUMN `fault_type_name` VARCHAR(100) NOT NULL COMMENT '故障类型名称，例如货叉、光电、安全装置',
  MODIFY COLUMN `enabled` TINYINT(1) NOT NULL DEFAULT 1 COMMENT '启用状态，1启用，0停用',
  MODIFY COLUMN `created_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) COMMENT '创建时间',
  MODIFY COLUMN `updated_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3) COMMENT '更新时间';

ALTER TABLE `tag_catalog`
  COMMENT = '02_标签基础表：静态标签索引表，只记录标签归属设备和故障类型，不记录实时值',
  MODIFY COLUMN `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键ID',
  MODIFY COLUMN `location` VARCHAR(255) DEFAULT NULL COMMENT '库区，和设备档案/历史表的 location 对应',
  MODIFY COLUMN `device_type` VARCHAR(100) DEFAULT NULL COMMENT '设备类型，和设备档案的 device_type 对应',
  MODIFY COLUMN `device_id` VARCHAR(100) DEFAULT NULL COMMENT '设备编号，和设备档案/历史表的 device_id 对应',
  MODIFY COLUMN `node_id` VARCHAR(255) DEFAULT NULL COMMENT 'OPC UA NodeId，程序优先用此字段匹配标签',
  MODIFY COLUMN `alias` VARCHAR(255) DEFAULT NULL COMMENT '采集别名，通常是不带 ns 前缀的完整点位路径',
  MODIFY COLUMN `tag` VARCHAR(255) NOT NULL COMMENT '故障标签名称，用于人工识别和兜底匹配',
  MODIFY COLUMN `fault_type_id` BIGINT UNSIGNED DEFAULT NULL COMMENT '故障类型ID，关联 fault_type_catalog；一条标签原则上只维护一个主分类',
  MODIFY COLUMN `enabled` TINYINT(1) NOT NULL DEFAULT 1 COMMENT '启用状态，1启用，0停用',
  MODIFY COLUMN `remark` TEXT DEFAULT NULL COMMENT '备注，保存标签说明、分类依据或维护说明',
  MODIFY COLUMN `created_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) COMMENT '创建时间',
  MODIFY COLUMN `updated_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3) COMMENT '更新时间';

CREATE TABLE IF NOT EXISTS `device_realtime_status` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键ID',
  `source_table` VARCHAR(64) NOT NULL COMMENT '来源历史热表，例如 cpk_alarm_log、flk_alarm_log、ylk_alarm_log',
  `location` VARCHAR(255) DEFAULT NULL COMMENT '库区或业务区域',
  `device_type` VARCHAR(100) DEFAULT NULL COMMENT '设备类型',
  `device_id` VARCHAR(100) DEFAULT NULL COMMENT '设备ID/编号',
  `device_name` VARCHAR(255) DEFAULT NULL COMMENT '设备名称，优先来自设备档案',
  `node_id` VARCHAR(255) DEFAULT NULL COMMENT 'OPC UA NodeId',
  `alias` VARCHAR(255) DEFAULT NULL COMMENT '采集别名',
  `tag` VARCHAR(255) NOT NULL COMMENT '故障标签',
  `fault_type` VARCHAR(100) DEFAULT NULL COMMENT '故障类型，由标签基础表映射得到',
  `tag_state` VARCHAR(100) DEFAULT NULL COMMENT '标签原始状态值',
  `tag_value` VARCHAR(255) DEFAULT NULL COMMENT '故障值，0非故障，1故障',
  `description` TEXT DEFAULT NULL COMMENT '故障描述',
  `status_description` TEXT DEFAULT NULL COMMENT '状态说明，通常为历史表 remark 的当前值',
  `last_fault_at` DATETIME DEFAULT NULL COMMENT '最近一次采集源时间或故障时间',
  `updated_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3) COMMENT '本行最近更新时间',
  PRIMARY KEY (`id`),
  UNIQUE KEY `uq_device_realtime_source_node` (`source_table`, `node_id`),
  KEY `idx_device_realtime_device` (`location`, `device_type`, `device_id`),
  KEY `idx_device_realtime_fault_type` (`fault_type`),
  KEY `idx_device_realtime_value` (`tag_value`, `updated_at`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='03_设备实时状态表：只保存每个设备/标签的当前状态，用于看板和快速查询；历史追溯以故障历史表为准';

ALTER TABLE `alarm_archive_tables`
  COMMENT = '故障历史归档源表配置：记录哪些热表参与月度结存',
  MODIFY COLUMN `source_table` VARCHAR(64) NOT NULL COMMENT '历史热表名称',
  MODIFY COLUMN `enabled` TINYINT(1) NOT NULL DEFAULT 1 COMMENT '启用状态，1参与归档，0不参与',
  MODIFY COLUMN `created_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) COMMENT '创建时间';

ALTER TABLE `alarm_archive_runs`
  COMMENT = '故障历史月度归档记录表：记录每次归档的复制、校验、删除结果',
  MODIFY COLUMN `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT COMMENT '主键ID',
  MODIFY COLUMN `archive_month` CHAR(6) NOT NULL COMMENT '归档月份，格式YYYYMM',
  MODIFY COLUMN `source_table` VARCHAR(64) NOT NULL COMMENT '归档源热表',
  MODIFY COLUMN `target_table` VARCHAR(80) NOT NULL COMMENT '月度结存目标表',
  MODIFY COLUMN `range_start` DATETIME(3) NOT NULL COMMENT '归档数据起始时间，闭区间',
  MODIFY COLUMN `range_end` DATETIME(3) NOT NULL COMMENT '归档数据结束时间，开区间',
  MODIFY COLUMN `started_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) COMMENT '归档开始时间',
  MODIFY COLUMN `finished_at` DATETIME(3) DEFAULT NULL COMMENT '归档结束时间',
  MODIFY COLUMN `copied_rows` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '复制到月表的行数',
  MODIFY COLUMN `deleted_rows` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '从热表删除的行数',
  MODIFY COLUMN `status` ENUM('running', 'completed', 'failed') NOT NULL DEFAULT 'running' COMMENT '归档状态',
  MODIFY COLUMN `error_message` TEXT DEFAULT NULL COMMENT '失败错误信息';

ALTER TABLE `daily_area_fault_stats`
  COMMENT = '05_基础统计模板-区域故障统计表：按库区和统计日期汇总设备总数、故障次数',
  MODIFY COLUMN `stat_date` DATE NOT NULL COMMENT '统计日期',
  MODIFY COLUMN `location` VARCHAR(255) NOT NULL COMMENT '库区',
  MODIFY COLUMN `device_total` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '设备总数',
  MODIFY COLUMN `fault_count` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '故障次数',
  MODIFY COLUMN `updated_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3) COMMENT '更新时间';

ALTER TABLE `daily_device_type_fault_stats`
  COMMENT = '05_基础统计模板-设备类型统计表：按设备类型和统计日期汇总设备总数、故障次数',
  MODIFY COLUMN `stat_date` DATE NOT NULL COMMENT '统计日期',
  MODIFY COLUMN `device_type` VARCHAR(100) NOT NULL COMMENT '设备类型',
  MODIFY COLUMN `device_total` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '设备总数',
  MODIFY COLUMN `fault_count` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '故障次数',
  MODIFY COLUMN `updated_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3) COMMENT '更新时间';

ALTER TABLE `daily_fault_type_stats`
  COMMENT = '05_基础统计模板-故障类型统计表：按故障类型和统计日期汇总涉及设备数、故障次数',
  MODIFY COLUMN `stat_date` DATE NOT NULL COMMENT '统计日期',
  MODIFY COLUMN `fault_type` VARCHAR(100) NOT NULL COMMENT '故障类型',
  MODIFY COLUMN `device_count` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '涉及设备数',
  MODIFY COLUMN `fault_count` BIGINT UNSIGNED NOT NULL DEFAULT 0 COMMENT '故障次数',
  MODIFY COLUMN `updated_at` DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3) COMMENT '更新时间';

CREATE OR REPLACE VIEW `tag_fault_type_design_view` AS
SELECT
  t.`location` AS `库区`,
  t.`device_type` AS `设备类型`,
  t.`device_id` AS `设备编号`,
  t.`tag` AS `标签`,
  CASE WHEN ft.`fault_type_code` = 'fork' THEN 1 ELSE NULL END AS `货叉`,
  CASE WHEN ft.`fault_type_code` = 'photoelectric' THEN 1 ELSE NULL END AS `光电`,
  CASE WHEN ft.`fault_type_code` = 'safety_device' THEN 1 ELSE NULL END AS `安全装置`,
  CASE WHEN ft.`fault_type_code` = 'breaker' THEN 1 ELSE NULL END AS `断路器`,
  CASE WHEN ft.`fault_type_code` = 'oversize_offset' THEN 1 ELSE NULL END AS `超边超偏`,
  CASE WHEN ft.`fault_type_code` = 'drive_fault' THEN 1 ELSE NULL END AS `驱动故障`,
  CASE WHEN ft.`fault_type_code` = 'ppi_comm' THEN 1 ELSE NULL END AS `通讯故障PPI`,
  CASE WHEN ft.`fault_type_code` = 'upper_signal' THEN 1 ELSE NULL END AS `通讯/上位信号`,
  CASE WHEN ft.`fault_type_code` = 'drive_positioning' THEN 1 ELSE NULL END AS `驱动/定位控制`,
  CASE WHEN t.`enabled` = 1 THEN '启用' ELSE '停用' END AS `启用状态`,
  t.`remark` AS `备注`,
  t.`node_id` AS `OPC UA NodeId`,
  t.`alias` AS `采集别名`
FROM `tag_catalog` t
LEFT JOIN `fault_type_catalog` ft ON ft.`id` = t.`fault_type_id`;
