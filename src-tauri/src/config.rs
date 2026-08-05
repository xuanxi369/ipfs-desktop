use serde::{Deserialize, Serialize};
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// IPFS 仓库路径
    pub ipfs_path: Option<PathBuf>,

    /// API 地址
    pub api_addr: String,

    /// Gateway 地址
    pub gateway_addr: String,

    /// 启动参数
    pub daemon_flags: Vec<String>,

    /// 是否开机自启动
    pub auto_launch: bool,

    /// 是否自动垃圾回收
    pub auto_gc: bool,

    /// 守护进程意外崩溃时是否自动重启（Phase D2 自愈）。
    /// 旧配置文件缺此字段时按 serde 默认（true）——「可长期在线」的默认姿态。
    #[serde(default = "default_true")]
    pub auto_restart: bool,

    /// 双栈路由策略，重启后保持用户选择。
    #[serde(default = "default_route_policy")]
    pub route_policy: String,

    /// 可选的 Kubo 二进制 SHA-256。设置后，不匹配的二进制会被拒绝。
    #[serde(default)]
    pub kubo_binary_sha256: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_route_policy() -> String {
    "KuboOnly".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            ipfs_path: None, // None 表示使用默认路径 (~/.ipfs)
            api_addr: "http://127.0.0.1:5001".to_string(),
            gateway_addr: "http://127.0.0.1:8080".to_string(),
            daemon_flags: vec!["--migrate=true".to_string(), "--enable-gc=true".to_string()],
            auto_launch: false,
            auto_gc: true,
            auto_restart: true,
            route_policy: default_route_policy(),
            kubo_binary_sha256: None,
        }
    }
}

impl AppConfig {
    /// 获取配置文件路径
    pub fn config_path() -> PathBuf {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap())
            .join("ipfs-desktop-rust");

        // 确保目录存在
        let _ = fs::create_dir_all(&config_dir);

        config_dir.join("config.json")
    }

    /// 从磁盘加载配置
    pub fn load() -> Result<Self, String> {
        Self::load_from(&Self::config_path())
    }

    pub fn load_from(config_path: &std::path::Path) -> Result<Self, String> {
        if !config_path.exists() {
            tracing::info!("Config file not found, using defaults");
            return Ok(Self::default());
        }

        tracing::info!("Loading config from: {:?}", config_path);

        let content = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config file: {}", e))?;

        let config: Self = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse config file: {}", e))?;

        tracing::info!("Config loaded successfully");
        Ok(config)
    }

    /// 保存配置到磁盘
    pub fn save(&self) -> Result<(), String> {
        self.save_to(&Self::config_path())
    }

    pub fn save_to(&self, config_path: &std::path::Path) -> Result<(), String> {
        tracing::info!("Saving config to: {:?}", config_path);

        // 确保父目录存在
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        // 序列化配置
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        // 写入文件
        crate::atomic_file::write_atomic(config_path, content.as_bytes())
            .map_err(|e| format!("Failed to write config file: {}", e))?;

        tracing::info!("Config saved successfully");
        Ok(())
    }

    /// 获取 IPFS 仓库路径
    ///
    /// 如果配置中没有指定，则使用默认路径
    pub fn get_ipfs_path(&self) -> PathBuf {
        if let Some(ref path) = self.ipfs_path {
            path.clone()
        } else {
            // 使用默认路径
            dirs::home_dir()
                .unwrap_or_else(|| std::env::current_dir().unwrap())
                .join(".ipfs")
        }
    }

    /// 验证配置的有效性
    pub fn validate(&self) -> Result<(), String> {
        validate_service_url(&self.api_addr, true)?;
        validate_service_url(&self.gateway_addr, false)?;

        // 验证 IPFS 路径（如果指定了）
        if let Some(ref path) = self.ipfs_path {
            if path.to_string_lossy().is_empty() {
                return Err("IPFS path cannot be empty".to_string());
            }
        }

        if crate::backend_router::RoutePolicy::parse(&self.route_policy).is_none() {
            return Err("route_policy must be KuboOnly, IrohOnly, or Auto".to_string());
        }
        if let Some(hash) = &self.kubo_binary_sha256 {
            if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("kubo_binary_sha256 must be 64 hexadecimal characters".to_string());
            }
        }

        Ok(())
    }
}

fn validate_service_url(value: &str, api: bool) -> Result<(), String> {
    let parsed =
        reqwest::Url::parse(value).map_err(|_| "address must be a valid URL".to_string())?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err("address must use http or https".into());
    }
    if parsed.username() != ""
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("address cannot contain credentials, query, or fragment".into());
    }
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| "address must specify a known port".to_string())?;
    if port == 0 {
        return Err("address port is invalid".into());
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "address host is required".to_string())?;
    let local = host == "localhost"
        || host == "ip6-localhost"
        || host
            .parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false);
    if api && !local && scheme != "https" {
        return Err("remote API addresses must use HTTPS".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.api_addr, "http://127.0.0.1:5001");
        assert_eq!(config.gateway_addr, "http://127.0.0.1:8080");
        assert!(config.auto_gc);
        assert!(!config.auto_launch);
    }

    #[test]
    fn test_config_validation() {
        let mut config = AppConfig::default();
        assert!(config.validate().is_ok());

        // 测试无效的 API 地址
        config.api_addr = "invalid".to_string();
        assert!(config.validate().is_err());

        // 恢复有效地址
        config.api_addr = "http://127.0.0.1:5001".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_auto_restart_serde_default() {
        // 旧配置文件（缺 auto_restart 字段）应反序列化为默认 true（零回归 + 自愈默认开）
        let json = r#"{"ipfs_path":null,"api_addr":"http://127.0.0.1:5001","gateway_addr":"http://127.0.0.1:8080","daemon_flags":[],"auto_launch":false,"auto_gc":true}"#;
        let cfg: AppConfig = serde_json::from_str(json).expect("legacy config should parse");
        assert!(
            cfg.auto_restart,
            "missing auto_restart must default to true"
        );
        assert!(AppConfig::default().auto_restart);
    }

    #[test]
    fn test_save_and_load() {
        let config = AppConfig::default();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        config.save_to(&path).expect("Failed to save config");
        let loaded = AppConfig::load_from(&path).expect("Failed to load config");

        // 验证
        assert_eq!(config.api_addr, loaded.api_addr);
        assert_eq!(config.gateway_addr, loaded.gateway_addr);
    }
}
