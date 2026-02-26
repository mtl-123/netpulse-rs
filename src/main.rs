// ════════════════════════════════════════════════════════════
// netpulse-rs v2.2.5 - 网络脉搏监控系统
// 特性：异步并发检测 | 企业微信告警 | 优雅退出 | 零警告编译
// ════════════════════════════════════════════════════════════

#![warn(rust_2018_idioms)]
#![allow(dead_code)]

use chrono::Local;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, Semaphore};
use tokio::time::{sleep, timeout};
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

// ────────────────────────────────────────────────────────────
// 配置结构（与 config.toml 严格对应）
// ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
struct Config {
    settings: Settings,
    #[serde(rename = "device")]
    devices: Vec<Device>,
}

#[derive(Debug, Deserialize, Clone)]
struct Settings {
    interval: u64,
    timeout: u64,
    alert_cooldown: u64,
    webhook: String,
    #[serde(default = "default_log_level")]
    log_level: String,
    #[serde(default = "default_max_concurrent")]
    max_concurrent_connections: usize,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_max_concurrent() -> usize {
    100
}

#[derive(Debug, Deserialize, Clone)]
struct Device {
    id: String,
    name: String,
    group: String,
    priority: String,
    ips: Vec<String>,
    os: String,
    location: String,
    checks: Vec<CheckItem>,
}

#[derive(Debug, Deserialize, Clone)]
struct CheckItem {
    port: u16,
    #[serde(default)]
    name: String,
}

// ────────────────────────────────────────────────────────────
// 告警状态管理（线程安全 + 冷却控制）
// ────────────────────────────────────────────────────────────

struct AlertState {
    last_alert: HashMap<String, i64>,
    is_failed: HashMap<String, bool>,
}

impl AlertState {
    fn new() -> Self {
        Self {
            last_alert: HashMap::new(),
            is_failed: HashMap::new(),
        }
    }

    fn should_alert(&mut self, device_id: &str, now_ts: i64, cooldown: u64) -> bool {
        let currently_failed = true;
        let prev_failed = self.is_failed.get(device_id).copied().unwrap_or(false);

        if prev_failed && !currently_failed {
            self.is_failed.remove(device_id);
            return false;
        }

        let last = self.last_alert.get(device_id).copied().unwrap_or(0);
        if now_ts - last >= cooldown as i64 {
            self.last_alert.insert(device_id.to_string(), now_ts);
            self.is_failed.insert(device_id.to_string(), currently_failed);
            true
        } else {
            false
        }
    }

    fn mark_recovered(&mut self, device_id: &str) -> bool {
        self.is_failed.remove(device_id).is_some()
    }
}

// ────────────────────────────────────────────────────────────
// 核心检测逻辑（三级并发 + 信号量限流）
// ────────────────────────────────────────────────────────────

async fn check_port_with_semaphore(
    ip: &str,
    port: u16,
    timeout_sec: u64,
    semaphore: Arc<Semaphore>,
) -> bool {
    let _permit = semaphore.acquire().await;
    let addr = format!("{}:{}", ip, port);
    let timeout_dur = Duration::from_secs(timeout_sec);

    matches!(timeout(timeout_dur, TcpStream::connect(&addr)).await, Ok(Ok(_)))
}

async fn check_item_with_parallel_ip(
    check: &CheckItem,
    ips: &[String],
    timeout_sec: u64,
    semaphore: Arc<Semaphore>,
) -> (bool, Vec<String>) {
    let mut tasks = tokio::task::JoinSet::new();

    for ip in ips {
        let ip_clone = ip.clone();
        let sem_clone = semaphore.clone();
        let port = check.port;
        let to_sec = timeout_sec;

        tasks.spawn(async move {
            let success = check_port_with_semaphore(&ip_clone, port, to_sec, sem_clone).await;
            (ip_clone, success)
        });
    }

    let mut failed_ips = Vec::new();
    let mut any_success = false;

    while let Some(result) = tasks.join_next().await {
        if let Ok((ip, success)) = result {
            if success {
                any_success = true;
            } else {
                failed_ips.push(ip);
            }
        }
    }
    (any_success, failed_ips)
}

async fn check_device_parallel(
    device: &Device,
    timeout_sec: u64,
    semaphore: Arc<Semaphore>,
) -> (bool, Vec<CheckFailure>) {
    let mut tasks = tokio::task::JoinSet::new();

    for check in &device.checks {
        let check_clone = check.clone();
        let ips_clone = device.ips.clone();
        let sem_clone = semaphore.clone();
        let to_sec = timeout_sec;

        tasks.spawn(async move {
            let (success, failed_ips) = check_item_with_parallel_ip(
                &check_clone,
                &ips_clone,
                to_sec,
                sem_clone,
            ).await;
            (check_clone, success, failed_ips)
        });
    }

    let mut failures = Vec::new();

    while let Some(result) = tasks.join_next().await {
        if let Ok((check, success, failed_ips)) = result {
            if !success {
                failures.push(CheckFailure {
                    check_name: if check.name.is_empty() {
                        format!("port:{}", check.port)
                    } else {
                        check.name.clone()
                    },
                    port: check.port,
                    attempted_ips: failed_ips,
                });
            }
        }
    }
    (failures.is_empty(), failures)
}

#[derive(Clone)]
struct CheckFailure {
    check_name: String,
    port: u16,
    attempted_ips: Vec<String>,
}

// ────────────────────────────────────────────────────────────
// 告警发送（企业微信 Markdown - 竖列清晰 + 静默模式）
// ────────────────────────────────────────────────────────────

async fn send_wechat_alert(webhook: &str, device: &Device, failures: &[CheckFailure]) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let mut detail = String::from("```\n");

    for (idx, failure) in failures.iter().enumerate() {
        detail.push_str(&format!(
            "┌─ 🔴 {} (端口：{})\n",
            failure.check_name,
            failure.port
        ));

        let display_ips: Vec<&String> = failure.attempted_ips.iter().take(10).collect();
        for (ip_idx, ip) in display_ips.iter().enumerate() {
            let connector = if ip_idx == display_ips.len() - 1 {
                "│  └─"
            } else {
                "│  ├─"
            };
            detail.push_str(&format!("{} ❌ {}\n", connector, ip));
        }

        if failure.attempted_ips.len() > 10 {
            detail.push_str(&format!(
                "│  └─ ... 还有 {} 个 IP\n",
                failure.attempted_ips.len() - 10
            ));
        }

        if idx < failures.len() - 1 {
            detail.push_str("│\n");
        }
    }

    detail.push_str(&format!(
        "└─ 📊 统计：{} 项检查失败 | {} 个 IP 受影响\n",
        failures.len(),
        failures.iter().map(|f| f.attempted_ips.len()).sum::<usize>()
    ));
    detail.push_str("```\n");

    let priority_emoji = match device.priority.as_str() {
        "critical" => "🔴",
        "high" => "🟠",
        "medium" => "🟡",
        _ => "🔵",
    };

    let content = format!(
        "{} **{}** 故障告警\n\n\
        > 📍 位置：{}\n\
        > 💻 系统：{} | 🏷️ 分组：{}\n\
        > ⚠️ 优先级：{}\n\n\
        **故障详情**：\n{}\n\
        ---\n\
        <font color=\"warning\">建议：请检查设备电源/网络/服务状态</font>",
        priority_emoji,
        device.name,
        device.location,
        device.os,
        device.group,
        device.priority,
        detail
    );

    let payload = serde_json::json!({
        "msgtype": "markdown",
        "markdown": { "content": content }
    });

    // 静默发送：不打印日志
    let _ = client.post(webhook).json(&payload).send().await;
}

// ────────────────────────────────────────────────────────────
// 配置加载 + 环境变量替换 + URL 自动清理
// ────────────────────────────────────────────────────────────

fn load_config(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;

    let content = env::var("WEBHOOK_URL")
        .map(|val| content.replace("${WEBHOOK_URL}", &val))
        .unwrap_or(content);

    let mut config: Config = toml::from_str(&content)?;

    config.settings.webhook = config.settings.webhook.trim().to_string();

    if config.settings.interval < 5 {
        return Err("interval 不能小于 5 秒".into());
    }
    if config.settings.timeout < 1 || config.settings.timeout > 30 {
        return Err("timeout 应在 1-30 秒之间".into());
    }
    if !config.settings.webhook.starts_with("http") {
        return Err(format!(
            "webhook URL 格式错误，必须以 http/https 开头：{}",
            config.settings.webhook
        ).into());
    }

    Ok(config)
}

// ────────────────────────────────────────────────────────────
// 日志初始化（ChronoLocal 兼容方案）
// ────────────────────────────────────────────────────────────

fn init_logging() {
    let level = env::var("LOG_LEVEL")
        .ok()
        .and_then(|l| match l.to_lowercase().as_str() {
            "debug" => Some(Level::DEBUG),
            "info" => Some(Level::INFO),
            "warn" => Some(Level::WARN),
            "error" => Some(Level::ERROR),
            _ => None,
        })
        .unwrap_or(Level::INFO);

    let timer = tracing_subscriber::fmt::time::ChronoLocal::new(
        "%Y-%m-%dT%H:%M:%S%:z".to_string()
    );

    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_thread_ids(false)
        .with_target(false)
        .with_file(false)
        .with_line_number(false)
        .with_level(true)
        .with_timer(timer)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);
}

// ────────────────────────────────────────────────────────────
// 主程序入口
// ────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    init_logging();

    println!();
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║     🚀 netpulse-rs v2.2.5 (高性能并发版)                  ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    info!("📅 启动时间：{}", Local::now().format("%Y-%m-%d %H:%M:%S"));

    let config = match load_config("config.toml") {
        Ok(c) => {
            info!("✓ 配置加载成功");
            info!("  ├─ 设备数量：{}", c.devices.len());
            info!("  ├─ 检测间隔：{}s", c.settings.interval);
            info!("  ├─ 连接超时：{}s", c.settings.timeout);
            info!("  └─ 并发限制：{} 连接", c.settings.max_concurrent_connections);
            println!();
            c
        }
        Err(e) => {
            error!("✗ 配置加载失败：{}", e);
            std::process::exit(1);
        }
    };

    let semaphore = Arc::new(Semaphore::new(config.settings.max_concurrent_connections));
    let alert_state = Arc::new(Mutex::new(AlertState::new()));
    let webhook = config.settings.webhook.clone();
    let timeout_sec = config.settings.timeout;
    let interval_sec = config.settings.interval;
    let cooldown_sec = config.settings.alert_cooldown;

    let shutdown_signal = async {
        let _ = tokio::signal::ctrl_c().await;
        println!();
        warn!("🛑 收到退出信号，正在关闭...");
    };

    let monitor_loop = async {
        let mut round = 0u64;
        let mut total_alerts = 0u64;
        let mut recovered_count = 0u64;

        loop {
            round += 1;
            let round_start = Instant::now();

            if round.is_multiple_of(10) {
                info!("╔══════════════════════════════════════════════════════════╗");
            }

            let mut tasks = tokio::task::JoinSet::new();

            for device in &config.devices {
                let dev = device.clone();
                let to_sec = timeout_sec;
                let sem = semaphore.clone();

                tasks.spawn(async move {
                    let (ok, failures) = check_device_parallel(&dev, to_sec, sem).await;
                    (dev, ok, failures)
                });
            }

            let mut group_failures: HashMap<String, Vec<(Device, Vec<CheckFailure>)>> = HashMap::new();

            while let Some(result) = tasks.join_next().await {
                // ✅ 修复：使用 if let 替代单模式 match（clippy::single_match）
                if let Ok((device, is_ok, failures)) = result {
                    if !is_ok {
                        group_failures
                            .entry(device.group.clone())
                            .or_default()
                            .push((device, failures));
                    } else {
                        let mut state = alert_state.lock().await;
                        if state.mark_recovered(&device.id) {
                            recovered_count += 1;
                        }
                    }
                }
            }

            let now_ts = Local::now().timestamp();
            let mut state = alert_state.lock().await;
            let mut new_alerts = 0u64;

            // ✅ 修复：使用 .values() 替代键值迭代（clippy::for_kv_map）
            for failed_list in group_failures.values() {
                for (device, failures) in failed_list {
                    if state.should_alert(&device.id, now_ts, cooldown_sec) {
                        new_alerts += 1;
                        total_alerts += 1;

                        let webhook_clone = webhook.clone();
                        let dev_clone = device.clone();
                        let failures_clone = failures.clone();

                        drop(state);
                        send_wechat_alert(&webhook_clone, &dev_clone, &failures_clone).await;
                        state = alert_state.lock().await;
                    }
                }
            }

            let elapsed = round_start.elapsed().as_secs();

            if group_failures.is_empty() {
                info!("✓ 第 {:>3} 轮 | 全部正常 | 耗时：{}s", round, elapsed);
            } else {
                warn!(
                    "⚠ 第 {:>3} 轮 | {} 设备故障 | {} 告警发送 | 耗时：{}s",
                    round,
                    group_failures.values().map(|v| v.len()).sum::<usize>(),
                    new_alerts,
                    elapsed
                );
            }


            if round.is_multiple_of(10) {
                info!("╚══════════════════════════════════════════════════════════╝");
                info!(
                    "📊 累计：{} 轮 | 告警：{} 次 | 恢复：{} 台",
                    round, total_alerts, recovered_count
                );
                println!();
            }

            if elapsed < interval_sec {
                sleep(Duration::from_secs(interval_sec - elapsed)).await;
            }
        }
    };

    tokio::select! {
        _ = monitor_loop => {},
        _ = shutdown_signal => {
            println!();
            info!("👋 系统已优雅退出");
            println!("╔══════════════════════════════════════════════════════════╗");
            println!("║                    感谢使用，再见！                       ║");
            println!("╚══════════════════════════════════════════════════════════╝");
            println!();
        }
    }
}
