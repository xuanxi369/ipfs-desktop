use crate::backend_trait::BackendType;
use crate::bandwidth::{BandwidthConfig, BandwidthMonitor, KuboConfigManager};
use crate::cache::CacheStore;
use crate::config::AppConfig;
use crate::daemon::{DaemonController, IpfsApiClient};
use crate::iroh_adapter::IrohBackend;
use crate::keyring::KeyManager;
use crate::kubo_adapter::KuboBackend;
use crate::offline_queue::{OfflineQueue, ReplayEngine};
use crate::proxy::ProxyClient;
use crate::types::DaemonStatus;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

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

    /// 连续自动重启计数（Phase D2 自愈防抖：超过上限则停手，健康一段时间后清零）
    pub restart_attempts: Arc<std::sync::atomic::AtomicU32>,

    /// SQLite 缓存
    pub cache: Arc<CacheStore>,
    pub content_index: Arc<crate::content_index::ContentIndex>,

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

    /// 双栈路由器（Phase C）：在 Backend 缝之上按内容/策略选后端
    pub backend_router: Arc<crate::backend_router::BackendRouter>,

    /// 节点身份记录（Phase D1）：人类可读标签 ↔ 节点密码学身份
    pub identity: Arc<crate::identity::IdentityStore>,

    /// 应用启动时刻（Unix 秒，Phase D3 可观测性：计算运行时长）
    pub app_started_at: u64,

    /// 守护进程本次进入 Running 的时刻（Unix 秒；停止/未运行时为 None）
    pub daemon_started_at: Arc<RwLock<Option<u64>>>,
}

/// 当前 Unix 秒
pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl AppState {
    /// 创建应用状态（在 lib.rs::run 中调用）
    pub fn new(config: AppConfig) -> Self {
        let api_client = IpfsApiClient::new(config.api_addr.clone());

        // 初始化缓存数据库
        let cache_dir = dirs::data_local_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap())
            .join("ipfs-desktop-rust");
        let cache = CacheStore::new(cache_dir.join("cache.db")).unwrap_or_else(|e| {
            tracing::warn!("Failed to open cache: {}, using fallback", e);
            // 如果 SQLite 失败（极少数情况），尝试内存缓存
            CacheStore::new(std::env::temp_dir().join("ipfs-cache-fallback.db"))
                .expect("Failed to create fallback cache")
        });

        let key_manager = KeyManager::new();

        // Phase 3: 初始化智能代理
        let cache_arc = Arc::new(cache);
        let content_index = Arc::new(
            crate::content_index::ContentIndex::new(cache_dir.join("content_index.db"))
                .unwrap_or_else(|e| {
                    tracing::warn!("Failed to open content index: {}, using fallback", e);
                    crate::content_index::ContentIndex::new(
                        std::env::temp_dir()
                            .join(format!("ipfs-content-index-{}.db", std::process::id())),
                    )
                    .expect("fallback content index")
                }),
        );
        let proxy_client = ProxyClient::new(config.api_addr.clone(), cache_arc.clone());

        // Phase 3: 初始化离线队列
        let offline_queue =
            OfflineQueue::new(cache_dir.join("offline_queue.db")).unwrap_or_else(|e| {
                tracing::warn!("Failed to open offline queue: {}, using fallback", e);
                OfflineQueue::new(std::env::temp_dir().join("ipfs-offline-queue-fallback.db"))
                    .expect("Failed to create fallback offline queue")
            });

        // Phase 4: 初始化双后端
        let kubo_backend = Arc::new(KuboBackend::new(config.api_addr.clone()));
        let iroh_backend = Arc::new(IrohBackend::new(cache_dir.join("iroh-data")));

        // Phase C: 双栈路由器（默认 KuboOnly，行为等价现有单栈）
        // 来源标记 / provider 持久化到 cache_dir/{cid_origins,cid_providers}.json
        let legacy_policy = crate::backend_router::RoutePolicy::parse(&config.route_policy)
            .unwrap_or(crate::backend_router::RoutePolicy::IrohOnly);
        let usage_mode = config
            .usage_mode
            .as_deref()
            .and_then(crate::backend_router::UsageMode::parse)
            .unwrap_or_else(|| crate::backend_router::UsageMode::from_legacy(legacy_policy));
        let initial_policy = usage_mode.route_policy();
        let backend_router = Arc::new(crate::backend_router::BackendRouter::new_with_policy(
            kubo_backend.clone(),
            iroh_backend.clone(),
            Some(cache_dir.clone()),
            initial_policy,
        ));

        // Phase D1: 节点身份记录（人类可读标签，持久化到 cache_dir/node_identity.json）
        let identity = Arc::new(crate::identity::IdentityStore::new(
            cache_dir.join("node_identity.json"),
        ));

        Self {
            config: Arc::new(RwLock::new(config)),
            daemon_status: Arc::new(RwLock::new(DaemonStatus::default())),
            daemon_controller: Arc::new(RwLock::new(None)),
            api_client: Arc::new(RwLock::new(Some(api_client))),
            proxy_client: Arc::new(RwLock::new(Some(proxy_client))),
            health_monitor: Arc::new(RwLock::new(None)),
            restart_attempts: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            cache: cache_arc,
            content_index,
            key_manager: Arc::new(key_manager),
            dashboard_poller: Arc::new(RwLock::new(None)),
            offline_queue: Arc::new(offline_queue),
            replay_handle: Arc::new(RwLock::new(None)),
            bandwidth_config: Arc::new(RwLock::new(BandwidthConfig::default())),
            bandwidth_monitor: Arc::new(std::sync::Mutex::new(BandwidthMonitor::new())),
            kubo_config: Arc::new(RwLock::new(None)),
            active_backend: Arc::new(RwLock::new(BackendType::Kubo)),
            kubo_backend,
            iroh_backend,
            backend_router,
            identity,
            app_started_at: now_secs(),
            daemon_started_at: Arc::new(RwLock::new(None)),
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
        // 同步重建代理客户端，使其指向新的 API 地址（共享同一份缓存）
        let new_proxy = ProxyClient::new(new_config.api_addr.clone(), self.cache.clone());
        *self.proxy_client.write().await = Some(new_proxy);
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

    /// 重置自愈重启计数（用户手动启动 / 守护进程持续健康后调用）
    pub fn reset_restart_attempts(&self) {
        self.restart_attempts
            .store(0, std::sync::atomic::Ordering::SeqCst);
    }

    /// 仅启动进程并置状态（**不**拉起任何后台循环）。
    ///
    /// 这是把「起进程」与「拉后台循环」拆开的关键——自愈重启只用这一层，
    /// 从而**绝不递归调用 `spawn_health_monitor`**（否则互相递归的 async 无法 Send/定尺寸）。
    async fn start_process(
        &self,
        app_handle: &tauri::AppHandle,
    ) -> Result<(), crate::error::DaemonError> {
        use crate::error::DaemonError;

        let binary_path = match crate::daemon::BinaryFinder::find_with_expected_hash(config_hash(
            &self.get_config().await,
        )) {
            Some(p) => p,
            None => {
                self.set_daemon_status(DaemonStatus::Failed {
                    error: DaemonError::BinaryNotFound.to_string(),
                })
                .await;
                let _ = app_handle.emit("daemon-status-changed", &self.get_daemon_status().await);
                return Err(DaemonError::BinaryNotFound);
            }
        };

        let config = self.get_config().await;
        let repo_path = config.get_ipfs_path();
        let flags = config.daemon_flags.clone();
        let controller = DaemonController::new(binary_path, repo_path);

        match controller.start(flags).await {
            Ok(_) => {
                let pid = controller.get_pid().await.unwrap_or(0);
                // Store ownership immediately. Kubo may need several seconds
                // before its RPC API starts accepting requests.
                self.set_daemon_controller(Some(controller)).await;
                if let Some(api_client) = self.get_api_client().await {
                    let mut last_error = None;
                    for _ in 0..60 {
                        // swarm/peers is a reliable readiness probe on Kubo
                        // versions whose id endpoint may return a proxy 502.
                        if api_client.swarm_peers().await.is_ok() {
                            let peer_id =
                                api_client
                                    .id()
                                    .await
                                    .map(|node| node.id)
                                    .unwrap_or_else(|error| {
                                        tracing::warn!(
                                            "Kubo id unavailable after API became ready: {}",
                                            error
                                        );
                                        "unknown".to_string()
                                    });
                            let status = DaemonStatus::Running {
                                pid,
                                peer_id,
                                api_addr: config.api_addr.clone(),
                            };
                            self.set_daemon_status(status.clone()).await;
                            let _ = app_handle.emit("daemon-status-changed", &status);
                            *self.daemon_started_at.write().await = Some(now_secs());
                            return Ok(());
                        } else {
                            last_error = Some(DaemonError::ApiError(
                                "swarm/peers probe failed".to_string(),
                            ));
                        }
                        tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
                    }
                    tracing::warn!(
                        "Kubo process is alive but RPC API did not become ready: {:?}",
                        last_error
                    );
                }
                let error = DaemonError::ApiConnectionFailed {
                    addr: config.api_addr,
                    detail: "Kubo process started, but its RPC API was not ready within 15 seconds"
                        .to_string(),
                };
                self.set_daemon_status(DaemonStatus::Failed {
                    error: error.to_string(),
                })
                .await;
                let _ = app_handle.emit("daemon-status-changed", &self.get_daemon_status().await);
                Err(error)
            }
            Err(e) => {
                self.set_daemon_status(DaemonStatus::Failed {
                    error: e.to_string(),
                })
                .await;
                let _ = app_handle.emit("daemon-status-changed", &self.get_daemon_status().await);
                Err(e)
            }
        }
    }

    /// 启动守护进程的完整流程（供 `start_daemon` 命令使用）：起进程 + 拉起三个后台循环。
    /// 仅从命令层调用，**不从健康监控里调用**（避免递归）。
    pub async fn start_daemon_core(
        &self,
        app_handle: tauri::AppHandle,
    ) -> Result<(), crate::error::DaemonError> {
        self.start_process(&app_handle).await?;
        self.spawn_health_monitor(app_handle.clone()).await;
        self.spawn_dashboard_poller(app_handle.clone()).await;
        self.spawn_replay_loop(app_handle).await;
        Ok(())
    }

    /// Attach to a Kubo daemon that was started outside this application.
    /// There is deliberately no process controller: Stop means "disconnect"
    /// and must not kill a process owned by another application or terminal.
    pub async fn attach_existing_daemon(
        &self,
        app_handle: tauri::AppHandle,
        peer_id: String,
    ) -> Result<(), crate::error::DaemonError> {
        let config = self.get_config().await;
        let status = DaemonStatus::Running {
            pid: 0,
            peer_id,
            api_addr: config.api_addr,
        };
        self.set_daemon_controller(None).await;
        self.set_daemon_status(status.clone()).await;
        *self.daemon_started_at.write().await = Some(now_secs());
        let _ = app_handle.emit("daemon-status-changed", &status);
        self.spawn_dashboard_poller(app_handle.clone()).await;
        self.spawn_replay_loop(app_handle).await;
        tracing::info!("Attached to an existing Kubo daemon");
        Ok(())
    }

    /// 启动健康监控后台任务（Phase D2：探测意外死亡 → 自愈重启，带退避与上限）
    pub async fn spawn_health_monitor(&self, app_handle: tauri::AppHandle) {
        self.cancel_health_monitor().await;

        /// 连续自动重启上限（超过则停手，避免崩溃循环）
        const MAX_AUTO_RESTARTS: u32 = 5;
        /// 持续健康多少个检查周期后清零重启预算（5s × 6 = 30s）
        const HEALTHY_CHECKS_TO_RESET: u32 = 6;

        let state = self.clone();
        let handle = tokio::spawn(async move {
            use std::sync::atomic::Ordering;
            tracing::info!("Health monitor started");
            let mut healthy_checks: u32 = 0;
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

                let controller_opt = state.get_daemon_controller().await;
                match controller_opt {
                    None => {
                        tracing::info!("Health monitor: controller removed, stopping");
                        break;
                    }
                    Some(controller) => {
                        if controller.is_running().await {
                            // 持续健康达阈值 → 清零自愈预算（把「偶发崩溃」与「崩溃循环」区分开）
                            healthy_checks += 1;
                            if healthy_checks >= HEALTHY_CHECKS_TO_RESET
                                && state.restart_attempts.load(Ordering::SeqCst) > 0
                            {
                                state.reset_restart_attempts();
                                tracing::info!("Daemon healthy — auto-restart budget reset");
                            }
                            continue;
                        }

                        // ── 检测到意外死亡 ──
                        tracing::error!("Health monitor: daemon process died unexpectedly!");
                        let auto_restart = state.get_config().await.auto_restart;

                        if auto_restart {
                            let n = state.restart_attempts.fetch_add(1, Ordering::SeqCst) + 1;
                            if n <= MAX_AUTO_RESTARTS {
                                tracing::warn!(
                                    "Auto-restarting daemon (attempt {}/{})",
                                    n,
                                    MAX_AUTO_RESTARTS
                                );
                                state.set_daemon_controller(None).await;
                                state.set_daemon_status(DaemonStatus::Starting).await;
                                let _ = app_handle
                                    .emit("daemon-status-changed", &DaemonStatus::Starting);
                                // 线性退避，给系统喘息
                                tokio::time::sleep(tokio::time::Duration::from_secs(2 * n as u64))
                                    .await;
                                // 只起进程（不重新 spawn 健康监控——本任务继续看护），避免递归 async。
                                match state.start_process(&app_handle).await {
                                    Ok(_) => {
                                        // 轮询/重放随死亡停了，这里重新拉起；健康监控由本任务继续
                                        state.spawn_dashboard_poller(app_handle.clone()).await;
                                        state.spawn_replay_loop(app_handle.clone()).await;
                                        healthy_checks = 0;
                                        tracing::info!("Daemon auto-restarted; monitor continues");
                                        continue;
                                    }
                                    Err(e) => {
                                        tracing::error!("Auto-restart failed: {}", e);
                                        break;
                                    }
                                }
                            }
                            tracing::error!(
                                "Auto-restart cap ({}) reached — giving up",
                                MAX_AUTO_RESTARTS
                            );
                        }

                        // 不自愈 / 已达上限 → 标记失败
                        let error_msg =
                            "Daemon process exited unexpectedly (detected by health monitor)"
                                .to_string();
                        state
                            .set_daemon_status(DaemonStatus::Failed {
                                error: error_msg.clone(),
                            })
                            .await;
                        state.set_daemon_controller(None).await;
                        if let Err(e) = app_handle.emit(
                            "daemon-status-changed",
                            &DaemonStatus::Failed { error: error_msg },
                        ) {
                            tracing::warn!(
                                "Failed to emit daemon-status-changed from health monitor: {}",
                                e
                            );
                        }
                        break;
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
                            Err(e) => {
                                tracing::warn!("Poller peers: {}", e);
                                None
                            }
                        }
                    };

                    let bw_fut = async {
                        match client.stats_bw().await {
                            Ok(b) => {
                                let json = serde_json::to_string(&b).unwrap_or_default();
                                state.cache.set_bandwidth(&json);
                                // 喂给带宽监控器计算平滑速率（供 get_bandwidth_status 使用）
                                if let Ok(mut mon) = state.bandwidth_monitor.lock() {
                                    mon.sample(b.total_in, b.total_out);
                                }
                                Some(b)
                            }
                            Err(e) => {
                                tracing::warn!("Poller bandwidth: {}", e);
                                None
                            }
                        }
                    };

                    let bs_fut = async {
                        match client.bitswap_stat().await {
                            Ok(b) => {
                                let json = serde_json::to_string(&b).unwrap_or_default();
                                state.cache.set_bitswap(&json);
                                Some(b)
                            }
                            Err(e) => {
                                tracing::warn!("Poller bitswap: {}", e);
                                None
                            }
                        }
                    };

                    let repo_fut = async {
                        match client.repo_stat().await {
                            Ok(r) => {
                                let json = serde_json::to_string(&r).unwrap_or_default();
                                state.cache.set_repo_stats(&json);
                                Some(r)
                            }
                            Err(e) => {
                                tracing::warn!("Poller repo: {}", e);
                                None
                            }
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

                    let tick = DashboardTick {
                        peers,
                        bandwidth: bw,
                        bitswap: bs,
                        repo,
                    };
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
                        if let Err(e) = app_handle.emit(
                            "replay-progress",
                            &serde_json::json!({
                                "success": success,
                                "failed": failed,
                                "remaining": queue.len().unwrap_or(0),
                            }),
                        ) {
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

fn config_hash(config: &AppConfig) -> Option<String> {
    config.kubo_binary_sha256.clone()
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
        assert!(matches!(
            state.get_daemon_status().await,
            DaemonStatus::Starting
        ));

        state
            .set_daemon_status(DaemonStatus::Running {
                pid: 1234,
                peer_id: "test".into(),
                api_addr: "http://localhost:5001".into(),
            })
            .await;
        assert!(matches!(
            state.get_daemon_status().await,
            DaemonStatus::Running { .. }
        ));

        state.set_daemon_status(DaemonStatus::Stopped).await;
        assert!(matches!(
            state.get_daemon_status().await,
            DaemonStatus::Stopped
        ));
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
        let new_config = AppConfig {
            api_addr: "http://127.0.0.1:6001".to_string(),
            ..AppConfig::default()
        };

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
        let dir = tempfile::tempdir().unwrap();
        let cache = crate::cache::CacheStore::new(dir.path().join("cache.db")).unwrap();
        // 缓存应该可用（写入并读取）
        cache.set_dashboard(r#"{"test":1}"#);
        assert!(cache.get_dashboard().is_some());
    }

    #[tokio::test]
    async fn test_key_manager_present() {
        // 使用临时目录，避免污染真实用户密钥目录
        let dir = std::env::temp_dir().join(format!("ipfs-state-keytest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mgr = crate::keyring::KeyManager::with_dir(dir);
        let rec = crate::keyring::KeyRecord::from_kubo("state-test", "k51state");
        mgr.save_record(&rec).unwrap();
        assert!(!mgr.load_record("state-test").unwrap().public_key.is_empty());
        mgr.delete_record("state-test").unwrap();
    }
}
