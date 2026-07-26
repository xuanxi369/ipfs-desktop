use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use crate::config::AppConfig;
use crate::types::DaemonStatus;
use crate::daemon::{DaemonController, IpfsApiClient};
use crate::cache::CacheStore;
use crate::keyring::KeyManager;
use crate::proxy::ProxyClient;
use crate::offline_queue::{OfflineQueue, ReplayEngine};
use crate::bandwidth::{BandwidthConfig, BandwidthMonitor, KuboConfigManager};
use crate::backend_trait::{Backend, BackendType};
use crate::kubo_adapter::KuboBackend;
use crate::iroh_adapter::IrohBackend;

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

    /// 智能代理客户端（Phase 3）
    pub proxy_client: Arc<RwLock<Option<ProxyClient>>>,

    /// 健康监控任务句柄
    pub health_monitor: Arc<RwLock<Option<JoinHandle<()>>>>,

    /// SQLite 缓存
    pub cache: Arc<CacheStore>,

    /// 密钥管理器
    pub key_manager: Arc<KeyManager>,

    /// 仪表盘自动轮询任务句柄
    pub dashboard_poller: Arc<RwLock<Option<JoinHandle<()>>>>,

    /// 离线操作队列（Phase 3）
    pub offline_queue: Arc<OfflineQueue>,

    /// 离线队列重放任务句柄（Phase 3）
    pub replay_handle: Arc<RwLock<Option<JoinHandle<()>>>>,

    /// 带宽配置（Phase 3）
    pub bandwidth_config: Arc<RwLock<BandwidthConfig>>,

    /// 带宽监控器（Phase 3）
    pub bandwidth_monitor: Arc<std::sync::Mutex<BandwidthMonitor>>,

    /// Kubo 配置管理器（Phase 3）
    pub kubo_config: Arc<RwLock<Option<KuboConfigManager>>>,

    /// 当前活跃后端类型（Phase 4）
    pub active_backend: Arc<RwLock<BackendType>>,

    /// Kubo 后端实例（Phase 4）
    pub kubo_backend: Arc<KuboBackend>,

    /// Iroh 后端实例（Phase 4）
    pub iroh_backend: Arc<IrohBackend>,
}

impl AppState {
    /// 创建应用状态（在 lib.rs::run 中调用）
    pub fn new(config: AppConfig) -> Self {
        let api_client = IpfsApiClient::new(config.api_addr.clone());

        // 初始化缓存数据库
        let cache_dir = dirs::data_local_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap())
            .join("ipfs-desktop-rust");
        let cache = CacheStore::new(cache_dir.join("cache.db"))
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to open cache: {}, using fallback", e);
                // 如果 SQLite 失败（极少数情况），尝试内存缓存
                CacheStore::new(std::env::temp_dir().join("ipfs-cache-fallback.db"))
                    .expect("Failed to create fallback cache")
            });

        let key_manager = KeyManager::new();

        // Phase 3: 初始化智能代理
        let cache_arc = Arc::new(cache);
        let proxy_client = ProxyClient::new(config.api_addr.clone(), cache_arc.clone());

        // Phase 3: 初始化离线队列
        let offline_queue = OfflineQueue::new(cache_dir.join("offline_queue.db"))
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to open offline queue: {}, using fallback", e);
                OfflineQueue::new(std::env::temp_dir().join("ipfs-offline-queue-fallback.db"))
                    .expect("Failed to create fallback offline queue")
            });

        // Phase 4: 初始化双后端
        let kubo_backend = KuboBackend::new(config.api_addr.clone());
        let iroh_backend = IrohBackend::new(cache_dir.join("iroh-data"));

        Self {
            config: Arc::new(RwLock::new(config)),
            daemon_status: Arc::new(RwLock::new(DaemonStatus::default())),
            daemon_controller: Arc::new(RwLock::new(None)),
            api_client: Arc::new(RwLock::new(Some(api_client))),
            proxy_client: Arc::new(RwLock::new(Some(proxy_client))),
            health_monitor: Arc::new(RwLock::new(None)),
            cache: cache_arc,
            key_manager: Arc::new(key_manager),
            dashboard_poller: Arc::new(RwLock::new(None)),
            offline_queue: Arc::new(offline_queue),
            replay_handle: Arc::new(RwLock::new(None)),
            bandwidth_config: Arc::new(RwLock::new(BandwidthConfig::default())),
            bandwidth_monitor: Arc::new(std::sync::Mutex::new(BandwidthMonitor::new())),
            kubo_config: Arc::new(RwLock::new(None)),
            active_backend: Arc::new(RwLock::new(BackendType::Kubo)),
            kubo_backend: Arc::new(kubo_backend),
            iroh_backend: Arc::new(iroh_backend),
        }
    }

    /// 获取当前配置的克隆
    pub async fn get_config(&self) -> AppConfig {
        self.config.read().await.clone()
    }

    /// 更新配置
    pub async fn update_config(&self, new_config: AppConfig) {
        let new_client = IpfsApiClient::new(new_config.api_addr.clone());
        *self.api_client.write().await = Some(new_client);
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

    /// 启动健康监控后台任务
    pub async fn spawn_health_monitor(&self, app_handle: tauri::AppHandle) {
        self.cancel_health_monitor().await;

        let state = self.clone();
        let handle = tokio::spawn(async move {
            tracing::info!("Health monitor started");
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

                let controller_opt = state.get_daemon_controller().await;
                match controller_opt {
                    None => {
                        tracing::info!("Health monitor: controller removed, stopping");
                        break;
                    }
                    Some(controller) => {
                        if !controller.is_running().await {
                            tracing::error!("Health monitor: daemon process died unexpectedly!");
                            let error_msg = "Daemon process exited unexpectedly (detected by health monitor)".to_string();
                            state.set_daemon_status(
                                DaemonStatus::Failed { error: error_msg.clone() }
                            ).await;
                            state.set_daemon_controller(None).await;
                            if let Err(e) = app_handle.emit("daemon-status-changed",
                                &DaemonStatus::Failed { error: error_msg }) {
                                tracing::warn!("Failed to emit daemon-status-changed from health monitor: {}", e);
                            }
                            break;
                        }
                    }
                }
            }
            *state.health_monitor.write().await = None;
            tracing::info!("Health monitor stopped");
        });

        *self.health_monitor.write().await = Some(handle);
    }

    /// 取消健康监控
    pub async fn cancel_health_monitor(&self) {
        if let Some(handle) = self.health_monitor.write().await.take() {
            handle.abort();
            tracing::info!("Health monitor cancelled");
        }
    }

    // ── Phase 2: 仪表盘自动轮询 ──

    /// 启动仪表盘自动轮询
    ///
    /// 每 10 秒从 Kubo API 拉取最新数据，缓存到 SQLite，
    /// 并通过 Tauri event 推送到前端。
    /// 当守护进程不是 Running 状态时自动停止。
    pub async fn spawn_dashboard_poller(&self, app_handle: tauri::AppHandle) {
        self.cancel_dashboard_poller().await;

        let state = self.clone();
        let handle = tokio::spawn(async move {
            tracing::info!("Dashboard poller started (interval: 10s)");
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

                // 检查守护进程是否在运行
                let status = state.get_daemon_status().await;
                if !matches!(status, DaemonStatus::Running { .. }) {
                    tracing::info!("Dashboard poller: daemon not running, stopping");
                    break;
                }

                // 获取最新数据并缓存
                if let Some(client) = state.get_api_client().await {
                    // 并行获取（每个失败不影响其他）
                    let peers_fut = async {
                        match client.swarm_peers().await {
                            Ok(p) => {
                                let json = serde_json::to_string(&p).unwrap_or_default();
                                state.cache.set_peers(&json);
                                Some(p)
                            }
                            Err(e) => { tracing::warn!("Poller peers: {}", e); None }
                        }
                    };

                    let bw_fut = async {
                        match client.stats_bw().await {
                            Ok(b) => {
                                let json = serde_json::to_string(&b).unwrap_or_default();
                                state.cache.set_bandwidth(&json);
                                Some(b)
                            }
                            Err(e) => { tracing::warn!("Poller bandwidth: {}", e); None }
                        }
                    };

                    let bs_fut = async {
                        match client.bitswap_stat().await {
                            Ok(b) => {
                                let json = serde_json::to_string(&b).unwrap_or_default();
                                state.cache.set_bitswap(&json);
                                Some(b)
                            }
                            Err(e) => { tracing::warn!("Poller bitswap: {}", e); None }
                        }
                    };

                    let repo_fut = async {
                        match client.repo_stat().await {
                            Ok(r) => {
                                let json = serde_json::to_string(&r).unwrap_or_default();
                                state.cache.set_repo_stats(&json);
                                Some(r)
                            }
                            Err(e) => { tracing::warn!("Poller repo: {}", e); None }
                        }
                    };

                    let (peers, bw, bs, repo) = tokio::join!(peers_fut, bw_fut, bs_fut, repo_fut);

                    // 推送到前端
                    use serde::Serialize;
                    #[derive(Serialize)]
                    struct DashboardTick {
                        peers: Option<crate::daemon::SwarmPeers>,
                        bandwidth: Option<crate::daemon::BandwidthStats>,
                        bitswap: Option<crate::daemon::BitswapStats>,
                        repo: Option<crate::daemon::RepoStats>,
                    }

                    let tick = DashboardTick { peers, bandwidth: bw, bitswap: bs, repo };
                    if let Err(e) = app_handle.emit("dashboard-tick", &tick) {
                        tracing::warn!("Failed to emit dashboard-tick: {}", e);
                    }
                }
            }
            *state.dashboard_poller.write().await = None;
            tracing::info!("Dashboard poller stopped");
        });

        *self.dashboard_poller.write().await = Some(handle);
    }

    /// 取消仪表盘轮询
    pub async fn cancel_dashboard_poller(&self) {
        if let Some(handle) = self.dashboard_poller.write().await.take() {
            handle.abort();
            tracing::info!("Dashboard poller cancelled");
        }
    }

    // ── Phase 3: 离线队列重放 ──

    /// 启动离线队列重放循环
    ///
    /// 当守护进程变为 Running 状态时调用。
    /// 每 15 秒检查队列，发现待处理条目则重放。
    pub async fn spawn_replay_loop(&self, app_handle: tauri::AppHandle) {
        // 取消已有循环
        if let Some(handle) = self.replay_handle.write().await.take() {
            handle.abort();
        }

        let queue = self.offline_queue.clone();
        let state = self.clone();

        let handle = tokio::spawn(async move {
            tracing::info!("Offline replay loop started");
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;

                let status = state.get_daemon_status().await;
                if !matches!(status, DaemonStatus::Running { .. }) {
                    tracing::info!("Replay loop: daemon not running, stopping");
                    break;
                }

                if let Ok(true) = queue.is_empty() {
                    continue;
                }

                // 获取 API 客户端并重放
                if let Some(api) = state.get_api_client().await {
                    let engine = ReplayEngine::new(queue.clone());
                    let (success, failed) = engine.replay_all(&api).await;
                    if success > 0 || failed > 0 {
                        if let Err(e) = app_handle.emit("replay-progress", &serde_json::json!({
                            "success": success,
                            "failed": failed,
                            "remaining": queue.len().unwrap_or(0),
                        })) {
                            tracing::warn!("Failed to emit replay-progress: {}", e);
                        }
                    }
                }
            }
            *state.replay_handle.write().await = None;
            tracing::info!("Offline replay loop stopped");
        });

        *self.replay_handle.write().await = Some(handle);
    }

    /// 获取代理客户端
    pub async fn get_proxy_client(&self) -> Option<ProxyClient> {
        self.proxy_client.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    #[tokio::test]
    async fn test_state_new() {
        let config = AppConfig::default();
        let state = AppState::new(config.clone());

        let loaded = state.get_config().await;
        assert_eq!(loaded.api_addr, config.api_addr);
    }

    #[tokio::test]
    async fn test_state_default_status() {
        let state = AppState::new(AppConfig::default());
        let status = state.get_daemon_status().await;
        assert!(matches!(status, DaemonStatus::Stopped));
    }

    #[tokio::test]
    async fn test_state_set_get_status() {
        let state = AppState::new(AppConfig::default());

        state.set_daemon_status(DaemonStatus::Starting).await;
        assert!(matches!(state.get_daemon_status().await, DaemonStatus::Starting));

        state.set_daemon_status(DaemonStatus::Running {
            pid: 1234,
            peer_id: "test".into(),
            api_addr: "http://localhost:5001".into(),
        }).await;
        assert!(matches!(state.get_daemon_status().await, DaemonStatus::Running { .. }));

        state.set_daemon_status(DaemonStatus::Stopped).await;
        assert!(matches!(state.get_daemon_status().await, DaemonStatus::Stopped));
    }

    #[tokio::test]
    async fn test_state_controller_absent_initially() {
        let state = AppState::new(AppConfig::default());
        assert!(state.get_daemon_controller().await.is_none());
    }

    #[tokio::test]
    async fn test_state_api_client_present_initially() {
        let state = AppState::new(AppConfig::default());
        assert!(state.get_api_client().await.is_some());
    }

    #[tokio::test]
    async fn test_state_update_config() {
        let state = AppState::new(AppConfig::default());
        let mut new_config = AppConfig::default();
        new_config.api_addr = "http://127.0.0.1:6001".to_string();

        state.update_config(new_config.clone()).await;
        assert_eq!(state.get_config().await.api_addr, "http://127.0.0.1:6001");
    }

    #[tokio::test]
    async fn test_health_monitor_cancel_without_spawn() {
        let state = AppState::new(AppConfig::default());
        state.cancel_health_monitor().await;
        assert!(state.health_monitor.read().await.is_none());
    }

    #[tokio::test]
    async fn test_cache_present() {
        let state = AppState::new(AppConfig::default());
        // 缓存应该可用（写入并读取）
        state.cache.set_dashboard(r#"{"test":1}"#);
        assert!(state.cache.get_dashboard().is_some());
    }

    #[tokio::test]
    async fn test_key_manager_present() {
        let state = AppState::new(AppConfig::default());
        let kp = state.key_manager.generate_key("state-test").unwrap();
        assert!(!kp.public_key.is_empty());
        state.key_manager.delete_key("state-test").unwrap();
    }
}
