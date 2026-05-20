# Kepware Bridge

工业数据采集服务，从 Kepware/KEPServerEX OPC UA 订阅点位变化，批量写入 MySQL。写库失败时先落到本地 sled 缓冲，后续自动回灌，避免数据库短暂不可用导致数据直接丢失。

## 当前写库目标

报警数据先写入三张结构一致的历史热表，按点位第一个字段路由：

- `cpk_alarm_log`
- `flk_alarm_log`
- `ylk_alarm_log`

服务启动后会把已加载的 `subscriptions[].tags` 预置到 `device_realtime_status`。采样批量写入历史热表成功后，同一批点位会同步 upsert 到 `device_realtime_status`，看板和快速查询直接读取该实时状态表；历史追溯仍以三张历史热表为准。

`config.yaml` 中的 `sink.table` 是兜底表，`sink.tag_prefix_routes` 是区域编码到目标表的对照表。现场新增区域编码时，只需要改配置，不需要重新编译。

```yaml
sink:
  table: "ylk_alarm_log"
  tag_prefix_routes:
    WH_CP_Zone01:
      table: "cpk_alarm_log"
    FSC1:
      table: "cpk_alarm_log"
    FSC2:
      table: "cpk_alarm_log"
    FLK1:
      table: "flk_alarm_log"
    YLK1:
      table: "ylk_alarm_log"
  batch_size: 500
  flush_interval_ms: 1000
```

## 字段解析规则

采集到的 OPC UA `node_id` 通常形如 `ns=2;s=完整点位路径`。程序优先从 `;s=` 后面的完整点位路径解析；如果不是 OPC UA 字符串 NodeId，则回退使用 `alias`。

### WH_CP_Zone01 报警点位

示例：

```text
WH_CP_Zone01.Convey.Conveyor.M5035.Alarm.DriveFault
WH_CP_Zone01.Convey.CSC02.Alarm.FROM_CSC02_SSJCS_Fault
```

写库字段：

- `location`: `WH_CP_Zone01`
- `device`: `Conveyor`；如果没有设备分类层级，则取 `CSC02`
- `device_id`: `M5035`；如果没有下一层设备编号，则与 `device` 相同
- `tag`: `Alarm` 后面的最后点位名，如 `DriveFault`
- `tag_state`: 原始采样值字符串，如 `true`、`false`、`6`
- `tag_value`: 解析后的报警标志，只写 `0` 或 `1`
- `description`: 优先写入 Kepware 标签说明；读取不到时留空，或由映射文件补充
- `remark`: 普通点位与 `description` 一致

### FSC/Iscs 点位

示例：

```text
FSC2.InBound.Iscs.BF-1_1_1.MTR-1_1_1_MTR.Details.DS
FSC1.OutBound.Iscs.BC-1_0_0-BC5191.MTR-1_0_0-BC5191_MTR.Details.DS
```

写库字段：

- `location`: `FSC2` 或 `FSC1`
- `device`: `BF-1_1_1`、`BC-1_0_0-BC5191`
- `device_id`: 从设备编码中提取中间编号，如 `1_1_1`、`1_0_0`
- `tag`: 从 `Iscs` 开始保留完整后缀，如 `Iscs.BF-1_1_1.MTR-1_1_1_MTR.Details.DS`
- `tag_state`: 原始采样值字符串
- `tag_value`: 解析后的报警标志，只写 `0` 或 `1`；数字状态优先按描述中的码表判断，例如 `正常`、`运行中` 写 `0`，`错误`、`警告`、`槽满`、`离线` 写 `1`
- `description`: 优先写入 Kepware 标签说明；读取不到时留空，或由映射文件补充
- `remark`: 按当前原始值解析 `description` 中的码表后写入状态详情，例如 `皮带输送机BF-1.5.3扫描仪状态：512、离线；1024、错误；8192、运行中；` 在当前值为 `1024` 时写入 `皮带输送机BF-1.5.3扫描仪状态错误`

## 故障描述来源

Kepware 标签属性编辑器中的“说明”可以通过属性点读取。程序会在创建订阅前，对每个采集点尝试读取同路径的 `._Description` 属性点，例如：

```text
ns=2;s=WH_CPK_Zone01.convey.conveyor.M5001.Alarm.error1
ns=2;s=WH_CPK_Zone01.convey.conveyor.M5001.Alarm.error1._Description
```

读取成功后，报警入库的 `description` 字段写入 Kepware 中维护的中文说明，例如 `错误1`。这一步只发生在订阅初始化阶段，不进入 OPC UA 数据变更回调。

描述优先级：

1. `subscriptions[].tags[].description`
2. `opcua.description_map_path` 指向的映射文件
3. Kepware `Tag._Description`
4. OPC UA 标准 `Description` 属性

现场点位很多时，不建议在 `subscriptions[].tags` 里手工维护所有中文描述。点位清单仍由配置或浏览脚本生成；描述优先从 Kepware 读取。只有 Kepware 没有维护说明、现场需要覆盖说明，或个别点位说明不规范时，再使用映射文件补充。

映射文件使用 YAML，键可以写完整 NodeId，也可以只写 `;s=` 后面的点位路径：

```yaml
ns=2;s=WH_CPK_Zone01.convey.conveyor.M5001.Alarm.error1: "错误1"
WH_CPK_Zone01.convey.conveyor.M5001.Alarm.error2: "错误2"
```

配置入口：

```yaml
opcua:
  description_map_path: "./description_map.yaml"
```

## 数据库表结构

生产库已存在三张报警表。仓库中的 `migrations/202605120001_create_alarm_log_tables.sql` 提供基础建表脚本，`migrations/202605130001_add_alarm_log_remark.sql` 为三张报警表补充 `remark` 故障备注字段。

```powershell
mysql -u root -p iot < .\migrations\202605120001_create_alarm_log_tables.sql
mysql -u root -p iot < .\migrations\202605130001_add_alarm_log_remark.sql
```

## 运行配置

默认读取 `config.yaml`。现场真实连接、账号密码和点位清单建议放在未提交的 `config.local.yaml`。

关键配置：

- `opcua.endpoint`: Kepware OPC UA 地址
- `opcua.description_map_path`: 可选描述映射文件；用于覆盖或补充 Kepware 说明
- `opcua.subscription_files`: 可选订阅文件列表；点位规模较大时，将成品库、辅料库等订阅清单拆到独立 YAML 文件中维护
- `opcua.monitored_item_create_batch_size_count`: 每次向 Kepware 创建 MonitoredItem 的点位数量，默认 500
- `mysql.url`: MySQL 连接串
- `subscriptions[].tags[].node_id`: OPC UA NodeId
- `subscriptions[].tags[].alias`: 点位别名；建议保留完整点位路径，便于非 OPC UA 来源复用同一套解析规则
- `sink.tag_prefix_routes`: 区域编码到报警表的路由表

大规模点位配置建议使用外部订阅文件：

```yaml
opcua:
  subscription_files:
    - "./points/subscriptions.cpk.yaml"
    - "./points/subscriptions.flk.yaml"
```

订阅文件可以使用包裹结构：

```yaml
subscriptions:
  - name: "flk_alarm_wh_flk_zone01_001"
    publishing_interval_ms: 500
    keep_alive_count: 10
    lifetime_count: 30
    tags:
      - { node_id: "ns=2;s=WH_FLK_Zone01.HCK_Convey.Conveyor.3001.Alarm.BFault", alias: "WH_FLK_Zone01.HCK_Convey.Conveyor.3001.Alarm.BFault" }
```

运行：

```powershell
$env:RUST_LOG = "info,sqlx=warn,opcua=warn"
cargo run -- --config .\config.yaml
```

编译 release：

```powershell
cargo build --release
.\target\release\kepware-bridge.exe --config .\config.yaml
```

## 点位发现

### 从 XML 导入 CPK 报警点位

成品库报警点位以 XML 清单维护时，使用导入脚本生成订阅配置和描述映射文件：

```powershell
python .\scripts\import_opcua_alarm_points.py C:\Users\nanfo\Downloads\opcua_alarm_points.xml --config .\config.local.yaml --description-map .\description_map.cpk.yaml --write
```

点位规模较大时，可以把订阅写到独立文件，并在主配置中只保留文件引用：

```powershell
python .\scripts\import_opcua_alarm_points.py C:\Users\nanfo\Downloads\HCK_GJK_Convey_opcua_points_updated.xml --config .\config.local.yaml --description-map .\points\description_map.yaml --subscriptions-output .\points\subscriptions.flk.yaml --subscription-file-ref ./points/subscriptions.flk.yaml --subscription-prefix flk_alarm --route-table flk_alarm_log --merge-description-map --write
```

导入结果：

- `subscriptions`: 按点位第一段前缀分组，并按每 1000 点切分订阅，例如 `cpk_alarm_wh_cp_zone01_001`
- `subscriptions[].tags[].node_id`: 固定写成 `ns=2;s=<TagName>`
- `subscriptions[].tags[].alias`: 保留完整 `TagName`
- `description_map.cpk.yaml`: 记录 XML 中的中文故障描述，避免在主配置里塞入 8803 条说明
- `sink.tag_prefix_routes`: 自动补齐 XML 中出现的前缀，并路由到 `cpk_alarm_log`
- `opcua.discovery.enabled`: 自动置为 `false`，避免旧的在线发现边界和生成后的订阅名冲突

不带 `--write` 时只做校验和统计，不修改配置：

```powershell
python .\scripts\import_opcua_alarm_points.py C:\Users\nanfo\Downloads\opcua_alarm_points.xml --config .\config.local.yaml
```

现场真实连接、账号密码、生成后的 `config.local.yaml` 和 `description_map.cpk.yaml` 作为本地运行配置保存，不提交到仓库。

### 从 Kepware 在线浏览点位

辅助浏览工具可以从 Kepware 输出点位清单：

```powershell
cargo run --bin browse_tags -- opc.tcp://127.0.0.1:49320 8
```

输出文件：

- `browse_output.txt`: 全量浏览结果
- `browse_alarm.txt`: 按故障/报警关键词筛选后的结果

Python 脚本支持按配置边界写回订阅点位：

```powershell
python .\scripts\opcua_browse_tags.py --config .\config.local.yaml --write
```

## 中文字段与字符集

报警表中的 `location`、`device`、`device_id`、`tag`、`tag_state`、`tag_value` 使用 `VARCHAR`，`description`、`remark` 使用 `TEXT`。这组类型可以存储中文报警描述和故障备注，不需要改成 JSON 或二进制字段。

建库和建表脚本使用 `utf8mb4`：

```sql
CREATE DATABASE IF NOT EXISTS `iot_alarm_sc` DEFAULT CHARSET utf8mb4;
...
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
```

MySQL 8 环境下显示为 `utf8mb4_0900_ai_ci` 是正常结果。连接串可以显式带上字符集，避免客户端连接沿用非 UTF-8 默认值：

```yaml
mysql:
  url: "mysql://user:pass@127.0.0.1:3306/iot_alarm_sc?ssl-mode=DISABLED&charset=utf8mb4"
```

## 运行约束

- OPC UA 回调只做轻量转换和 `mpsc::try_send`，不直接写数据库。
- Sink Worker 负责批量写入、失败缓冲和重启后回灌。
- `buffer.path` 必须放在持久化磁盘目录。
- 生产环境建议使用 `Basic256Sha256 + Sign and Encrypt + 用户名密码`。
- 日志输出到 stdout/stderr，metrics 默认绑定 `127.0.0.1:9090`。
## 数据库 V2：月度结存与统计

故障历史仍写入在线热表：`cpk_alarm_log`、`flk_alarm_log`、`ylk_alarm_log`。在线热表保留当月数据，上月及更早数据按月归档到独立结存表，例如 `cpk_alarm_log_202605`、`flk_alarm_log_202605`、`ylk_alarm_log_202605`。

月度归档由数据库存储过程执行：

```sql
CALL sp_archive_alarm_month('202605');
```

归档过程会先创建月度结存表，再复制指定月份数据，确认目标表行数不低于源表后，按 5000 行一批从热表删除旧数据。每次执行结果写入 `alarm_archive_runs`，包含归档月份、源表、目标表、复制行数、删除行数、状态和错误信息。

基础统计保留三张独立物理表：

- `daily_area_fault_stats`：区域故障统计表
- `daily_device_type_fault_stats`：设备类型统计表
- `daily_fault_type_stats`：故障类型统计表

统计刷新由存储过程执行：

```sql
CALL sp_refresh_daily_fault_stats('2026-05-15');
```

MySQL Event 会每天刷新昨日统计；每月 1 日先刷新昨日统计，再归档上月历史数据。现场数据库需要开启 Event Scheduler：

```sql
SET GLOBAL event_scheduler = ON;
```

历史查询由程序根据时间范围选择物理表：当月查询访问热表，历史月份查询访问对应月表，跨月查询使用 `UNION ALL` 合并结果。查询条件必须包含时间范围，并优先带上库区、设备编号或故障类型条件，避免跨大量月表全量扫描。

标签基础信息保存在 `tag_catalog`，故障类型保存在 `fault_type_catalog`。程序启动时加载标签基础缓存，写入历史表时补充 `fault_type`；未匹配标签仍然照常入库，`fault_type` 留空，并通过 `metadata_unmapped_samples_total` 指标暴露待维护数量。
