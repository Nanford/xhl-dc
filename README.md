# Kepware Bridge

Rust 1.0 版本工业采集服务：从 Kepware/KEPServerEX OPC UA 订阅 tag 数据，批量写入 MySQL，写库失败时落到本地 sled 缓冲，后续自动回灌。

## 本机准备

- Windows + PowerShell
- Rust 1.75+，本仓库已按 Edition 2021 编写
- Kepware OPC UA Server，本机常见端口 `49320`
- MySQL，建议 8.0+，生产优先 MySQL 8.4 LTS
- 可选：`sqlx-cli`
- 可选：Python + `asyncua` + `PyYAML`，用于半自动发现 Kepware 点位并生成 tag 配置

## 初始化 MySQL

先创建数据库，再执行迁移：

```powershell
mysql -u root -p -e "CREATE DATABASE IF NOT EXISTS iot CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci;"
mysql -u root -p iot < .\migrations\202605060001_create_tag_log.sql
mysql -u root -p iot < .\migrations\202605080001_add_source_column.sql
```

如果你已安装 `sqlx-cli`，也可以执行：

```powershell
sqlx migrate run --database-url "mysql://user:pass@127.0.0.1:3306/iot"
```

## 配置

默认读取 `.\config.yaml`。运行前至少修改：

- `opcua.endpoint`
- `mysql.url`
- `subscriptions[].tags[].node_id`
- `subscriptions[].tags[].alias`

本机私有配置建议写到 `config.local.yaml`，该文件已被 `.gitignore` 忽略，适合放真实 MySQL 密码和现场点位。

Kepware 字符串点位通常是：

```yaml
node_id: "ns=2;s=Channel1.Device1.Temperature"
```

测试期可以用：

```yaml
security_policy: None
identity: anonymous
```

生产应切到：

```yaml
security_policy: Basic256Sha256
identity:
  username: "kepware_user"
  password: "change_me"
```

可以先用 discovery 工具查看 Kepware 实际开放的 endpoint：

```powershell
cargo run --bin opcua_endpoints -- opc.tcp://127.0.0.1:49320
```

## 半自动发现点位

主服务启动时仍然只订阅 `subscriptions[].tags`，不会直接把 browse 到的所有点位自动订阅。`opcua.discovery` 是给辅助脚本使用的发现边界，用来先生成或替换 tag 清单，再由人检查配置后运行服务。

推荐先在 `config.local.yaml` 里配置发现范围：

```yaml
opcua:
  discovery:
    enabled: true
    target_subscription: "fast"
    root_node_ids:
      - "ns=2;s=Channel1"
    include_paths:
      - "Channel1.Device1"
    exclude_paths:
      - "Channel1.Device1._System"
      - "Channel1.Device1._Statistics"
    min_namespace_index: 2
    max_depth_count: 6
    max_tags_count: 500
    include_system: false
    include_arrays: false
```

先 dry-run 查看发现结果，不写文件：

```powershell
python .\scripts\opcua_browse_tags.py --config .\config.local.yaml
```

确认结果后再写回 `target_subscription` 指定的订阅：

```powershell
python .\scripts\opcua_browse_tags.py --config .\config.local.yaml --write
```

如果现场只想临时覆盖某个边界，可以追加命令行参数，例如：

```powershell
python .\scripts\opcua_browse_tags.py --config .\config.local.yaml --root-node-id "ns=2;s=Channel2" --include-path "Channel2.DeviceA" --limit 200
```

## WCS HTTP 采集

支持通过 HTTP API 轮询 WCS 数据（如设备故障状态），与 OPC UA 采集共用同一个入库管道。

在 `config.yaml` 或 `config.local.yaml` 中增加 `wcs` 段：

```yaml
wcs:
  base_url: "http://192.168.1.100:8080/api/v1"
  poll_interval_ms: 5000
  headers:
    Authorization: "Bearer <token>"
  timeout_ms: 10000
  retry_interval_ms: 5000
  endpoints:
    - path: "/conveyor/status"
      method: GET
      tags:
        - { json_path: "running", alias: "conveyor_running" }
        - { json_path: "speed", alias: "conveyor_speed", value_type: float }
        - { json_path: "error_code", alias: "conveyor_error", value_type: int }
```

- `json_path` 支持点分路径，如 `data.readings.temp`
- `value_type` 可选 `bool`（默认）、`int`、`float`、`text`
- `opcua` 和 `wcs` 可以同时配置，也可以只配其中一个
- 数据来源通过 `tag_log.source` 列区分（`opcua` 或 `wcs`）

升级数据库：

```powershell
mysql -u root -p iot < .\migrations\202605080001_add_source_column.sql
```

## 运行

```powershell
$env:RUST_LOG = "info,sqlx=warn,opcua=warn"
cargo run -- --config .\config.yaml
```

编译 release：

```powershell
cargo build --release
.\target\release\kepware-bridge.exe --config .\config.yaml
```

查看 metrics：

```powershell
curl http://127.0.0.1:9090/metrics
```

## Kepware 检查项

- Project Properties -> OPC UA 中确认允许匿名登录，或者配置用户名密码。
- 客户端会话最大数不能是 `0`。
- Windows 防火墙允许 OPC UA 端口，常见为 `49320`。
- 修改 Kepware 配置后保存项目。
- 生产长连接使用有线网络。

## 验证命令

```powershell
cargo fmt --all
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```

## Linux 部署提示

Linux 上使用同一份 release binary 或重新编译后运行。建议通过 systemd 托管，`buffer.path` 放到持久化目录，例如 `/var/lib/kepware-bridge/wal`。容器化时必须挂载该目录，否则写库失败时的缓冲数据会在容器重建后丢失。
