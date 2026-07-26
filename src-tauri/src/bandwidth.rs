//! 带宽管理 — Phase 3
//!
//! 通过读写 Kubo 配置文件来限制上传/下载速率。
//!
//! Kubo 0.19+ 支持 `Swarm.ResourceMgr.Limits` 配置：
//! ```json
//! "Swarm": {
//!   "ResourceMgr": {
//!     "Limits": {
//!       "System": {
//!         "Connections": 1024,
//!         "Streams": 4096
//!       },
//!       "Transient": {
//!         "Connections": 256,
//!         "Streams": 1024
//!       }
//!     }
//!   }
//! }
//! ```
//!
//! 注意：Kubo 不支持直接的字节速率限制。
//! 本模块通过 `ipfs config` 命令间接管理，并提供：
//! - 连接数限制（间接限制带宽）
//! - Swarm 连接上限控制
//! - 实时速率监控（通过 stats/bw API）

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;

// ════════════════════════════════════════════════════════════════
// 带宽配置
// ════════════════════════════════════════════════════════════════

/// 带宽管理器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthConfig {
    /// 最大连接数（对等节点）
    pub max_connections: u32,
    /// 最大流数
    pub max_streams: u32,
    /// 上传限速（字节/秒，0 = 不限制）
    /// 通过 Swarm.Transports.Network 的 wrapper 实现（如使用代理）
    pub upload_limit: u64,
    /// 下载限速（字节/秒，0 = 不限制）
    pub download_limit: u64,
    /// 是否启用带宽管理
    pub enabled: bool,
}

impl Default for BandwidthConfig {
    fn default() -> Self {
        Self {
            max_connections: 600,
            max_streams: 2048,
            upload_limit: 0,
            download_limit: 0,
            enabled: true,
        }
    }
}

/// 当前带宽状态（从 stats/bw 获取）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthStatus {
    /// 当前下载速率（B/s）
    pub rate_in: f64,
    /// 当前上传速率（B/s）
    pub rate_out: f64,
    /// 累计下载字节
    pub total_in: u64,
    /// 累计上传字节
    pub total_out: u64,
}

// ════════════════════════════════════════════════════════════════
// Kubo 配置管理器
// ════════════════════════════════════════════════════════════════

/// Kubo 配置读写器
///
/// 通过调用 `ipfs config` CLI 命令来管理 Kubo 配置。
pub struct KuboConfigManager {
    /// IPFS 二进制路径
    binary_path: PathBuf,
    /// IPFS 仓库路径
    repo_path: PathBuf,
}

impl KuboConfigManager {
    pub fn new(binary_path: PathBuf, repo_path: PathBuf) -> Self {
        Self { binary_path, repo_path }
    }

    /// 读取 Kubo 配置项
    pub fn get_config(&self, key: &str) -> Result<String, String> {
        let output = Command::new(&self.binary_path)
            .env("IPFS_PATH", &self.repo_path)
            .args(["config", key])
            .output()
            .map_err(|e| format!("Failed to run ipfs config: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    /// 写入 Kubo 配置项
    pub fn set_config(&self, key: &str, value: &str) -> Result<(), String> {
        let output = Command::new(&self.binary_path)
            .env("IPFS_PATH", &self.repo_path)
            .args(["config", key, value])
            .output()
            .map_err(|e| format!("Failed to run ipfs config: {}", e))?;

        if output.status.success() {
            tracing::info!("Kubo config set: {} = {}", key, value);
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    /// 应用带宽配置到 Kubo
    pub fn apply_bandwidth_config(&self, cfg: &BandwidthConfig) -> Result<(), String> {
        if !cfg.enabled {
            tracing::info!("Bandwidth management disabled, skipping config write");
            return Ok(());
        }

        // 设置连接限制
        self.set_config(
            "Swarm.ResourceMgr.Limits.System.Connections",
            &cfg.max_connections.to_string(),
        )?;

        // 设置流限制
        self.set_config(
            "Swarm.ResourceMgr.Limits.System.Streams",
            &cfg.max_streams.to_string(),
        )?;

        // 设置瞬时连接限制
        let transient_conns = (cfg.max_connections / 4).max(64);
        self.set_config(
            "Swarm.ResourceMgr.Limits.Transient.Connections",
            &transient_conns.to_string(),
        )?;

        let transient_streams = (cfg.max_streams / 4).max(256);
        self.set_config(
            "Swarm.ResourceMgr.Limits.Transient.Streams",
            &transient_streams.to_string(),
        )?;

        tracing::info!(
            "Bandwidth config applied: {} conns, {} streams",
            cfg.max_connections,
            cfg.max_streams
        );
        Ok(())
    }

    /// 获取当前 Swarm 连接限制
    pub fn get_swarm_limits(&self) -> Result<SwarmLimits, String> {
        let conns = self.get_config("Swarm.ResourceMgr.Limits.System.Connections")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(600);

        let streams = self.get_config("Swarm.ResourceMgr.Limits.System.Streams")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2048);

        Ok(SwarmLimits { max_connections: conns, max_streams: streams })
    }
}

/// Swarm 当前限制
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmLimits {
    pub max_connections: u32,
    pub max_streams: u32,
}

// ════════════════════════════════════════════════════════════════
// 带宽监控器
// ════════════════════════════════════════════════════════════════

/// 带宽速率采样器
///
/// 存储最近 N 个采样点，计算平滑速率。
pub struct BandwidthMonitor {
    /// 下载速率历史（B/s）
    rate_in_history: Vec<f64>,
    /// 上传速率历史（B/s）
    rate_out_history: Vec<f64>,
    /// 上次采样时的累计值
    last_total_in: u64,
    last_total_out: u64,
    last_sample_time: Option<std::time::Instant>,
    /// 最大历史点数
    max_history: usize,
}

impl BandwidthMonitor {
    pub fn new() -> Self {
        Self {
            rate_in_history: Vec::new(),
            rate_out_history: Vec::new(),
            last_total_in: 0,
            last_total_out: 0,
            last_sample_time: None,
            max_history: 30, // 保留最近 30 个采样点
        }
    }

    /// 添加新采样点
    pub fn sample(&mut self, total_in: u64, total_out: u64) {
        let now = std::time::Instant::now();

        if let Some(last_time) = self.last_sample_time {
            let elapsed = now.duration_since(last_time).as_secs_f64();
            if elapsed > 0.0 {
                let rate_in = (total_in.saturating_sub(self.last_total_in)) as f64 / elapsed;
                let rate_out = (total_out.saturating_sub(self.last_total_out)) as f64 / elapsed;

                self.rate_in_history.push(rate_in);
                self.rate_out_history.push(rate_out);

                if self.rate_in_history.len() > self.max_history {
                    self.rate_in_history.remove(0);
                }
                if self.rate_out_history.len() > self.max_history {
                    self.rate_out_history.remove(0);
                }
            }
        }

        self.last_total_in = total_in;
        self.last_total_out = total_out;
        self.last_sample_time = Some(now);
    }

    /// 平滑下载速率（B/s）
    pub fn smooth_rate_in(&self) -> f64 {
        if self.rate_in_history.is_empty() {
            return 0.0;
        }
        self.rate_in_history.iter().sum::<f64>() / self.rate_in_history.len() as f64
    }

    /// 平滑上传速率（B/s）
    pub fn smooth_rate_out(&self) -> f64 {
        if self.rate_out_history.is_empty() {
            return 0.0;
        }
        self.rate_out_history.iter().sum::<f64>() / self.rate_out_history.len() as f64
    }
}

impl Default for BandwidthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_bandwidth_config() {
        let cfg = BandwidthConfig::default();
        assert_eq!(cfg.max_connections, 600);
        assert!(cfg.enabled);
    }

    #[test]
    fn test_bandwidth_monitor_sampling() {
        let mut monitor = BandwidthMonitor::new();

        // 初始采样
        monitor.sample(1000, 500);
        assert_eq!(monitor.smooth_rate_in(), 0.0); // 第一次没有速率

        // 模拟 1 秒后 100MB 下载
        std::thread::sleep(std::time::Duration::from_millis(100));
        monitor.sample(101_000_000, 1_000_000);

        let rate = monitor.smooth_rate_in();
        assert!(rate > 0.0, "Rate should be positive after second sample");
    }

    #[test]
    fn test_bandwidth_monitor_history_limit() {
        let mut monitor = BandwidthMonitor::new();
        for i in 0..50 {
            monitor.sample(i * 1000, i * 500);
        }
        assert!(monitor.rate_in_history.len() <= 30);
        assert!(monitor.rate_out_history.len() <= 30);
    }

    #[test]
    fn test_kubo_config_parse() {
        // 只需验证结构体可以构造
        let limits = SwarmLimits {
            max_connections: 600,
            max_streams: 2048,
        };
        let json = serde_json::to_string(&limits).unwrap();
        let parsed: SwarmLimits = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.max_connections, 600);
    }
}
