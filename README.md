# Kepware Bridge

工业数据采集服务，从 Kepware/KEPServerEX OPC UA 订阅点位变化，批量写入 MySQL。写库失败时先落到本地 sled 缓冲，后续自动回灌，避免数据库短暂不可用导致数据直接丢失。

## 当前写库目标

报警数据写入三张结构一致的表，按点位第一个字段路由：

- `cpk_alarm_log`
- `flk_alarm_log`
- `ylk_alarm_log`

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
- `tag_value`: 采样值字符串，如 `true`、`false`、`6`
- `tag_state`: 非零/`true` 写 `active`，零/`false` 写 `inactive`
- `description`: 优先写入 Kepware 标签说明；读取不到时留空，或由映射文件补充

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
- `tag_value`: 采样值字符串
- `tag_state`: 非零/`true` 写 `active`，零/`false` 写 `inactive`
- `description`: 优先写入 Kepware 标签说明；读取不到时留空，或由映射文件补充

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

生产库已存在三张报警表。仓库中的 `migrations/202605120001_create_alarm_log_tables.sql` 提供同结构建表脚本，用于新环境初始化或本地验证。

```powershell
mysql -u root -p iot < .\migrations\202605120001_create_alarm_log_tables.sql
```

## 运行配置

默认读取 `config.yaml`。现场真实连接、账号密码和点位清单建议放在未提交的 `config.local.yaml`。

关键配置：

- `opcua.endpoint`: Kepware OPC UA 地址
- `opcua.description_map_path`: 可选描述映射文件；用于覆盖或补充 Kepware 说明
- `mysql.url`: MySQL 连接串
- `subscriptions[].tags[].node_id`: OPC UA NodeId
- `subscriptions[].tags[].alias`: 点位别名；建议保留完整点位路径，便于非 OPC UA 来源复用同一套解析规则
- `sink.tag_prefix_routes`: 区域编码到报警表的路由表

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

报警表中的 `location`、`device`、`device_id`、`tag`、`tag_state`、`tag_value` 使用 `VARCHAR`，`description` 使用 `TEXT`。这组类型可以存储中文报警描述，不需要改成 JSON 或二进制字段。

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
