# Kepware Bridge

Rust 编写的工业数据采集服务，从 Kepware（KEPServerEX）通过 OPC UA 订阅实时数据，批量落库到 MySQL。生产环境长跑型服务，不是一次性脚本。

## 项目目标

服务对接现场 Kepware OPC UA Server，订阅指定 tag 的数据变更，按批次写入 MySQL，提供基础的失败缓冲与可观测能力。目标是在工厂网络环境下稳定运行数月不掉数据，重启不丢数据，配置变更不重新编译。

## 技术栈（已锁定，不要更换）

核心依赖如下，未经讨论不要替换：

- Rust 1.75+，Edition 2021
- async-opcua 0.18（FreeOpcUa/async-opcua）
- tokio 1.x，full features
- sqlx 0.8（MySQL，runtime-tokio）
- serde + serde_yaml（配置）
- tracing + tracing-subscriber（日志）
- sled 0.34（本地失败缓冲）
- metrics + metrics-exporter-prometheus（监控指标）
- anyhow + thiserror（错误处理）

为什么不用 locka99/opcua：老仓库已停滞两年，社区主力迁到 async-opcua。
为什么不用 open62541-rs：依赖 C 工具链，跨平台编译复杂。
为什么不用 sea-orm/diesel：sqlx 编译期校验已够用，再加 ORM 是过度设计。

## 架构约定

数据流是严格单向的，任何修改都不能破坏这个方向：

```
Kepware OPC UA Server
        │  Subscription + MonitoredItem
        ▼
   DataChangeCallback (同步闭包)
        │  try_send
        ▼
   tokio::mpsc Channel
        │  select(消息 / 定时器)
        ▼
   Sink Worker (批量攒批)
        │  INSERT 失败时
        ▼
   sled 本地缓冲 ──回灌──► MySQL
```

回调闭包里只做一件事：把数据塞进 mpsc 通道。绝不在回调里写库、做 IO、做复杂计算、做 await。async-opcua 的 DataChangeCallback 签名是同步闭包，强行 async 会编译失败，也会反压到协议层。

## 模块划分

```
src/
├── main.rs           启动入口、信号处理、关闭流程
├── config.rs         配置加载与校验
├── opcua_client.rs   Session 管理、订阅创建、自动重连
├── sink.rs           批量入库 worker
├── buffer.rs         sled 失败缓冲与回灌
├── metrics.rs        Prometheus 指标暴露
└── types.rs          通用类型(TagSample, ValueKind 等)
```

新增模块前要先问清楚为什么现有模块不够用，避免越加越散。

## 配置文件

config.yaml 是唯一配置入口，所有运行时参数从这里读，不允许硬编码端点、点位、库连接、批量阈值等任何业务参数。

```yaml
opcua:
  endpoint: "opc.tcp://192.168.43.5:49320"
  security_policy: None        # None | Basic256Sha256
  identity: anonymous          # anonymous | { username: ..., password: ... }
  session_retry_limit: -1      # -1 表示永久重试
  application_uri: "urn:KepwareBridge"

mysql:
  url: "mysql://user:pass@127.0.0.1:3306/iot"
  max_connections: 8

subscriptions:
  - name: "fast"
    publishing_interval_ms: 500
    keep_alive_count: 10
    lifetime_count: 30
    tags:
      - { node_id: "ns=2;s=Channel1.Device1.Temperature", alias: "temp_1" }
  - name: "slow"
    publishing_interval_ms: 5000
    tags:
      - { node_id: "ns=2;s=Channel1.Device1.Pressure", alias: "press_1" }

sink:
  table: "tag_log"
  batch_size: 500
  flush_interval_ms: 1000

buffer:
  path: "./data/wal"
  max_size_mb: 1024

metrics:
  bind: "0.0.0.0:9090"
```

按"采样频率"分组创建多个 Subscription，避免高频点拖低频点。lifetime_count 必须 ≥ 3 × keep_alive_count，否则服务端会拒绝订阅。

## 数据库 Schema

窄表 + 复合索引，按月分表。新建迁移走 sqlx-cli。

```sql
CREATE TABLE tag_log (
  id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
  node_id VARCHAR(255) NOT NULL,
  alias VARCHAR(64) NOT NULL,
  value_type TINYINT NOT NULL,         -- 0=bool 1=int 2=float 3=string
  value_num DOUBLE NULL,
  value_str VARCHAR(512) NULL,
  source_ts DATETIME(3) NOT NULL,
  server_ts DATETIME(3) NOT NULL,
  quality INT NOT NULL,
  ingest_ts DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
  INDEX idx_node_ts (node_id, source_ts DESC),
  INDEX idx_alias_ts (alias, source_ts DESC)
) ENGINE=InnoDB ROW_FORMAT=DYNAMIC;
```

OPC UA Variant 类型必须按 BuiltInType 分支处理，bool/i32/i64/f32/f64 走 value_num，String/ByteString 走 value_str，不要全部 to_string 塞进同一列，否则后续查询和聚合都做不动。

## 开发规范

错误处理用 anyhow::Result 走业务路径，库内部接口用 thiserror 自定义错误类型。生产代码禁止 unwrap()/expect()，测试代码不限。

日志强制走 tracing。OPC UA 连接状态变化、Session 重建、Subscription 创建、批量入库结果、失败缓冲触发、缓冲回灌结果，全部必须 info 级别记录。每条数据变更不要打 debug 日志，量级会爆。

并发只用 tokio 原语：mpsc / oneshot / watch / broadcast。不要用 Arc<Mutex<Vec<T>>> 拼凑队列，不要用 std::sync::Mutex 锁住跨 await 点的状态。sqlx::Pool 内部已线程安全，不要再加锁。

资源关闭：main 监听 ctrl_c 与 SIGTERM，收到后向所有 worker 发 watch 关闭信号，等 mpsc 排空、sled flush、metrics 服务器停止后再退出。退出超过 30 秒强制 abort 并打 error 日志。

测试覆盖三块：config 解析与校验、Variant → ValueKind 类型转换、sink 的 batch flush 边界（满批触发、超时触发、关闭时排空）。集成测试用 testcontainers 起 MySQL，用 async-opcua-server 跑一个 mock 服务端。

## Kepware 端必须确认的设置

Project Properties → OPC UA 里的"允许匿名登录"必须打开（早期版本默认关闭）。"客户端会话最大数"不能是 0。诊断建议开启便于排查。

端点定义里测试期可以只勾"无（不安全）"，生产必须升级到 Basic256Sha256 + Sign and Encrypt + 用户名密码。

Windows 防火墙 49320 入站放行。Kepware 改完配置必须点保存项目（不只是应用），运行时才生效。

## 已知坑点

Kepware 的 NodeId 是字符串型，命名空间通常 ns=2，格式 `ns=2;s=Channel.Device.Tag`，构造 NodeId 用字符串变体（NodeId::new(2, "...")），不要用数字 ID。

回调闭包里 mpsc::try_send 失败（通道满）说明 sink 消费跟不上，必须打 warn 日志并自增 metrics 计数器（dropped_samples_total），但绝对不能阻塞回调，更不能换成 blocking_send。

WiFi 网卡上跑长连接订阅，断线重连频率显著高于有线，生产部署一律走有线。

Kepware 服务端默认会话超时是 60 秒，如果应用层心跳间隔大于这个值会被踢，需要客户端侧主动 ping 或者订阅 ServerStatus 节点当 watchdog。

sled 数据库目录必须放在持久化盘上，容器化部署要挂卷，否则容器重启缓冲数据全丢。

## 常用命令

```bash
# 编译
cargo build --release

# 运行(默认读 ./config.yaml)
RUST_LOG=info,sqlx=warn,opcua=warn ./target/release/kepware-bridge

# 测试
cargo test
cargo test --test integration -- --nocapture

# 数据库迁移
sqlx migrate add <name>
sqlx migrate run --database-url mysql://...

# 格式化 + lint(提交前必跑)
cargo fmt --all
cargo clippy --all-targets -- -D warnings

# 检查未使用依赖
cargo machete

# 查看 metrics
curl http://localhost:9090/metrics
```

## Claude Code 工作约定

修改代码前先 read 相关模块，不要凭文件名直觉改动。涉及多个模块的改动先列改动清单，确认后再动手。

新增依赖前必须给出"为什么现有依赖不够用"的理由，避免依赖膨胀。

重构涉及数据流方向调整时，先在对话里画出新旧数据流对比图，确认无误再改代码。

不要主动给代码加注释，让代码自解释。只在"为什么这么写"非显而易见时加 doc comment（比如绕过某个 Kepware bug 的 workaround、特定数值阈值的来源）。

不要把 println!/dbg! 留在生产代码，全部用 tracing。提交前 grep 一遍确认。

新增配置项必须同步更新 config.yaml 示例与 README，配置项命名走 snake_case，单位写在字段名里（_ms / _mb / _count）。

## 禁止事项

不要把 OPC UA DataChangeCallback 改成 async 闭包，会编译失败，也会破坏架构假设。

不要在回调里直连数据库或调用 sqlx，必须经过 mpsc 通道。

不要为了"灵活"把所有数据塞进单个 JSON 字段，类型化字段是性能基础。

不要在没有失败缓冲机制的情况下做"快速修复"丢数据的代价比延迟修复高得多。

不要直接 catch 住所有错误然后 continue，未识别的错误必须 bubble up 到顶层日志，便于发现新问题。

不要把 Kepware 的连接信息、用户名密码硬编码进代码或测试用例，全部走配置或环境变量。
