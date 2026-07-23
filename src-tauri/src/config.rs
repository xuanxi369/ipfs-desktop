use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;

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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            ipfs_path: None, // None 表示使用默认路径 (~/.ipfs)
            api_addr: "http://127.0.0.1:5001".to_string(),
            gateway_addr: "http://127.0.0.1:8080".to_string(),
            daemon_flags: vec![
                "--migrate=true".to_string(),
                "--enable-gc=true".to_string(),
            ],
            auto_launch: false,
            auto_gc: true,
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
        let config_path = Self::config_path();
        
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
        let config_path = Self::config_path();
        
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
        fs::write(&config_path, content)
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
        // 验证 API 地址格式
        if !self.api_addr.starts_with("http://") && !self.api_addr.starts_with("https://") {
            return Err("API address must start with http:// or https://".to_string());
        }
        
        // 验证 Gateway 地址格式
        if !self.gateway_addr.starts_with("http://") && !self.gateway_addr.starts_with("https://") {
            return Err("Gateway address must start with http:// or https://".to_string());
        }
        
        // 验证 IPFS 路径（如果指定了）
        if let Some(ref path) = self.ipfs_path {
            if path.to_string_lossy().is_empty() {
                return Err("IPFS path cannot be empty".to_string());
            }
        }
        
        Ok(())
    }
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
    fn test_save_and_load() {
        let config = AppConfig::default();
        
        // 保存配置
        config.save().expect("Failed to save config");
        
        // 加载配置
        let loaded = AppConfig::load().expect("Failed to load config");
        
        // 验证
        assert_eq!(config.api_addr, loaded.api_addr);
        assert_eq!(config.gateway_addr, loaded.gateway_addr);
    }
}
