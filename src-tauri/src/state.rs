use std::sync::Arc;
use tokio::sync::RwLock;
use crate::config::AppConfig;
use crate::types::DaemonStatus;
use crate::daemon::{DaemonController, IpfsApiClient};

/// 全局应用状态
#[derive(Clone)]
pub struct AppState {
    /// 配置
    pub config: Arc<RwLock<AppConfig>>,
    
    /// 守护进程状态
    pub daemon_status: Arc<RwLock<DaemonStatus>>,
    
    /// 守护进程控制器
    pub daemon_controller: Arc<RwLock<Option<DaemonController>>>,
    
    /// IPFS API 客户端
    pub api_client: Arc<RwLock<Option<IpfsApiClient>>>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        // 创建 API 客户端
        let api_client = IpfsApiClient::new(config.api_addr.clone());
        
        Self {
            config: Arc::new(RwLock::new(config)),
            daemon_status: Arc::new(RwLock::new(DaemonStatus::default())),
            daemon_controller: Arc::new(RwLock::new(None)),
            api_client: Arc::new(RwLock::new(Some(api_client))),
        }
    }
    
    /// 获取当前配置的克隆
    pub async fn get_config(&self) -> AppConfig {
        self.config.read().await.clone()
    }
    
    /// 更新配置
    pub async fn update_config(&self, new_config: AppConfig) {
        // 更新 API 客户端
        let new_client = IpfsApiClient::new(new_config.api_addr.clone());
        *self.api_client.write().await = Some(new_client);
        
        // 更新配置
        *self.config.write().await = new_config;
    }
    
    /// 获取守护进程状态
    pub async fn get_daemon_status(&self) -> DaemonStatus {
        self.daemon_status.read().await.clone()
    }
    
    /// 设置守护进程状态
    pub async fn set_daemon_status(&self, status: DaemonStatus) {
        *self.daemon_status.write().await = status;
    }
    
    /// 获取守护进程控制器的引用
    pub async fn get_daemon_controller(&self) -> Option<DaemonController> {
        self.daemon_controller.read().await.clone()
    }
    
    /// 设置守护进程控制器
    pub async fn set_daemon_controller(&self, controller: Option<DaemonController>) {
        *self.daemon_controller.write().await = controller;
    }
    
    /// 获取 API 客户端
    pub async fn get_api_client(&self) -> Option<IpfsApiClient> {
        self.api_client.read().await.clone()
    }
}
