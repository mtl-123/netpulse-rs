# 🩺 netpulse-rs

> **网络脉搏监控系统** - 高性能异步 TCP 连通性检测工具，7×24 小时守护您的基础设施

[![Rust](https://img.shields.io/badge/Rust-1.70+-orange?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/mtl-123/netpulse-rs?label=version)](https://github.com/mtl-123/netpulse-rs/releases)

---

## 📋 目录

- [✨ 核心特性](#-核心特性)
- [🏗️ 并发架构](#️-并发架构)
- [🚀 快速开始](#-快速开始)
- [⚙️ 配置指南](#️-配置指南)
- [🔧 调试与日志](#-调试与日志)
- [📊 告警效果](#-告警效果)
- [🐳 部署方案](#-部署方案)
- [🛠️ 开发指南](#️-开发指南)
- [🤝 贡献指南](#-贡献指南)
- [📜 许可证](#-许可证)

---

## ✨ 核心特性

| 特性                | 说明                                            | 价值                         |
| ------------------- | ----------------------------------------------- | ---------------------------- |
| 🔍 **三级并发检测** | 设备级 + 端口级 + IP 级并行检测                 | 21 设备×18 IP 检测仅需 ~3 秒 |
| 🔔 **企业微信告警** | Markdown 竖列格式 + 告警防抖 + 静默恢复         | 告警清晰不刷屏，运维效率提升 |
| ⚙️ **配置驱动**     | TOML 配置，添加设备只需复制 `[[device]]` 块     | 零代码变更，30 秒接入新设备  |
| 🛡️ **生产就绪**     | 优雅退出 + 连接数限流 + 结构化日志 + 零警告编译 | 7×24 小时稳定运行无忧        |
| 🐳 **部署友好**     | 单二进制文件，支持 systemd / Docker / 裸机      | 任意 Linux 环境一键部署      |
| 🔐 **安全传输**     | rustls 纯 Rust TLS 实现，无 OpenSSL 依赖        | 避免系统库兼容问题，更安全   |

---

## 🏗️ 并发架构

### 三级并行检测模型

```
┌─────────────────────────────────────────────────────────┐
│                    第 N 轮检测开始                        │
└─────────────────────────────────────────────────────────┘
                          │
          ┌───────────────┼───────────────┐
          ▼               ▼               ▼
    ┌──────────┐    ┌──────────┐    ┌──────────┐
    │ 设备 01   │    │ 设备 02   │    │ 设备 03   │  ← Level 1: 设备级并行
    └────┬─────┘    └────┬─────┘    └────┬─────┘
         │               │               │
    ┌────┴────┐     ┌────┴────┐     ┌────┴────┐
    ▼         ▼     ▼         ▼     ▼         ▼
 ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐
 │SSH:22│ │HTTP:80│ │SSH:22│ │HTTPS:443│ │RDP:3389│ │SSH:50022│  ← Level 2: 端口级并行
 └──┬───┘ └──┬───┘ └──┬───┘ └──┬───┘ └──┬───┘ └──┬───┘
    │         │         │         │         │         │
 ┌──┴──┐   ┌──┴──┐   ┌──┴──┐   ┌──┴──┐   ┌──┴──┐   ┌──┴──┐
 │192.1│   │192.1│   │192.1│   │192.1│   │192.1│   │192.1│  ← Level 3: IP 级并行
 │192.2│   │192.2│   │192.2│   │192.2│   │192.2│   │192.2│
 │ ... │   │ ... │   │ ... │   │ ... │   │ ... │   │ ... │
 └─────┘   └─────┘   └─────┘   └─────┘   └─────┘   └─────┘
          │
          ▼
    ┌─────────────────┐
    │ 信号量限流 100   │  ← 全局并发连接数控制，防止 TCP 耗尽
    └─────────────────┘
```

### 性能对比数据

| 场景                     | 串行检测 | 并发检测 | 提升倍数  |
| ------------------------ | -------- | -------- | --------- |
| 1 设备 × 1 端口 × 1 IP   | 3s       | 3s       | 1×        |
| 1 设备 × 3 端口 × 1 IP   | 9s       | 3s       | **3×**    |
| 1 设备 × 1 端口 × 18 IP  | 54s      | 3s       | **18×**   |
| 21 设备 × 3 端口 × 18 IP | ~1134s   | ~3s      | **~378×** |

> 💡 实际检测时间 ≈ `max(单端口超时, 单设备端口数) × 设备数 / 并发系数`

### 关键组件说明

```rust
// 1️⃣ 信号量限流 - 防止连接数爆炸
let semaphore = Arc::new(Semaphore::new(100));  // 最大 100 并发连接

// 2️⃣ 设备级并行 - JoinSet 管理任务
let mut tasks = tokio::task::JoinSet::new();
for device in &devices {
    tasks.spawn(check_device(device, semaphore.clone()));
}

// 3️⃣ 端口级并行 - 每个设备内多端口同时检测
for check in &device.checks {
    tasks.spawn(check_port_parallel(check, ips, semaphore.clone()));
}

// 4️⃣ IP 级并行 - 每个端口多 IP 同时探测（任一通即成功）
for ip in &ips {
    tasks.spawn(check_single_port(ip, port, semaphore.clone()));
}
```

---

## 🚀 快速开始

### 环境要求

- Rust 1.70+（推荐最新版）
- Linux / macOS / Windows（WSL2）
- 网络可达目标设备

### 编译安装

```bash
# 1. 克隆项目
git clone https://github.com/mtl-123/netpulse-rs.git
cd netpulse-rs

# 2. 编译 Release 版本（推荐生产使用）
cargo build --release

# 3. 验证二进制文件
./target/release/netpulse-rs --help  # 或直接运行

# 4. （可选）安装到系统路径
cargo install --path . --root ~/.local
# 然后可直接运行: ~/.local/bin/netpulse-rs
```

### 首次运行

```bash
# 1. 复制示例配置
cp config.example.toml config.toml

# 2. 编辑配置（至少修改 webhook）
vim config.toml

# 3. 运行监控
./target/release/netpulse-rs

# 4. 后台运行（Linux）
nohup ./target/release/netpulse-rs > monitor.log 2>&1 &

# 5. 查看实时日志
tail -f monitor.log
```

### 验证运行状态

```bash
# 查看进程
ps aux | grep netpulse-rs

# 查看日志尾部
tail -n 50 monitor.log

# 预期输出示例
╔══════════════════════════════════════════════════════════╗
║     🚀 网络连通性监控系统 v2.2.4 (高性能并发版)          ║
╚══════════════════════════════════════════════════════════╝

2026-02-26T10:30:00+08:00  INFO  📅 启动时间：2026-02-26 10:30:00
2026-02-26T10:30:00+08:00  INFO  ✓ 配置加载成功
2026-02-26T10:30:00+08:00  INFO    ├─ 设备数量：21
2026-02-26T10:30:00+08:00  INFO    ├─ 检测间隔：15s
2026-02-26T10:30:00+08:00  INFO    ├─ 连接超时：3s
2026-02-26T10:30:00+08:00  INFO    └─ 并发限制：100 连接

2026-02-26T10:30:15+08:00  INFO  ✓ 第   1 轮 | 全部正常 | 耗时：2s
```

---

## ⚙️ 配置指南

### 配置文件结构

```toml
# ════════════════════════════════════════════════════════════
# netpulse-rs 配置文件 v2.0
# 格式: TOML | 编码: UTF-8
# ════════════════════════════════════════════════════════════

# ── 全局设置 ────────────────────────────────────────────────
[settings]
# 检测轮询间隔（秒），建议 ≥15s 避免频繁请求
interval = 15

# 单次 TCP 连接超时（秒），1-30 之间
timeout = 3

# 同一设备告警冷却时间（秒），避免 5 分钟内重复告警
alert_cooldown = 300

# 企业微信机器人 Webhook（支持 ${ENV_VAR} 环境变量替换）
webhook = "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=xxx"

# 日志级别：debug | info | warn | error
log_level = "info"

# 最大并发连接数，根据服务器性能调整（默认 100）
max_concurrent_connections = 100

# ── 设备监控列表（扁平结构，添加设备只需复制 [[device]]）───

# 【示例：数据库设备】
[[device]]
id = "redis-master-01"                    # 设备唯一标识（英文）
name = "Redis 主节点"                      # 显示名称（中文）
group = "database"                        # 分组标签（用于告警聚合）
priority = "critical"                     # 优先级：critical|high|medium|low
ips = ["192.168.1.133"]                   # IP 列表，任一通即正常
os = "linux"                              # 操作系统：linux|windows|network
location = "机房/机柜/机架"                # 物理位置描述
# 检测项：port=端口号, name=显示名称(可选)
checks = [
  { port = 6379, name = "Redis服务" }
]

# 【示例：多 IP 多端口设备】
[[device]]
id = "app-server-01"
name = "应用服务器 01"
group = "application"
priority = "high"
# 多 IP：支持故障转移，任一 IP 的任一端口通即判定设备正常
ips = ["192.168.1.100", "192.168.1.101", "192.168.1.102"]
os = "linux"
location = "ESXi-Cluster-01"
checks = [
  { port = 22, name = "SSH" },
  { port = 80, name = "HTTP" },
  { port = 443, name = "HTTPS" }
]

# ── 快速添加模板（复制下方块，修改字段即可）──────────────────
# [[device]]
# id = "<唯一英文标识>"
# name = "<中文显示名>"
# group = "<分组: database/physical/virtual/network/storage/application>"
# priority = "<优先级: critical/high/medium/low>"
# ips = ["<IP1>", "<IP2>"]  # 支持多 IP，任一通即正常
# os = "<系统: linux/windows/network>"
# location = "<位置描述>"
# checks = [
#   { port = <端口号>, name = "<服务名称>" }
# ]
```

### 配置项详解

| 字段            | 类型    | 必填 | 说明                          | 示例                            |
| --------------- | ------- | ---- | ----------------------------- | ------------------------------- |
| `id`            | string  | ✅   | 设备唯一标识，用于状态追踪    | `redis-master-01`               |
| `name`          | string  | ✅   | 告警中显示的中文名称          | `Redis 主节点`                  |
| `group`         | string  | ✅   | 设备分组，相同组告警聚合      | `database`                      |
| `priority`      | string  | ✅   | 告警优先级，影响 emoji 图标   | `critical`                      |
| `ips`           | array   | ✅   | IP 地址列表，**任一通即正常** | `["1.1.1.1", "2.2.2.2"]`        |
| `os`            | string  | ✅   | 操作系统类型                  | `linux` / `windows` / `network` |
| `location`      | string  | ✅   | 物理位置描述                  | `机房/A区/01机柜`               |
| `checks[].port` | integer | ✅   | 检测的 TCP 端口号             | `22`, `80`, `443`               |
| `checks[].name` | string  | ❌   | 端口服务名称（可选）          | `SSH`, `HTTP`                   |

### 环境变量覆盖敏感配置

```bash
# 避免 webhook 明文写在配置文件中
export WEBHOOK_URL="https://qyapi.weixin.qq.com/...?key=xxx"

# 运行时会自动替换 config.toml 中的 ${WEBHOOK_URL}
./target/release/netpulse-rs

# 也可覆盖日志级别
export LOG_LEVEL=debug
./target/release/netpulse-rs
```

---

## 🔧 调试与日志

### 日志级别控制

```bash
# 方法 1：配置文件设置
[settings]
log_level = "debug"  # debug | info | warn | error

# 方法 2：环境变量覆盖（优先级更高）
LOG_LEVEL=debug ./target/release/netpulse-rs

# 方法 3：编译时指定（不推荐）
# 需修改 init_logging() 函数默认值
```

### 调试输出示例

```bash
# 启用 debug 日志运行
LOG_LEVEL=debug ./target/release/netpulse-rs 2>&1 | grep -E "(连接|耗时|告警)"

# 预期输出
2026-02-26T10:30:15+08:00  DEBUG  ✗ 连接失败 192.168.1.100:22 - Connection refused
2026-02-26T10:30:15+08:00  DEBUG  ✓ 连接成功 192.168.1.101:22
2026-02-26T10:30:15+08:00  INFO   ✓ 第   1 轮 | 全部正常 | 耗时：2s
```

### 常见调试场景

#### 🔍 场景 1：检测时间过长

```bash
# 1. 检查并发配置
grep max_concurrent config.toml  # 建议 ≥100

# 2. 检查超时设置
grep timeout config.toml  # 建议 3-5s

# 3. 启用 debug 查看各阶段耗时
LOG_LEVEL=debug ./netpulse-rs 2>&1 | grep "耗时"

# 4. 优化建议：
#    - 增加 max_concurrent_connections
#    - 减少单设备 IP 数量（拆分设备）
#    - 降低 timeout（网络良好时设为 2s）
```

#### 🔍 场景 2：告警不发送

```bash
# 1. 测试 webhook 连通性
curl -I "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=xxx"

# 2. 检查配置加载
./target/release/netpulse-rs 2>&1 | grep "webhook"

# 3. 验证 URL 格式（自动 trim 后）
#    确保 config.toml 中 webhook 无首尾空格

# 4. 启用 debug 查看发送详情
LOG_LEVEL=debug ./netpulse-rs 2>&1 | grep -A5 "告警"
```

#### 🔍 场景 3：编译失败

```bash
# 清理缓存并重新编译
cargo clean && cargo update && cargo build --release

# 检查 Rust 版本
rustc --version  # 要求 ≥1.70

# 检查依赖冲突
cargo tree | grep reqwest  # 确认 rustls-tls 已启用
```

### 日志格式说明

```
<时间戳+ 时区>  <级别>  <消息>

示例：
2026-02-26T10:30:00+08:00  INFO  ✓ 第   1 轮 | 全部正常 | 耗时：2s

字段说明：
- 时间：RFC3339 格式，含本地时区
- 级别：INFO/WARN/ERROR/DEBUG
- 消息：结构化业务日志，支持 emoji 视觉引导
```

---

## 📊 告警效果

### 企业微信 Markdown 渲染效果

```
🔴 **Redis 主节点** 故障告警

> 📍 位置：机房/机柜/机架
> 💻 系统：linux | 🏷️ 分组：database
> ⚠️ 优先级：critical

**故障详情**：
```

┌─ 🔴 Redis 服务 (端口：6379)
│ └─ ❌ 192.168.1.133
│
└─ 📊 统计：1 项检查失败 | 1 个 IP 受影响

```
---
<font color="warning">建议：请检查设备电源/网络/服务状态</font>
```

### 多 IP 多端口故障示例

```
🟠 **应用服务器 01** 故障告警

> 📍 位置：ESXi-Cluster-01
> 💻 系统：linux | 🏷️ 分组：application
> ⚠️ 优先级：high

**故障详情**：
```

┌─ 🔴 SSH (端口：22)
│ ├─ ❌ 192.168.1.100
│ ├─ ❌ 192.168.1.101
│ └─ ❌ 192.168.1.102
│
├─ 🔴 HTTP (端口：80)
│ ├─ ❌ 192.168.1.100
│ ├─ ❌ 192.168.1.101
│ └─ ❌ 192.168.1.102
│
└─ 📊 统计：2 项检查失败 | 6 个 IP 受影响

```
---
<font color="warning">建议：请检查设备电源/网络/服务状态</font>
```

### 告警策略说明

| 策略              | 说明                                    | 配置项                 |
| ----------------- | --------------------------------------- | ---------------------- |
| 🔔 **首次告警**   | 设备故障时立即发送                      | -                      |
| ⏸ **冷却防抖**    | 同一设备 5 分钟内不重复告警             | `alert_cooldown = 300` |
| 🔕 **静默恢复**   | 设备恢复时不发送通知，仅记录日志        | 内置逻辑               |
| 📦 **聚合发送**   | 同组设备故障聚合为一条告警              | `group` 字段           |
| 🎨 **优先级标识** | critical=🔴, high=🟠, medium=🟡, low=🔵 | `priority` 字段        |

---

## 🐳 部署方案

### 方案 A：systemd 服务（推荐 Linux 生产环境）

```ini
# /etc/systemd/system/netpulse-rs.service
[Unit]
Description=NetPulse Network Monitor
After=network.target
Wants=network.target

[Service]
Type=simple
User=monitor
Group=monitor
WorkingDirectory=/opt/netpulse-rs
ExecStart=/opt/netpulse-rs/target/release/netpulse-rs
Restart=on-failure
RestartSec=10
StandardOutput=journal
StandardError=journal
SyslogIdentifier=netpulse-rs

# 环境变量
Environment=LOG_LEVEL=info
Environment=WEBHOOK_URL=${WEBHOOK_URL}

# 资源限制（可选）
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
```

```bash
# 部署步骤
sudo useradd -r -s /bin/false monitor
sudo mkdir -p /opt/netpulse-rs
sudo cp target/release/netpulse-rs /opt/netpulse-rs/
sudo cp config.toml /opt/netpulse-rs/
sudo cp netpulse-rs.service /etc/systemd/system/

sudo systemctl daemon-reload
sudo systemctl enable --now netpulse-rs
sudo systemctl status netpulse-rs

# 查看日志
journalctl -u netpulse-rs -f
```

### 方案 B：Docker 容器化部署

```dockerfile
# Dockerfile
FROM rust:1.75-slim-bullseye as builder
WORKDIR /app
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl

FROM alpine:3.19
RUN apk --no-cache add ca-certificates
WORKDIR /app
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/netpulse-rs .
COPY config.example.toml config.toml

EXPOSE 8080
CMD ["./netpulse-rs"]
```

```bash
# 构建并运行
docker build -t netpulse-rs .
docker run -d \
  --name netpulse \
  -v $(pwd)/config.toml:/app/config.toml:ro \
  -e WEBHOOK_URL=${WEBHOOK_URL} \
  -e LOG_LEVEL=info \
  --restart=unless-stopped \
  netpulse-rs

# 查看日志
docker logs -f netpulse
```

### 方案 C：Supervisor 进程守护（通用方案）

```ini
# /etc/supervisor/conf.d/netpulse-rs.conf
[program:netpulse-rs]
command=/opt/netpulse-rs/target/release/netpulse-rs
directory=/opt/netpulse-rs
user=monitor
autostart=true
autorestart=true
redirect_stderr=true
stdout_logfile=/var/log/netpulse-rs/out.log
stderr_logfile=/var/log/netpulse-rs/err.log
environment=LOG_LEVEL="info",WEBHOOK_URL="%(ENV_WEBHOOK_URL)s"
```

```bash
# 安装并启动
sudo apt install supervisor  # Debian/Ubuntu
sudo supervisorctl reread
sudo supervisorctl update
sudo supervisorctl start netpulse-rs
```

### 部署检查清单

```bash
# ✅ 编译检查
cargo build --release 2>&1 | grep -E "(error|warning)" || echo "✓ 编译通过"

# ✅ 配置检查
./target/release/netpulse-rs 2>&1 | head -10 | grep "配置加载成功"

# ✅ 网络检查
curl -I https://qyapi.weixin.qq.com  # 确保能访问企业微信 API

# ✅ 权限检查
ls -la target/release/netpulse-rs  # 确保有执行权限

# ✅ 后台运行检查
ps aux | grep netpulse-rs | grep -v grep  # 确认进程存活
```

---

## 🛠️ 开发指南

### 项目结构

```
netpulse-rs/
├── Cargo.toml          # 依赖声明 + 元数据
├── Cargo.lock          # 依赖锁定（提交到 Git）
├── src/
│   └── main.rs         # 主程序入口（单文件架构）
├── config.example.toml # 配置模板
├── config.toml         # 实际配置（.gitignore）
├── README.md           # 项目文档
├── LICENSE             # MIT 许可证
├── .gitignore          # Git 忽略规则
└── systemd/            # （可选）部署文件
    └── netpulse-rs.service
```

### 开发工作流

```bash
# 1. 克隆并进入项目
git clone https://github.com/mtl-123/netpulse-rs.git
cd netpulse-rs

# 2. 开发模式运行（支持热重载）
cargo run

# 3. 代码检查（提交前必做）
cargo fmt --check          # 格式检查
cargo clippy -- -D warnings  #  lint 检查

# 4. 运行测试（预留测试框架）
cargo test

# 5. 构建 Release 版本
cargo build --release

# 6. 性能分析（可选）
cargo build --release --features=profiling
```

### 添加新检测类型（扩展指南）

当前仅支持 `tcp_port` 检测，如需扩展：

```rust
// 1. 在 CheckItem 中添加类型字段
#[derive(Debug, Deserialize, Clone)]
struct CheckItem {
    #[serde(rename = "type", default = "default_check_type")]
    check_type: String,  // "tcp_port" | "http_get" | "ping" ...
    port: u16,
    #[serde(default)]
    name: String,
    // 扩展字段...
}

fn default_check_type() -> String { "tcp_port".to_string() }

// 2. 在 check_device_parallel 中添加分支
match check.check_type.as_str() {
    "tcp_port" => check_port_parallel(...).await,
    "http_get" => check_http_get(...).await,  // 新增实现
    "ping" => check_icmp(...).await,          // 新增实现
    _ => warn!("未知检测类型：{}", check.check_type),
}

// 3. 更新配置示例和文档
```

### 性能优化建议

```rust
// 🔹 减少克隆：使用 &T 和 Cow 替代 .clone()
// 🔹 复用 Client：reqwest::Client 应全局复用，避免每请求创建
// 🔹 批处理日志：高频 debug 日志可聚合后输出
// 🔹 异步边界：避免在 async 函数中调用阻塞操作

// 示例：复用 HTTP Client
lazy_static! {
    static ref HTTP_CLIENT: reqwest::Client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
}
```

---

## 🤝 贡献指南

欢迎提交 Issue / PR 改进本项目！

### 提交 Issue

- 🔍 先搜索是否已有类似问题
- 📋 提供：Rust 版本、OS、配置片段、错误日志
- 🎯 明确期望行为 vs 实际行为

### 提交 PR

```bash
# 1. Fork 项目并克隆
git clone https://github.com/YOUR_USERNAME/netpulse-rs.git

# 2. 创建功能分支
git checkout -b feat/your-feature-name

# 3. 开发并测试
cargo fmt
cargo clippy -- -D warnings
cargo test

# 4. 提交规范
git commit -m "feat: 添加 HTTP 检测支持"
# 类型：feat|fix|docs|style|refactor|test|chore

# 5. 推送并创建 PR
git push origin feat/your-feature-name
# 在 GitHub 创建 Pull Request，描述变更内容
```

### 代码规范

- ✅ 使用 `cargo fmt` 格式化代码
- ✅ 通过 `cargo clippy -- -D warnings` 检查
- ✅ 新增功能需更新 README 文档
- ✅ 公共函数添加 Rustdoc 注释

---

## 📜 许可证

本项目采用 **MIT License** - 详见 [LICENSE](LICENSE) 文件

```
MIT License

Copyright (c) 2026 梅桃林

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

---

## 🙏 致谢

- [Tokio](https://tokio.rs) - 高性能异步运行时
- [reqwest](https://docs.rs/reqwest) - 简洁的 HTTP 客户端
- [tracing](https://docs.rs/tracing) - 结构化日志框架
- [企业微信机器人](https://work.weixin.qq.com/api/doc/90000/90136/91770) - 告警通知通道

---

> 💡 **提示**：如遇问题，请先查阅本文档 + 启用 `LOG_LEVEL=debug`。  
> 仍无法解决？[提交 Issue](https://github.com/mtl-123/netpulse-rs/issues/new) 获取帮助 🚀
