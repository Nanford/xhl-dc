# AGENTS.md

本文件是 Codex 在本仓库工作的项目级指令。`CLAUDE.md` 是业务和架构约束的详细来源；本文件把这些约束整理成更适合 Codex 执行、修改、测试和交付时使用的操作规则。

## 指令优先级

1. 用户当前对话中的明确要求优先。
2. 本文件约束当前目录及其子目录中的所有工作。
3. `CLAUDE.md` 是项目目标、架构、技术栈和禁止事项的事实来源。
4. 如果本文件与 `CLAUDE.md` 冲突，不要自行猜测；先指出冲突并让用户确认。
5. 对于编程过程，关键的代码需要适当增加注释
6. 程序需要有log等级管理，测试版可以执行debug的方式，然后生产发布的就是 error或者waring。这样不会输出大量的log

## 项目定位

这是一个 Rust 编写的工业数据采集长跑服务：

```text
Kepware / KEPServerEX OPC UA Server
  -> async-opcua Subscription + MonitoredItem
  -> 同步 DataChangeCallback
  -> tokio::mpsc
  -> Sink Worker 批量写入
  -> MySQL
  -> INSERT 失败时写入 sled 本地缓冲并回灌
```

目标是稳定运行数月、重启不丢数据、配置变更不重新编译。不要把它当一次性脚本、临时采集工具或轻量 demo 处理。

## 当前技术适用性评估

复核日期：2026-05-06。

- `Rust 1.75+` 与 Edition 2021 仍然适合本项目。Rust 2024 Edition 已经可用，但本项目以稳定生产和跨平台部署为主，除非专门做迁移任务，不主动切到 Edition 2024。
- `async-opcua 0.18` 适合当前 OPC UA 客户端场景，优先级高于旧的 `locka99/opcua` 和需要 C 工具链的 `open62541-rs`。如需升级 async-opcua，必须先检查 DataChangeCallback、Subscription、Session API 是否破坏现有数据流。
- `tokio 1.x`、`sqlx 0.8`、`serde_yaml`、`tracing`、`metrics` 这组依赖对 Windows 开发和 Linux 部署都可行，不要无理由替换。
- `sled 0.34` 可以作为本地失败缓冲使用，但要把它当作单进程嵌入式持久化队列，不要多进程共享目录。缓冲目录必须在持久化磁盘上。若将来因为维护状态或数据一致性风险要替换为 SQLite、redb 或专用 WAL，需要先写设计说明并获得确认。
- MySQL 建议生产优先使用 MySQL 8.4 LTS 系列。若现场已安装 MySQL 8.0 或其他版本，先按现有环境开发测试，不要为了追新强制升级数据库。不要默认使用 MySQL 9.x Innovation 版本特性。
- Kepware 通常运行在 Windows 现场机上；本服务可以先在 Windows 开发测试，未来部署到 Linux 时通过网络访问 Kepware OPC UA 端点。不要假设 Kepware 也会迁到 Linux。

## 已锁定技术栈

未经用户确认，不要替换以下核心依赖：

- Rust 1.75+，Edition 2021
- async-opcua 0.18
- tokio 1.x，full features
- sqlx 0.8，MySQL，runtime-tokio
- serde + serde_yaml
- tracing + tracing-subscriber
- sled 0.34
- metrics + metrics-exporter-prometheus
- anyhow + thiserror

新增依赖前必须说明：

- 现有依赖为什么不够用。
- 新依赖是否支持 Windows 开发和 Linux 部署。
- 新依赖是否引入 C/C++ 工具链、OpenSSL、系统动态库或交叉编译负担。
- 新依赖失败时对不丢数据目标的影响。

## 架构硬约束

- 数据流必须保持单向：Kepware -> 回调 -> mpsc -> sink -> MySQL / sled。
- OPC UA `DataChangeCallback` 必须保持同步闭包。
- 回调里只允许做轻量转换和 `mpsc::try_send`。
- 回调里禁止 `await`、数据库写入、文件 IO、复杂计算、阻塞等待、`blocking_send`。
- `try_send` 失败时记录 `warn`，增加 `dropped_samples_total` 指标，不要阻塞 OPC UA 协议层。
- `Sink Worker` 负责批量攒批、定时 flush、关闭时排空、失败后写 sled 缓冲。
- sled 缓冲必须支持服务重启后的回灌。没有失败缓冲时，不要做会丢数据的快速修复。

## 模块边界

优先沿用以下模块划分：

```text
src/
  main.rs           启动入口、信号处理、关闭流程
  config.rs         配置加载与校验
  opcua_client.rs   Session 管理、订阅创建、自动重连
  sink.rs           批量入库 worker
  buffer.rs         sled 失败缓冲与回灌
  metrics.rs        Prometheus 指标暴露
  types.rs          通用类型，如 TagSample、ValueKind
```

新增模块前先判断现有模块是否真的不够用。不要因为文件名直觉直接改动；先读取相关模块和调用方。涉及多个模块的改动，先列出改动清单。涉及数据流方向、缓冲语义、数据库 schema、部署方式或依赖替换的改动，必须先让用户确认。

## 配置规则

- `config.yaml` 是唯一运行时配置入口。
- 不允许硬编码 OPC UA endpoint、NodeId、数据库连接、用户名密码、批量阈值、缓冲路径、metrics 端口。
- 配置项使用 `snake_case`。
- 有单位的字段必须把单位写进字段名，例如 `_ms`、`_mb`、`_count`。
- 新增配置项时同步更新 `config.yaml` 示例和 `README`，如果 README 尚不存在，应在交付说明中指出。
- 密码和现场连接信息只能走配置文件、环境变量或本机未提交的私有配置，不要写入测试用例或示例真实值。

## 数据库规则

- 数据表保持窄表设计，按月分表的方案不应被无理由替换。
- 新 schema 变更走 `sqlx-cli` migration。
- OPC UA Variant 必须按 BuiltInType 转换：
  - `bool`、整数、浮点数写入 `value_num`，并设置 `value_type`。
  - `String`、`ByteString` 写入 `value_str`，并设置 `value_type`。
  - 不要把所有值 `to_string()` 塞进单列。
  - 不要为了灵活性改成单个 JSON 字段。
- `source_ts`、`server_ts`、`ingest_ts` 保留毫秒精度。
- 索引优先服务按 `node_id/source_ts` 和 `alias/source_ts` 查询。

## Windows 开发测试约定

当前默认开发系统是 Windows，使用 PowerShell。

常用命令：

```powershell
cargo build
cargo build --release
cargo test
cargo fmt --all
cargo clippy --all-targets -- -D warnings
```

运行示例：

```powershell
$env:RUST_LOG = "info,sqlx=warn,opcua=warn"
.\target\debug\kepware-bridge.exe --config .\config.yaml
```

数据库迁移示例：

```powershell
sqlx migrate run --database-url "mysql://user:pass@127.0.0.1:3306/iot"
```

Windows 注意事项：

- Kepware OPC UA 默认端口常见为 `49320`，本机或现场机 Windows 防火墙需要允许入站。
- 如果 MySQL 装在本机，优先用 `127.0.0.1` 明确 TCP 连接，避免 named pipe 等平台差异。
- `buffer.path` 可以在开发期使用 `.\data\wal`，但测试重启恢复时不要放到临时目录。
- 集成测试若使用 `testcontainers`，先确认 Docker Desktop 正常运行；没有 Docker 时不要伪造集成测试通过。
- PowerShell 下环境变量写法与 Linux 不同，文档和脚本要分别给出。

## Kepware 现场约束

- 测试期可以使用 `SecurityPolicy=None` 和匿名登录，但生产必须升级到 `Basic256Sha256 + Sign and Encrypt + 用户名密码`。
- Kepware Project Properties -> OPC UA 中需要确认允许匿名登录、客户端会话最大数不是 0、诊断按需开启。
- Kepware 修改配置后必须保存项目，不只是点击应用。
- NodeId 通常是字符串型，格式类似 `ns=2;s=Channel.Device.Tag`。不要把 Kepware 字符串点位误写成数字 NodeId。
- 生产订阅长连接应走有线网络，不要默认 WiFi 稳定。
- 服务端会话超时需要通过客户端 keepalive、ping 或订阅 ServerStatus 作为 watchdog 处理。

## Linux 部署预留

写代码时保持 Windows 可开发、Linux 可部署：

- 不要硬编码 `\` 路径分隔符，使用 `Path` / `PathBuf`。
- 不要依赖 Windows 服务、注册表、盘符、PowerShell 专属行为进入核心业务逻辑。
- Linux 部署建议使用 systemd，监听 SIGTERM 后优雅关闭。
- `buffer.path` 在 Linux 生产建议放到 `/var/lib/kepware-bridge/wal` 或挂载卷。
- 日志输出到 stdout/stderr，交给 systemd、容器平台或日志采集器处理。
- metrics 绑定地址保持可配置，生产默认不要暴露到不可信网络。
- 容器化部署必须给 sled 缓冲目录挂持久化卷。

## 并发、错误与日志

- 并发只使用 tokio 原语：`mpsc`、`oneshot`、`watch`、`broadcast`。
- 不要用 `Arc<Mutex<Vec<T>>>` 自制队列。
- 不要用 `std::sync::Mutex` 锁住跨 `await` 点的状态。
- `sqlx::Pool` 已经线程安全，不要额外包锁。
- 业务路径使用 `anyhow::Result`，库内部接口用 `thiserror` 定义明确错误。
- 生产代码禁止 `unwrap()` / `expect()`；测试代码可按测试可读性使用。
- 日志统一使用 `tracing`，不要留下 `println!`、`dbg!`。
- 必须以 `info` 记录 OPC UA 连接状态、Session 重建、Subscription 创建、批量入库结果、失败缓冲触发、缓冲回灌结果。
- 不要为每条数据变更打日志。

## 关闭流程

`main` 应同时处理 Windows `ctrl_c` 和 Linux SIGTERM。收到关闭信号后：

1. 通过 `watch` 通知所有 worker。
2. 等待 mpsc 排空。
3. flush 未入库批次。
4. flush sled。
5. 停止 metrics server。
6. 超过 30 秒仍无法退出时 abort，并记录 `error`。

## 测试要求

至少覆盖：

- `config.yaml` 解析与校验。
- OPC UA Variant -> `ValueKind` 类型转换。
- sink batch flush 边界：满批触发、超时触发、关闭时排空。
- MySQL 写入失败后的 sled 缓冲。
- 服务重启后的缓冲回灌。

集成测试优先：

- 用 `testcontainers` 启动 MySQL。
- 用 `async-opcua-server` 或等价 mock OPC UA server 模拟数据变化。
- 不能连接真实 Kepware 或真实生产 MySQL 作为自动化测试前提。

## Codex 工作方式

- 开始代码修改前先读 `CLAUDE.md`、本文件、相关源文件和测试。
- 优先使用 `rg` 查找文本和文件。
- 手工编辑文件使用 `apply_patch`。
- 不要改动与任务无关的文件。
- 如果发现用户已有修改，不要回滚；必须在现有状态上工作。
- 不要使用破坏性 git 命令。
- 修改代码后按风险运行验证命令；没有运行过的验证不能声称通过。
- 文档类修改至少重新读取生成的文件，确认 UTF-8、标题和关键约束完整。
- 对依赖版本、数据库版本、Kepware 行为、OpenAI/Codex 行为等可能变化的信息，不要凭记忆下结论；优先核对官方文档或项目源。

## 禁止事项

- 禁止把 `DataChangeCallback` 改成 async 闭包。
- 禁止在回调里直连 MySQL 或调用 `sqlx`。
- 禁止用阻塞方式处理 OPC UA 回调。
- 禁止把所有采样值塞进 JSON 或字符串单列。
- 禁止绕过失败缓冲直接丢弃写库失败数据。
- 禁止 catch 所有错误后无条件 continue。
- 禁止硬编码 Kepware、MySQL、用户名、密码和现场点位。
- 禁止为了“快速可用”牺牲重启不丢数据的目标。

