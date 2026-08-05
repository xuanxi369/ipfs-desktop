use crate::backend_trait::{Backend, BackendType};
use crate::config::AppConfig;
use crate::daemon::{
    BandwidthStats, BitswapStats, IpfsApiClient, NodeId, PinAddResult, PinList, PinRmResult,
    RepoStats, SwarmPeers,
};
use crate::error::DaemonError;
use crate::state::AppState;
use crate::types::DaemonStatus;
use serde::Serialize;
use tauri::Emitter;
use tauri::State;

/// 获取守护进程状态
#[tauri::command]
pub async fn get_daemon_status(state: State<'_, AppState>) -> Result<DaemonStatus, DaemonError> {
    Ok(state.get_daemon_status().await)
}

/// 启动守护进程
#[tauri::command]
pub async fn start_daemon(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), DaemonError> {
    let current_status = state.get_daemon_status().await;

    // 检查当前状态
    if !matches!(
        current_status,
        DaemonStatus::Stopped | DaemonStatus::Failed { .. }
    ) {
        return Err(DaemonError::InvalidState);
    }

    // 手动启动 → 重置自愈重启计数（用户主动操作，给一份新的自愈预算）
    state.reset_restart_attempts();

    // Kubo may already have been started by a terminal, launch agent, or the
    // original IPFS Desktop. Starting a second process would fail on repo.lock
    // and occupied ports, so attach to the healthy API instead.
    if let Some(api_client) = state.get_api_client().await {
        if api_client.swarm_peers().await.is_ok() {
            let peer_id = api_client
                .id()
                .await
                .map(|node| node.id)
                .unwrap_or_else(|_| "unknown".to_string());
            tracing::info!("Kubo API already available; attaching instead of spawning");
            return state.attach_existing_daemon(app_handle, peer_id).await;
        }
    }

    // 更新状态为启动中
    state.set_daemon_status(DaemonStatus::Starting).await;
    app_handle
        .emit("daemon-status-changed", &DaemonStatus::Starting)
        .map_err(|e| DaemonError::ConfigError(e.to_string()))?;

    tracing::info!("Starting IPFS daemon...");

    // 核心启动流程（与自愈重启共用同一实现）
    state.start_daemon_core(app_handle).await
}

/// 停止守护进程
#[tauri::command]
pub async fn stop_daemon(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), DaemonError> {
    let current_status = state.get_daemon_status().await;

    // 检查当前状态
    if matches!(current_status, DaemonStatus::Stopped) {
        return Ok(());
    }

    // 先取消健康监控
    state.cancel_health_monitor().await;

    // Phase 2: 取消仪表盘轮询
    state.cancel_dashboard_poller().await;

    // 更新状态为停止中
    state.set_daemon_status(DaemonStatus::Stopping).await;

    // 发送事件到前端
    app_handle
        .emit("daemon-status-changed", &DaemonStatus::Stopping)
        .map_err(|e| DaemonError::ConfigError(e.to_string()))?;

    tracing::info!("Stopping IPFS daemon...");

    // 获取控制器
    if let Some(controller) = state.get_daemon_controller().await {
        match controller.stop().await {
            Ok(_) => {
                tracing::info!("Daemon stopped successfully");
                state.set_daemon_status(DaemonStatus::Stopped).await;
                state.set_daemon_controller(None).await;

                app_handle
                    .emit("daemon-status-changed", &DaemonStatus::Stopped)
                    .map_err(|e| DaemonError::ConfigError(e.to_string()))?;

                Ok(())
            }
            Err(e) => {
                let err_str = e.to_string();
                tracing::error!("Failed to stop daemon: {}", err_str);
                state
                    .set_daemon_status(DaemonStatus::Failed {
                        error: err_str.clone(),
                    })
                    .await;
                app_handle
                    .emit("daemon-status-changed", &state.get_daemon_status().await)
                    .map_err(|e| DaemonError::ConfigError(e.to_string()))?;
                Err(e)
            }
        }
    } else {
        // 没有控制器，直接设置为已停止
        state.set_daemon_status(DaemonStatus::Stopped).await;
        app_handle
            .emit("daemon-status-changed", &DaemonStatus::Stopped)
            .map_err(|e| DaemonError::ConfigError(e.to_string()))?;
        Ok(())
    }
}

/// 重启守护进程
#[tauri::command]
pub async fn restart_daemon(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), DaemonError> {
    tracing::info!("Restarting IPFS daemon...");

    // 如果有控制器，使用控制器的 restart 方法（带轮询）
    if let Some(controller) = state.get_daemon_controller().await {
        let config = state.get_config().await;
        // 重启前取消健康监控，restart 内部会重新 start
        state.cancel_health_monitor().await;
        match controller.restart(config.daemon_flags.clone()).await {
            Ok(_) => {
                // 重启成功后重新启动健康监控
                state.spawn_health_monitor(app_handle.clone()).await;
                // 重启成功后更新状态
                if let Some(api_client) = state.get_api_client().await {
                    if let Ok(node_id) = api_client.id().await {
                        let pid = controller.get_pid().await.unwrap_or(0);
                        let status = DaemonStatus::Running {
                            pid,
                            peer_id: node_id.id.clone(),
                            api_addr: config.api_addr.clone(),
                        };
                        state.set_daemon_status(status.clone()).await;
                        app_handle
                            .emit("daemon-status-changed", &status)
                            .map_err(|e| DaemonError::ConfigError(e.to_string()))?;
                        return Ok(());
                    }
                }
                state
                    .set_daemon_status(DaemonStatus::Running {
                        pid: controller.get_pid().await.unwrap_or(0),
                        peer_id: "unknown".to_string(),
                        api_addr: config.api_addr.clone(),
                    })
                    .await;
                let current = state.get_daemon_status().await;
                app_handle
                    .emit("daemon-status-changed", &current)
                    .map_err(|e| DaemonError::ConfigError(e.to_string()))?;
                Ok(())
            }
            Err(e) => {
                let err_str = e.to_string();
                let failed = DaemonStatus::Failed {
                    error: err_str.clone(),
                };
                state.set_daemon_status(failed.clone()).await;
                app_handle
                    .emit("daemon-status-changed", &failed)
                    .map_err(|e| DaemonError::ConfigError(e.to_string()))?;
                Err(e)
            }
        }
    } else {
        // 没有控制器，走 停止+启动 路径
        if let Err(e) = stop_daemon(state.clone(), app_handle.clone()).await {
            tracing::warn!("Stop daemon failed during restart: {}", e);
        }
        // 轮询确认停止（最多 10 秒）
        for i in 0..20 {
            let status = state.get_daemon_status().await;
            if matches!(status, DaemonStatus::Stopped | DaemonStatus::Failed { .. }) {
                tracing::info!("Daemon confirmed stopped after {} ms", (i + 1) * 500);
                break;
            }
            if i == 19 {
                tracing::warn!("Daemon still not stopped after 10s, proceeding anyway");
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
        start_daemon(state, app_handle).await
    }
}

/// 获取配置
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, DaemonError> {
    Ok(state.get_config().await)
}

/// 更新配置
#[tauri::command]
pub async fn update_config(
    state: State<'_, AppState>,
    new_config: AppConfig,
) -> Result<(), DaemonError> {
    // 验证配置
    new_config.validate().map_err(DaemonError::ConfigError)?;

    // 保存配置到磁盘
    new_config.save().map_err(DaemonError::ConfigError)?;

    // 更新状态
    state.update_config(new_config.clone()).await;

    tracing::info!("Configuration updated and saved");

    Ok(())
}

/// 获取节点 ID（用于测试 API 连接）
#[tauri::command]
pub async fn get_node_id(state: State<'_, AppState>) -> Result<String, DaemonError> {
    tracing::info!("Getting node ID...");

    // 优先走智能代理（缓存 + 熔断 + 统计）
    if let Some(proxy) = state.get_proxy_client().await {
        if let Ok(node) = proxy.get_node_id().await {
            return Ok(node.id);
        }
    }

    // 代理不可用时回退到原始 API 客户端
    if let Some(api_client) = state.get_api_client().await {
        match api_client.id().await {
            Ok(node_id) => {
                tracing::info!("Node ID: {}", node_id.id);
                Ok(node_id.id)
            }
            Err(e) => {
                tracing::error!("Failed to get node ID: {}", e);
                Err(e)
            }
        }
    } else {
        Err(DaemonError::ApiError(
            "API client not initialized".to_string(),
        ))
    }
}

/// 打开 IPFS WebUI
///
/// 在系统默认浏览器中打开 IPFS 自带的 WebUI。
/// 需要守护进程处于运行状态。
#[tauri::command]
pub async fn open_webui(state: State<'_, AppState>) -> Result<(), DaemonError> {
    let status = state.get_daemon_status().await;
    let config = state.get_config().await;

    let webui_url = format!("{}/webui", config.api_addr);

    match status {
        DaemonStatus::Running { .. } => {
            tracing::info!("Opening WebUI at: {}", webui_url);
            open::that(&webui_url)
                .map_err(|e| DaemonError::ConfigError(format!("Failed to open browser: {}", e)))?;
            Ok(())
        }
        _ => Err(DaemonError::InvalidState),
    }
}

/// 获取 WebUI URL
#[tauri::command]
pub async fn get_webui_url(state: State<'_, AppState>) -> Result<String, DaemonError> {
    let config = state.get_config().await;
    Ok(format!("{}/webui", config.api_addr))
}

/// 添加文件到 IPFS
///
/// 接受本地文件路径，上传到 IPFS 并返回哈希。
#[tauri::command]
pub async fn add_file(
    state: State<'_, AppState>,
    file_path: String,
    prefer: Option<String>,
) -> Result<crate::types::AddResult, DaemonError> {
    let path = std::path::PathBuf::from(&file_path);
    if !path.exists() {
        return Err(DaemonError::IoError(format!(
            "File not found: {}",
            file_path
        )));
    }

    // 写侧策略：显式偏好（"iroh"/"kubo"）优先——「本地/信任圈内容优先 iroh」由此表达；
    // 无偏好时按策略（Auto 默认落 Kubo 以保证公网可寻址）。省略参数即旧行为（零回归）。
    let prefer_backend = match prefer.as_deref() {
        Some("iroh") | Some("Iroh") => Some(BackendType::Iroh),
        Some("kubo") | Some("Kubo") => Some(BackendType::Kubo),
        _ => None,
    };
    match state.backend_router.choose_for_add(prefer_backend).await {
        BackendType::Kubo => {
            // 保留原有 Kubo 路径：用实时 api_client（尊重运行时改地址）
            let client = state.get_api_client().await.ok_or(DaemonError::ApiError(
                "API client not initialized".to_string(),
            ))?;
            let res = client.add_file(&path).await?;
            state
                .backend_router
                .record_origin(&res.hash, BackendType::Kubo)
                .await;
            Ok(res)
        }
        BackendType::Iroh => {
            let out = state
                .iroh_backend
                .add_file(&path)
                .await
                .map_err(|e| DaemonError::ApiError(e.to_string()))?;
            state
                .backend_router
                .record_origin(&out.cid, BackendType::Iroh)
                .await;
            // 转为统一的 AddResult 形态，前端不感知后端差异
            Ok(crate::types::AddResult {
                hash: out.cid,
                size: out.size.to_string(),
                name: out.name,
            })
        }
    }
}

/// 批量添加文件到 IPFS
#[tauri::command]
pub async fn add_files(
    state: State<'_, AppState>,
    file_paths: Vec<String>,
) -> Result<Vec<crate::types::AddResult>, DaemonError> {
    validate_path_count(&file_paths)?;
    let mut results = Vec::new();
    for file_path in &file_paths {
        results.push(add_file(state.clone(), file_path.clone(), None).await?);
    }
    Ok(results)
}

/// 设置开机自启
#[tauri::command]
pub async fn set_auto_launch(state: State<'_, AppState>, enable: bool) -> Result<(), DaemonError> {
    let app_path = std::env::current_exe()
        .map_err(|e| DaemonError::IoError(format!("Failed to get app path: {}", e)))?;

    let app_name = "ipfs-desktop-rust";

    let auto = auto_launch::AutoLaunchBuilder::new()
        .set_app_name(app_name)
        .set_app_path(&app_path.to_string_lossy())
        .set_macos_launch_mode(auto_launch::MacOSLaunchMode::LaunchAgent)
        .build()
        .map_err(|e| DaemonError::ConfigError(format!("Failed to build auto-launch: {}", e)))?;

    if enable {
        auto.enable().map_err(|e| {
            DaemonError::ConfigError(format!("Failed to enable auto-launch: {}", e))
        })?;
        tracing::info!("Auto-launch enabled");
    } else {
        auto.disable().map_err(|e| {
            DaemonError::ConfigError(format!("Failed to disable auto-launch: {}", e))
        })?;
        tracing::info!("Auto-launch disabled");
    }

    // 更新配置
    let mut config = state.get_config().await;
    config.auto_launch = enable;
    config.save().map_err(DaemonError::ConfigError)?;
    state.update_config(config).await;

    Ok(())
}

/// 获取开机自启状态
#[tauri::command]
pub async fn get_auto_launch(state: State<'_, AppState>) -> Result<bool, DaemonError> {
    Ok(state.get_config().await.auto_launch)
}

// ════════════════════════════════════════════════════════════════
// A1: 下载功能 — cat / get
// ════════════════════════════════════════════════════════════════

/// 从 IPFS 读取文件内容（通过 CID）
///
/// 返回文件的原始字节内容。对于文本文件，前端会转换为字符串显示。
#[tauri::command]
pub async fn cat_file(state: State<'_, AppState>, cid: String) -> Result<Vec<u8>, DaemonError> {
    tracing::info!("cat_file: {}", cid);

    // 双栈韧性：按路由顺序尝试（Auto 下主后端取不到 → 自动 fallback 到另一个）。
    // Kubo 腿仍用实时 api_client（尊重运行时改地址）。
    let order = state.backend_router.cat_order(&cid).await;
    let mut first_err: Option<DaemonError> = None;
    for (i, backend) in order.iter().enumerate() {
        match cat_via(&state, *backend, &cid).await {
            Ok(bytes) => {
                if i > 0 {
                    // fallback 命中 → 回填来源标记，下次直达（自愈）
                    state.backend_router.record_origin(&cid, *backend).await;
                    tracing::info!("cat_file fallback hit on {:?} for {}", backend, cid);
                }
                return Ok(bytes);
            }
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
    }
    // 本地两栈都 miss → 网络 fallback（已知 iroh provider 则跨节点取回）
    if let Some(Ok(bytes)) = state.backend_router.try_network_fetch(&cid).await {
        tracing::info!("cat_file network-fallback hit for {}", cid);
        return Ok(bytes);
    }
    Err(first_err
        .unwrap_or_else(|| DaemonError::ApiError("no backend produced content".to_string())))
}

/// 用指定后端读取内容（Kubo 腿用实时 api_client，尊重运行时改地址）
async fn cat_via(
    state: &AppState,
    backend: BackendType,
    cid: &str,
) -> Result<Vec<u8>, DaemonError> {
    match backend {
        BackendType::Kubo => {
            let client = state
                .get_api_client()
                .await
                .ok_or_else(|| DaemonError::ApiError("API client not initialized".to_string()))?;
            client.cat(cid).await
        }
        BackendType::Iroh => state
            .iroh_backend
            .cat(cid)
            .await
            .map_err(|e| DaemonError::ApiError(e.to_string())),
    }
}

/// 流式下载文件（带进度事件）
///
/// 每读取一个 chunk 发送 `download-progress` 事件到前端。
#[tauri::command]
pub async fn download_file(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    cid: String,
    save_path: String,
) -> Result<(), DaemonError> {
    tracing::info!("download_file: {} -> {}", cid, save_path);

    let output = std::path::PathBuf::from(&save_path);
    validate_cid(&cid)?;
    validate_output_path(&output)?;
    let backend = state.backend_router.choose_for_cid(&cid).await;
    let written = match backend {
        BackendType::Kubo => {
            let client = state.get_api_client().await.ok_or(DaemonError::ApiError(
                "API client not initialized".to_string(),
            ))?;
            // HTTP chunked responses often omit Content-Length. Query Kubo's
            // stat endpoint first so the UI can report a real percentage.
            let total_hint = client.file_size(&cid).await.ok();
            client
                .cat_to_file(&cid, &output, |loaded, total| {
                    let _ = app_handle.emit(
                        "download-progress",
                        &DownloadProgress {
                            cid: cid.clone(),
                            loaded,
                            total: total.or(total_hint),
                        },
                    );
                })
                .await?
        }
        BackendType::Iroh => {
            state
                .iroh_backend
                .export_to_file(&cid, &output)
                .await
                .map_err(iroh_err)?;
            let size = tokio::fs::metadata(&output)
                .await
                .map_err(|e| DaemonError::IoError(e.to_string()))?
                .len();
            let _ = app_handle.emit(
                "download-progress",
                &DownloadProgress {
                    cid: cid.clone(),
                    loaded: size,
                    total: Some(size),
                },
            );
            size
        }
    };

    tracing::info!("Download complete: {} bytes -> {:?}", written, output);
    Ok(())
}

/// 下载进度事件载荷
#[derive(Debug, Clone, Serialize)]
struct DownloadProgress {
    cid: String,
    loaded: u64,
    total: Option<u64>,
}

/// 上传进度事件载荷
#[derive(Debug, Clone, Serialize)]
struct UploadProgress {
    name: String,
    loaded: u64,
    total: u64,
}

/// 根据 CID 获取文件大小
#[tauri::command]
pub async fn get_file_size(state: State<'_, AppState>, cid: String) -> Result<u64, DaemonError> {
    validate_cid(&cid)?;
    match state.backend_router.choose_for_cid(&cid).await {
        BackendType::Kubo => {
            state
                .get_api_client()
                .await
                .ok_or(DaemonError::ApiError(
                    "API client not initialized".to_string(),
                ))?
                .file_size(&cid)
                .await
        }
        BackendType::Iroh => Ok(state.iroh_backend.cat(&cid).await.map_err(iroh_err)?.len() as u64),
    }
}

// ════════════════════════════════════════════════════════════════
// A2: Pin 管理
// ════════════════════════════════════════════════════════════════

/// 获取 Pin 列表
#[tauri::command]
pub async fn get_pin_list(state: State<'_, AppState>) -> Result<PinList, DaemonError> {
    tracing::info!("get_pin_list");

    // 优先走智能代理（缓存 + 熔断 + 统计）
    if let Some(proxy) = state.get_proxy_client().await {
        if let Ok(pins) = proxy.get_pin_list().await {
            return Ok(pins);
        }
    }

    let client = state.get_api_client().await.ok_or(DaemonError::ApiError(
        "API client not initialized".to_string(),
    ))?;

    client.pin_ls().await
}

/// 添加 Pin
#[tauri::command]
pub async fn add_pin(state: State<'_, AppState>, cid: String) -> Result<PinAddResult, DaemonError> {
    tracing::info!("add_pin: {}", cid);

    // 写命令同样经代理（统一熔断 + 指标）；代理不可用时回退到原始客户端
    let result = if let Some(proxy) = state.get_proxy_client().await {
        let cid_owned = cid.clone();
        proxy
            .raw_call(|api| async move { api.pin_add(&cid_owned).await })
            .await
            .map_err(DaemonError::ApiError)?
    } else {
        let client = state.get_api_client().await.ok_or(DaemonError::ApiError(
            "API client not initialized".to_string(),
        ))?;
        client.pin_add(&cid).await?
    };
    // 写操作后让 pin 缓存失效，保证下次读取拿到最新列表
    state.cache.invalidate("pins");
    Ok(result)
}

/// 移除 Pin
#[tauri::command]
pub async fn remove_pin(
    state: State<'_, AppState>,
    cid: String,
) -> Result<PinRmResult, DaemonError> {
    tracing::info!("remove_pin: {}", cid);

    // 写命令同样经代理（统一熔断 + 指标）；代理不可用时回退到原始客户端
    let result = if let Some(proxy) = state.get_proxy_client().await {
        let cid_owned = cid.clone();
        proxy
            .raw_call(|api| async move { api.pin_rm(&cid_owned).await })
            .await
            .map_err(DaemonError::ApiError)?
    } else {
        let client = state.get_api_client().await.ok_or(DaemonError::ApiError(
            "API client not initialized".to_string(),
        ))?;
        client.pin_rm(&cid).await?
    };
    // 写操作后让 pin 缓存失效，保证下次读取拿到最新列表
    state.cache.invalidate("pins");
    Ok(result)
}

// ════════════════════════════════════════════════════════════════
// A3: 节点状态仪表盘
// ════════════════════════════════════════════════════════════════

/// 仪表盘聚合数据
#[derive(Debug, Clone, Serialize)]
pub struct DashboardStats {
    /// 节点 ID
    pub node_id: Option<NodeId>,
    /// 版本信息
    pub version: Option<String>,
    /// 仓库统计
    pub repo: Option<RepoStats>,
    /// 对等节点列表
    pub peers: Option<SwarmPeers>,
    /// 带宽统计
    pub bandwidth: Option<BandwidthStats>,
    /// Bitswap 统计
    pub bitswap: Option<BitswapStats>,
    /// Pin 数量
    pub pin_count: usize,
}

/// 获取仪表盘聚合数据
///
/// 一次性获取所有节点状态信息，前端用于渲染仪表盘。
/// 单个 API 调用失败不影响其他数据。
#[tauri::command]
pub async fn get_dashboard_stats(
    state: State<'_, AppState>,
) -> Result<DashboardStats, DaemonError> {
    tracing::info!("get_dashboard_stats");

    let client = state.get_api_client().await.ok_or(DaemonError::ApiError(
        "API client not initialized".to_string(),
    ))?;

    // 真正并行获取所有数据（单个失败不影响其他）
    let (node_id, version, repo, peers, bandwidth, bitswap, pin_ls) = tokio::join!(
        client.id(),
        client.version(),
        client.repo_stat(),
        client.swarm_peers(),
        client.stats_bw(),
        client.bitswap_stat(),
        client.pin_ls(),
    );

    let node_id = node_id
        .map_err(|e| tracing::warn!("Failed to get node id: {}", e))
        .ok();
    let version = version
        .map(|v| v.version)
        .map_err(|e| tracing::warn!("Failed to get version: {}", e))
        .ok();
    let repo = repo
        .map_err(|e| tracing::warn!("Failed to get repo stats: {}", e))
        .ok();
    let peers = peers
        .map_err(|e| tracing::warn!("Failed to get peers: {}", e))
        .ok();
    let bandwidth = bandwidth
        .map_err(|e| tracing::warn!("Failed to get bandwidth: {}", e))
        .ok();
    let bitswap = bitswap
        .map_err(|e| tracing::warn!("Failed to get bitswap stats: {}", e))
        .ok();
    let pin_count = match pin_ls {
        Ok(pins) => pins.pins.len(),
        Err(e) => {
            tracing::warn!("Failed to get pin count: {}", e);
            0
        }
    };

    Ok(DashboardStats {
        node_id,
        version,
        repo,
        peers,
        bandwidth,
        bitswap,
        pin_count,
    })
}

/// Locate the node's currently connected public peers. Coordinates are
/// approximate GeoIP results and should not be treated as physical locations.
#[tauri::command]
pub async fn get_peer_geography(
    state: State<'_, AppState>,
) -> Result<crate::peer_geo::PeerGeoReport, DaemonError> {
    let client = state.get_api_client().await.ok_or(DaemonError::ApiError(
        "API client not initialized".to_string(),
    ))?;
    crate::peer_geo::locate_connected_peers(&client).await
}

/// 添加文件到 IPFS（带进度事件）
///
/// 在开始上传前检测文件大小，并发送 upload-progress 事件。
#[tauri::command]
pub async fn add_file_with_progress(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    file_path: String,
) -> Result<crate::types::AddResult, DaemonError> {
    let path = std::path::PathBuf::from(&file_path);
    if !path.exists() {
        return Err(DaemonError::IoError(format!(
            "File not found: {}",
            file_path
        )));
    }

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed")
        .to_string();

    let client = state.get_api_client().await.ok_or(DaemonError::ApiError(
        "API client not initialized".to_string(),
    ))?;

    // 发送开始事件
    let _ = app_handle.emit(
        "upload-progress",
        &UploadProgress {
            name: file_name.clone(),
            loaded: 0,
            total: 0,
        },
    );

    // 真实分块进度：回调按字节累加，节流至每 512KB 或完成时推送一次
    let app = app_handle.clone();
    let name_cb = file_name.clone();
    let last_emit = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let result = client
        .add_file_streaming(&path, move |sent, total| {
            use std::sync::atomic::Ordering;
            let prev = last_emit.load(Ordering::Relaxed);
            if sent == total || sent.saturating_sub(prev) >= 512 * 1024 {
                last_emit.store(sent, Ordering::Relaxed);
                let _ = app.emit(
                    "upload-progress",
                    &UploadProgress {
                        name: name_cb.clone(),
                        loaded: sent,
                        total,
                    },
                );
            }
        })
        .await?;

    tracing::info!("Upload complete: {} -> {}", file_path, result.hash);
    state
        .content_index
        .upsert(&crate::content_index::ContentRecord {
            cid: result.hash.clone(),
            name: result.name.clone(),
            size: result.size.parse().unwrap_or(0),
            backend: "Kubo".to_string(),
            added_at: crate::state::now_secs() as i64,
        })
        .map_err(DaemonError::IoError)?;
    Ok(result)
}

#[tauri::command]
pub async fn list_content(
    state: State<'_, AppState>,
) -> Result<Vec<crate::content_index::ContentRecord>, DaemonError> {
    state.content_index.list().map_err(DaemonError::IoError)
}

/// 仅删除本应用的列表记录；不会删除、unpin 或 GC 实际内容。
#[tauri::command]
pub async fn remove_content_record(
    state: State<'_, AppState>,
    cid: String,
) -> Result<(), DaemonError> {
    if cid.trim().is_empty() || cid.len() > 256 {
        return Err(DaemonError::ApiError("invalid CID/hash format".to_string()));
    }
    state
        .content_index
        .remove(&cid)
        .map_err(DaemonError::IoError)
}

// ════════════════════════════════════════════════════════════════
// Phase 2: IPNS 发布/解析 + 密钥管理
// ════════════════════════════════════════════════════════════════

/// 生成新的 IPNS 密钥（由 Kubo 在其密钥库中生成并保管私钥）
///
/// 私钥全程由 Kubo 管理，本应用只保存一份「标签 → 真实 IPNS 名称」的公开记录，
/// 不接触任何私钥。需要守护进程处于运行状态。
#[tauri::command]
pub async fn generate_key(
    state: State<'_, AppState>,
    label: String,
) -> Result<crate::keyring::KeyRecord, DaemonError> {
    tracing::info!("generate_key: {}", label);

    let client = state.get_api_client().await.ok_or(DaemonError::ApiError(
        "API client not initialized (start the daemon first)".to_string(),
    ))?;

    // 由 Kubo 生成密钥，返回真实的 IPNS 名称（Id）
    let kg = client.key_gen(&label).await?;
    let record = crate::keyring::KeyRecord::from_kubo(kg.name, kg.id);

    // 保存公开记录（便于离线展示）
    state
        .key_manager
        .save_record(&record)
        .map_err(DaemonError::ConfigError)?;

    Ok(record)
}

/// 列出所有密钥（以 Kubo 的密钥库为权威来源，守护进程不可用时回退到本地记录）
#[tauri::command]
pub async fn list_keys(
    state: State<'_, AppState>,
) -> Result<Vec<crate::keyring::KeyRecord>, DaemonError> {
    // 优先查询 Kubo 的权威列表并同步本地记录
    if let Some(client) = state.get_api_client().await {
        if let Ok(kl) = client.key_list().await {
            let records: Vec<crate::keyring::KeyRecord> = kl
                .keys
                .into_iter()
                .map(|k| crate::keyring::KeyRecord::from_kubo(k.name, k.id))
                .collect();
            state.key_manager.sync_from_kubo(&records);
            return Ok(records);
        }
    }

    // 守护进程不可用 → 回退到本地记录
    state
        .key_manager
        .list_records()
        .map_err(DaemonError::ConfigError)
}

/// 删除密钥（同时从 Kubo 密钥库和本地记录移除）
#[tauri::command]
pub async fn delete_key(state: State<'_, AppState>, label: String) -> Result<(), DaemonError> {
    // 先删 Kubo 侧（"self" 等内置密钥可能失败，记录但不阻断本地清理）
    if let Some(client) = state.get_api_client().await {
        if let Err(e) = client.key_rm(&label).await {
            tracing::warn!("Kubo key/rm failed for '{}': {}", label, e);
        }
    }
    state
        .key_manager
        .delete_record(&label)
        .map_err(DaemonError::ConfigError)
}

/// IPNS 发布：将 CID 绑定到密钥名称
#[tauri::command]
pub async fn ipns_publish(
    state: State<'_, AppState>,
    cid: String,
    key_name: String,
    lifetime: Option<String>,
    ipns_base: Option<String>,
    allow_offline: Option<bool>,
) -> Result<crate::daemon::IpnsPublishResult, DaemonError> {
    let lifetime = lifetime.unwrap_or_else(|| "24h".to_string());
    let allow_offline = allow_offline.unwrap_or(false);

    tracing::info!(
        "ipns_publish: {} -> {} (lifetime: {}, offline: {})",
        cid,
        key_name,
        lifetime,
        allow_offline
    );

    let client = state.get_api_client().await.ok_or(DaemonError::ApiError(
        "API client not initialized".to_string(),
    ))?;

    client
        .name_publish_full(
            &cid,
            &key_name,
            &lifetime,
            ipns_base.as_deref(),
            allow_offline,
        )
        .await
}

/// IPNS 解析：将 IPNS 名称解析为 CID
#[tauri::command]
pub async fn ipns_resolve(
    state: State<'_, AppState>,
    name: String,
) -> Result<crate::daemon::IpnsResolveResult, DaemonError> {
    tracing::info!("ipns_resolve: {}", name);

    let client = state.get_api_client().await.ok_or(DaemonError::ApiError(
        "API client not initialized".to_string(),
    ))?;

    client.name_resolve(&name).await
}

/// 从缓存获取仪表盘数据（毫秒级响应）
///
/// 优先返回缓存，缓存未命中则穿透 API 并回填缓存。
#[tauri::command]
pub async fn get_cached_dashboard(
    state: State<'_, AppState>,
) -> Result<DashboardStats, DaemonError> {
    // 通过智能代理组装：每个字段都先查缓存，未命中才穿透 API 并回填，
    // 同时统一记录缓存命中率 / 延迟 / 熔断等指标（供代理统计面板展示）。
    if let Some(proxy) = state.get_proxy_client().await {
        let node_id = proxy.get_node_id().await.ok();
        let repo = proxy.get_repo_stats().await.ok();
        let peers = proxy.get_swarm_peers().await.ok();
        let bandwidth = proxy.get_bandwidth().await.ok();
        let bitswap = proxy.get_bitswap().await.ok();
        let pin_count = proxy
            .get_pin_list()
            .await
            .map(|p| p.pins.len())
            .unwrap_or(0);

        return Ok(DashboardStats {
            node_id,
            version: None,
            repo,
            peers,
            bandwidth,
            bitswap,
            pin_count,
        });
    }

    // 代理不可用时回退到直接 API 查询
    let client = state.get_api_client().await;
    get_dashboard_stats_inner(state, client).await
}

/// 内部：执行完整 API 查询并回填缓存
async fn get_dashboard_stats_inner(
    state: State<'_, AppState>,
    client: Option<IpfsApiClient>,
) -> Result<DashboardStats, DaemonError> {
    let client = client.ok_or(DaemonError::ApiError(
        "API client not initialized".to_string(),
    ))?;

    let node_id = match client.id().await {
        Ok(id) => {
            let json = serde_json::to_string(&id).unwrap_or_default();
            state.cache.set_node_info(&json);
            Some(id)
        }
        Err(e) => {
            tracing::warn!("Failed to get node id: {}", e);
            None
        }
    };

    let repo = match client.repo_stat().await {
        Ok(r) => {
            let json = serde_json::to_string(&r).unwrap_or_default();
            state.cache.set_repo_stats(&json);
            Some(r)
        }
        Err(e) => {
            tracing::warn!("Failed to get repo stats: {}", e);
            None
        }
    };

    let peers = match client.swarm_peers().await {
        Ok(p) => {
            let json = serde_json::to_string(&p).unwrap_or_default();
            state.cache.set_peers(&json);
            Some(p)
        }
        Err(e) => {
            tracing::warn!("Failed to get peers: {}", e);
            None
        }
    };

    let bandwidth = match client.stats_bw().await {
        Ok(b) => {
            let json = serde_json::to_string(&b).unwrap_or_default();
            state.cache.set_bandwidth(&json);
            Some(b)
        }
        Err(e) => {
            tracing::warn!("Failed to get bandwidth: {}", e);
            None
        }
    };

    let bitswap = match client.bitswap_stat().await {
        Ok(b) => {
            let json = serde_json::to_string(&b).unwrap_or_default();
            state.cache.set_bitswap(&json);
            Some(b)
        }
        Err(e) => {
            tracing::warn!("Failed to get bitswap stats: {}", e);
            None
        }
    };

    let pin_count = match client.pin_ls().await {
        Ok(pins) => pins.pins.len(),
        Err(e) => {
            tracing::warn!("Failed to get pin count: {}", e);
            0
        }
    };

    Ok(DashboardStats {
        node_id,
        version: None,
        repo,
        peers,
        bandwidth,
        bitswap,
        pin_count,
    })
}

// ════════════════════════════════════════════════════════════════
// Phase 3: 智能代理 + 离线队列 + 带宽管理
// ════════════════════════════════════════════════════════════════

/// 获取代理统计（缓存命中率、API调用数、熔断次数、延迟）
#[tauri::command]
pub async fn get_proxy_stats(
    state: State<'_, AppState>,
) -> Result<crate::proxy::ProxyStats, DaemonError> {
    if let Some(proxy) = state.get_proxy_client().await {
        Ok(proxy.get_stats().await)
    } else {
        Err(DaemonError::ApiError(
            "Proxy client not initialized".to_string(),
        ))
    }
}

/// 设置预取提示（Tab 切换时调用）
#[tauri::command]
pub async fn set_prefetch_hint(
    state: State<'_, AppState>,
    hint: String,
) -> Result<(), DaemonError> {
    use crate::proxy::PrefetchHint;
    let hint = match hint.as_str() {
        "dashboard" => PrefetchHint::Dashboard,
        "pins" => PrefetchHint::Pins,
        "files" => PrefetchHint::Files,
        "ipns" => PrefetchHint::Ipns,
        "daemon_started" => PrefetchHint::DaemonStarted,
        _ => return Err(DaemonError::ConfigError(format!("Unknown hint: {}", hint))),
    };
    if let Some(proxy) = state.get_proxy_client().await {
        proxy.set_prefetch_hint(hint).await;
        proxy.trigger_prefetch(hint).await;
    }
    Ok(())
}

/// 获取离线队列状态
#[tauri::command]
pub async fn get_offline_queue(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, DaemonError> {
    let entries = state
        .offline_queue
        .list_all()
        .map_err(DaemonError::ConfigError)?;
    let count = state
        .offline_queue
        .len()
        .map_err(DaemonError::ConfigError)?;

    Ok(serde_json::json!({
        "count": count,
        "entries": entries.iter().map(|e| serde_json::json!({
            "id": e.id,
            "operation": e.operation,
            "retry_count": e.retry_count,
            "last_error": e.last_error,
        })).collect::<Vec<_>>(),
    }))
}

/// 手动触发离线队列重放
#[tauri::command]
pub async fn flush_offline_queue(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, DaemonError> {
    let client = state.get_api_client().await.ok_or(DaemonError::ApiError(
        "API client not initialized".to_string(),
    ))?;

    let engine = crate::offline_queue::ReplayEngine::new(state.offline_queue.clone());
    let (success, failed) = engine.replay_all(&client).await;

    Ok(serde_json::json!({
        "success": success,
        "failed": failed,
        "remaining": state.offline_queue.len().unwrap_or(0),
    }))
}

/// 获取带宽配置
#[tauri::command]
pub async fn get_bandwidth_config(
    state: State<'_, AppState>,
) -> Result<crate::bandwidth::BandwidthConfig, DaemonError> {
    Ok(state.bandwidth_config.read().await.clone())
}

/// 更新带宽配置
#[tauri::command]
pub async fn set_bandwidth_config(
    state: State<'_, AppState>,
    config: crate::bandwidth::BandwidthConfig,
) -> Result<(), DaemonError> {
    // 立即更新内存配置
    *state.bandwidth_config.write().await = config.clone();

    // 如果有 Kubo 配置管理器，应用到磁盘
    if let Some(kc) = state.kubo_config.read().await.as_ref() {
        kc.apply_bandwidth_config(&config)
            .map_err(DaemonError::ConfigError)?;
    }

    tracing::info!(
        "Bandwidth config updated: {} conns / {} streams",
        config.max_connections,
        config.max_streams
    );
    Ok(())
}

/// 获取当前带宽速率
#[tauri::command]
pub async fn get_bandwidth_status(
    state: State<'_, AppState>,
) -> Result<crate::bandwidth::BandwidthStatus, DaemonError> {
    let monitor = state
        .bandwidth_monitor
        .lock()
        .map_err(|e| DaemonError::ConfigError(e.to_string()))?;

    Ok(crate::bandwidth::BandwidthStatus {
        rate_in: monitor.smooth_rate_in(),
        rate_out: monitor.smooth_rate_out(),
        total_in: monitor.last_total_in,
        total_out: monitor.last_total_out,
    })
}

/// 带降级的文件添加：守护进程不可用时自动入队
#[tauri::command]
pub async fn add_file_safe(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    file_path: String,
) -> Result<serde_json::Value, DaemonError> {
    let status = state.get_daemon_status().await;
    if !matches!(status, DaemonStatus::Running { .. }) {
        // 守护进程未运行 → 入队
        let op = crate::offline_queue::OfflineOperation::AddFile {
            file_path: file_path.clone(),
            queued_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        let id = state
            .offline_queue
            .enqueue(op)
            .map_err(DaemonError::ConfigError)?;
        tracing::info!("Daemon not running, queued file add as id={}", id);
        return Ok(serde_json::json!({
            "queued": true, "queue_id": id, "file_path": file_path
        }));
    }

    // 守护进程运行中 → 直接上传
    let result = add_file_with_progress(state.clone(), app_handle, file_path).await?;
    Ok(serde_json::json!({
        "queued": false,
        "hash": result.hash,
        "size": result.size,
        "name": result.name,
    }))
}

// ════════════════════════════════════════════════════════════════
// Phase 4: 双后端 + 兼容性测试 + 性能基准
// ════════════════════════════════════════════════════════════════

/// 获取当前活跃后端类型
#[tauri::command]
pub async fn get_active_backend(state: State<'_, AppState>) -> Result<String, DaemonError> {
    let backend = state.active_backend.read().await;
    Ok(backend.to_string())
}

/// 切换活跃后端
///
/// Kubo → Iroh: 尝试连接 Iroh 节点
/// Iroh → Kubo: 回退到 Kubo HTTP API
#[tauri::command]
pub async fn switch_backend(
    state: State<'_, AppState>,
    backend_type: String,
) -> Result<String, DaemonError> {
    let new_type = match backend_type.as_str() {
        "kubo" | "Kubo" => crate::backend_trait::BackendType::Kubo,
        "iroh" | "Iroh" => crate::backend_trait::BackendType::Iroh,
        _ => {
            return Err(DaemonError::ConfigError(format!(
                "Unknown backend: {}",
                backend_type
            )))
        }
    };

    // 诚实防护：Iroh 后端目前是 stub，实际的文件 / Pin / IPNS 操作仍全部
    // 硬编码走 Kubo HTTP（commands 未按 active_backend 路由）。若允许把活跃后端
    // 切到 Iroh，状态会与真实行为脱节。因此在 iroh_adapter 完成真实实现之前，
    // 这里明确拒绝切换，只保留其在基准 / 兼容性测试中的用途。
    if matches!(new_type, crate::backend_trait::BackendType::Iroh) {
        return Err(DaemonError::ApiError(
            "Iroh 后端尚未接入实际操作（当前仅用于基准与兼容性测试）；\
             文件 / Pin / IPNS 仍由 Kubo 处理，暂不支持切换为活跃后端"
                .to_string(),
        ));
    }

    // 验证目标后端可用
    let available = match new_type {
        crate::backend_trait::BackendType::Kubo => state.kubo_backend.is_available().await,
        crate::backend_trait::BackendType::Iroh => state.iroh_backend.is_available().await,
    };

    if !available {
        return Err(DaemonError::ApiError(format!(
            "Backend '{}' is not available",
            new_type
        )));
    }

    *state.active_backend.write().await = new_type;
    tracing::info!("Switched backend to: {}", new_type);

    Ok(format!("Switched to {}", new_type))
}

/// 获取后端能力信息
#[tauri::command]
pub async fn get_backend_capabilities(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, DaemonError> {
    let active = *state.active_backend.read().await;
    let caps = match active {
        crate::backend_trait::BackendType::Kubo => state.kubo_backend.capabilities(),
        crate::backend_trait::BackendType::Iroh => state.iroh_backend.capabilities(),
    };
    Ok(serde_json::to_value(caps).unwrap_or_default())
}

// ════════════════════════════════════════════════════════════════
// Phase 4: 性能基准测试 & 协议兼容性测试
// ════════════════════════════════════════════════════════════════

/// 运行 Kubo vs Iroh 微基准测试
///
/// 对 node_info / repo_stat / swarm_peers 三个操作各测量若干次延迟，
/// 返回统计结果。后端不可用时该操作会记录为失败（不会导致命令报错）。
#[tauri::command]
pub async fn run_benchmark(
    state: State<'_, AppState>,
) -> Result<crate::benchmark::BenchSuiteResult, DaemonError> {
    tracing::info!("run_benchmark");
    let kubo = state.kubo_backend.as_ref().clone();
    let iroh = state.iroh_backend.as_ref().clone();
    let bench = crate::benchmark::MicroBenchmark::new(kubo, iroh);
    Ok(bench.run_all().await)
}

/// 运行 Kubo ↔ Iroh 协议兼容性测试
///
/// 校验版本信息、可用性、仓库初始化、节点发现等维度，返回兼容性评分。
#[tauri::command]
pub async fn run_compat_test(
    state: State<'_, AppState>,
) -> Result<crate::compat_test::CompatSuiteResult, DaemonError> {
    tracing::info!("run_compat_test");
    let kubo = state.kubo_backend.as_ref().clone();
    let iroh = state.iroh_backend.as_ref().clone();
    let mut tester = crate::compat_test::CompatTester::new(kubo, iroh);
    Ok(tester.run_all().await)
}

// ════════════════════════════════════════════════════════════════
// Phase B (a): iroh 原生收发 + BlobTicket 分享
//
// 这些命令直接走 iroh 原生后端（不经 Kubo）。未启用 `iroh-backend` feature
// 时后端为 stub，命令会返回「需启用 feature」的错误，前端可据此提示。
// ════════════════════════════════════════════════════════════════

/// BackendError → DaemonError（前端统一错误模型）
fn iroh_err(e: crate::backend_trait::BackendError) -> DaemonError {
    use crate::backend_trait::BackendErrorKind as K;
    match e.kind {
        K::InvalidArgument => DaemonError::ConfigError(e.message),
        K::NotFound => DaemonError::ApiError(format!("content not found: {}", e.message)),
        K::Unavailable => DaemonError::ApiConnectionFailed {
            addr: "iroh".into(),
            detail: e.message,
        },
        K::Network | K::Timeout => DaemonError::ApiConnectionFailed {
            addr: "iroh".into(),
            detail: e.message,
        },
        K::Unsupported => DaemonError::Backend {
            kind: "Unsupported".into(),
            message: e.message,
        },
        K::Internal => DaemonError::Backend {
            kind: "Internal".into(),
            message: e.message,
        },
    }
}

/// 用 iroh 原生后端添加文件（内容寻址，返回 BLAKE3 hash 作为 cid）
#[tauri::command]
pub async fn iroh_add_file(
    state: State<'_, AppState>,
    file_path: String,
) -> Result<crate::backend_trait::AddOutput, DaemonError> {
    let path = std::path::PathBuf::from(&file_path);
    if !path.exists() {
        return Err(DaemonError::IoError(format!(
            "File not found: {}",
            file_path
        )));
    }
    let out = state.iroh_backend.add_file(&path).await.map_err(iroh_err)?;
    // 打上 iroh 来源标记，供 Auto 路由精确分发
    state
        .backend_router
        .record_origin(&out.cid, BackendType::Iroh)
        .await;
    Ok(out)
}

/// 获取 iroh 节点信息（持久身份 / 版本）
#[tauri::command]
pub async fn iroh_node_info(
    state: State<'_, AppState>,
) -> Result<crate::backend_trait::NodeInfo, DaemonError> {
    state.iroh_backend.node_info().await.map_err(iroh_err)
}

/// 为本地某个 iroh blob 生成可分享的 ticket 字符串
///
/// 对方用 `iroh_fetch_ticket` 即可跨节点收取内容。
#[tauri::command]
pub async fn iroh_share(state: State<'_, AppState>, cid: String) -> Result<String, DaemonError> {
    tracing::info!("iroh_share: {}", cid);
    state
        .iroh_backend
        .share_ticket(&cid)
        .await
        .map_err(iroh_err)
}

/// 用 ticket 从远端 iroh 节点收取内容，可选保存到本地路径
///
/// 返回 `{ cid, size, saved }`。
#[tauri::command]
pub async fn iroh_fetch_ticket(
    state: State<'_, AppState>,
    ticket: String,
    save_path: Option<String>,
) -> Result<serde_json::Value, DaemonError> {
    validate_ticket(&ticket)?;
    tracing::info!(
        "iroh_fetch_ticket requested (ticket redacted, save={})",
        save_path.is_some()
    );
    let parsed_cid = crate::iroh_adapter::ticket_cid(&ticket);
    if let Some(ref p) = save_path {
        let path = std::path::PathBuf::from(p);
        validate_output_path(&path)?;
        let (cid, size) = state
            .iroh_backend
            .fetch_ticket_to_file(&ticket, &path)
            .await
            .map_err(iroh_err)?;
        state
            .backend_router
            .record_origin(&cid, BackendType::Iroh)
            .await;
        state.backend_router.record_provider(&cid, &ticket).await;
        return Ok(serde_json::json!({ "cid": cid, "size": size, "saved": p }));
    }
    let bytes = state
        .iroh_backend
        .fetch_ticket(&ticket)
        .await
        .map_err(iroh_err)?;
    let cid = parsed_cid;
    if let Some(cid) = &cid {
        // 内容已落入 iroh → 标记来源；同时记住 provider，供日后本地 miss 时网络重取
        state
            .backend_router
            .record_origin(cid, BackendType::Iroh)
            .await;
        state.backend_router.record_provider(cid, &ticket).await;
    }
    let size = bytes.len();

    Ok(serde_json::json!({ "cid": cid, "size": size, "saved": null }))
}

/// keep-alive：让某 iroh blob 不被 GC 回收（命名持久 tag；对应 Kubo 的 pin）
#[tauri::command]
pub async fn iroh_keep(state: State<'_, AppState>, cid: String) -> Result<(), DaemonError> {
    tracing::info!("iroh_keep: {}", cid);
    state.iroh_backend.keep(&cid).await.map_err(iroh_err)
}

/// 取消 keep-alive（删除命名 tag，内容此后可被 GC）
#[tauri::command]
pub async fn iroh_unkeep(state: State<'_, AppState>, cid: String) -> Result<(), DaemonError> {
    tracing::info!("iroh_unkeep: {}", cid);
    state.iroh_backend.unkeep(&cid).await.map_err(iroh_err)
}

/// 关闭 iroh 网络/serving 栈（Phase D2 生命周期）；下次使用自动重建（重启）。
#[tauri::command]
pub async fn iroh_shutdown(state: State<'_, AppState>) -> Result<(), DaemonError> {
    tracing::info!("iroh_shutdown requested");
    state.iroh_backend.shutdown().await.map_err(iroh_err)
}

// ════════════════════════════════════════════════════════════════
// Phase C (b): 双栈路由骨架
// ════════════════════════════════════════════════════════════════

/// 登记一个 BlobTicket 作为某 CID 的 provider（**不立即拉取**）。
///
/// 之后在 `Auto` 策略下 `cat` 该 CID 时，若本地两栈都没有，会自动从这个 provider
/// 跨节点取回——「先记住去哪拿，用时再拿」的惰性跨节点内容发现。返回解析出的 CID。
#[tauri::command]
pub async fn iroh_register_ticket(
    state: State<'_, AppState>,
    ticket: String,
) -> Result<String, DaemonError> {
    validate_ticket(&ticket)?;
    let cid = crate::iroh_adapter::ticket_cid(&ticket).ok_or_else(|| {
        DaemonError::ApiError(
            "invalid ticket (build with --features iroh-backend to parse tickets)".to_string(),
        )
    })?;
    state.backend_router.record_provider(&cid, &ticket).await;
    tracing::info!("registered iroh provider for {}", cid);
    Ok(cid)
}

/// 获取当前路由策略（KuboOnly / IrohOnly / Auto）
#[tauri::command]
pub async fn get_route_policy(state: State<'_, AppState>) -> Result<String, DaemonError> {
    Ok(state.backend_router.policy().await.to_string())
}

/// 设置路由策略
#[tauri::command]
pub async fn set_route_policy(
    state: State<'_, AppState>,
    policy: String,
) -> Result<String, DaemonError> {
    let p = crate::backend_router::RoutePolicy::parse(&policy)
        .ok_or_else(|| DaemonError::ConfigError(format!("Unknown route policy: {}", policy)))?;
    state.backend_router.set_policy(p).await;
    let mut config = state.get_config().await;
    config.route_policy = p.to_string();
    config.save().map_err(DaemonError::ConfigError)?;
    state.update_config(config).await;
    Ok(p.to_string())
}

fn validate_cid(cid: &str) -> Result<(), DaemonError> {
    let value = cid.trim();
    if value.is_empty() || value.len() > 256 || !value.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(DaemonError::ApiError("invalid CID/hash format".to_string()));
    }
    Ok(())
}

fn validate_ticket(ticket: &str) -> Result<(), DaemonError> {
    let value = ticket.trim();
    if value.is_empty() || value.len() > 16_384 || value.bytes().any(|b| b.is_ascii_control()) {
        return Err(DaemonError::ApiError("invalid iroh ticket".to_string()));
    }
    Ok(())
}

fn validate_path_count(paths: &[String]) -> Result<(), DaemonError> {
    if paths.is_empty() || paths.len() > 256 {
        return Err(DaemonError::IoError(
            "file batch must contain 1 to 256 paths".to_string(),
        ));
    }
    Ok(())
}

fn validate_output_path(path: &std::path::Path) -> Result<(), DaemonError> {
    crate::path_security::validate_output_path(path)
}

/// 查询某个 CID 在当前策略下会被路由到哪个后端（不实际读取，只做决策展示）
#[tauri::command]
pub async fn get_backend_route(
    state: State<'_, AppState>,
    cid: String,
) -> Result<String, DaemonError> {
    let t = state.backend_router.choose_for_cid(&cid).await;
    Ok(t.to_string())
}

// ════════════════════════════════════════════════════════════════
// Phase D1: 节点身份（可命名 · 可验证 · 可展示）
// ════════════════════════════════════════════════════════════════

/// 组合后的节点身份（人类可读标签 + 两后端的密码学身份）
#[derive(Debug, Clone, Serialize)]
pub struct NodeIdentityInfo {
    pub label: String,
    pub created_at: u64,
    /// Kubo PeerID（守护进程运行时才有）
    pub kubo_peer_id: Option<String>,
    /// iroh EndpointId（自证公钥；feature 构建下为持久身份，否则为 stub 占位）
    pub iroh_node_id: Option<String>,
}

/// 获取节点身份（标签 + 两后端 ID）
#[tauri::command]
pub async fn get_node_identity(
    state: State<'_, AppState>,
) -> Result<NodeIdentityInfo, DaemonError> {
    let id = state.identity.load();

    // Kubo PeerID 直接取自守护进程状态（无需额外查询）
    let kubo_peer_id = match state.get_daemon_status().await {
        DaemonStatus::Running { peer_id, .. } => Some(peer_id),
        _ => None,
    };

    // iroh 节点身份（feature 构建下为持久 node_id）
    let iroh_node_id = state.iroh_backend.node_info().await.ok().map(|n| n.peer_id);

    Ok(NodeIdentityInfo {
        label: id.label,
        created_at: id.created_at,
        kubo_peer_id,
        iroh_node_id,
    })
}

/// 设置节点的人类可读标签
#[tauri::command]
pub async fn set_node_label(
    state: State<'_, AppState>,
    label: String,
) -> Result<NodeIdentityInfo, DaemonError> {
    state
        .identity
        .set_label(&label)
        .map_err(DaemonError::ConfigError)?;
    tracing::info!("node label set to '{}'", label.trim());
    get_node_identity(state).await
}

/// 导出可验证的身份文档（iroh node_id 本身是自证 Ed25519 公钥）
#[tauri::command]
pub async fn export_identity(state: State<'_, AppState>) -> Result<String, DaemonError> {
    let info = get_node_identity(state).await?;
    let doc = serde_json::json!({
        "label": info.label,
        "created_at": info.created_at,
        "kubo_peer_id": info.kubo_peer_id,
        "iroh_node_id": info.iroh_node_id,
        "note": "iroh node_id is a self-certifying Ed25519 public key; a peer verifies it by connecting.",
    });
    serde_json::to_string_pretty(&doc).map_err(|e| DaemonError::ConfigError(e.to_string()))
}

// ════════════════════════════════════════════════════════════════
// Phase D3: 节点健康度（「我的节点健康度」）
// ════════════════════════════════════════════════════════════════

/// 节点健康度快照（跨后端聚合，字段按可用性填充）
#[derive(Debug, Clone, Serialize)]
pub struct NodeHealth {
    /// 应用运行时长（秒）
    pub app_uptime_secs: u64,
    /// 守护进程本次在线时长（秒；未运行为 None）
    pub daemon_uptime_secs: Option<u64>,
    pub kubo_running: bool,
    /// 仓库对象数 / 大小（Kubo）
    pub num_objects: Option<u64>,
    pub repo_size: Option<u64>,
    /// 连接的对等节点数（Kubo）
    pub peers: Option<usize>,
    /// 累计接收 / 发送字节（Kubo；「贡献量」看 bytes_out）
    pub bytes_in: Option<u64>,
    pub bytes_out: Option<u64>,
    /// iroh 本地内容条目数（feature 构建下有值，否则 None）
    pub iroh_content_count: Option<u64>,
}

/// 获取节点健康度快照
#[tauri::command]
pub async fn get_node_health(state: State<'_, AppState>) -> Result<NodeHealth, DaemonError> {
    let now = crate::state::now_secs();
    let app_uptime_secs = now.saturating_sub(state.app_started_at);

    let kubo_running = matches!(
        state.get_daemon_status().await,
        DaemonStatus::Running { .. }
    );
    let daemon_uptime_secs = if kubo_running {
        state
            .daemon_started_at
            .read()
            .await
            .map(|t| now.saturating_sub(t))
    } else {
        None
    };

    let (mut num_objects, mut repo_size, mut peers, mut bytes_in, mut bytes_out) =
        (None, None, None, None, None);
    if kubo_running {
        if let Some(client) = state.get_api_client().await {
            // 并行拉取（各自失败不影响其他）
            let (repo, sw, bw) =
                tokio::join!(client.repo_stat(), client.swarm_peers(), client.stats_bw(),);
            if let Ok(r) = repo {
                num_objects = Some(r.num_objects);
                repo_size = Some(r.repo_size);
            }
            if let Ok(s) = sw {
                peers = Some(s.peers.len());
            }
            if let Ok(b) = bw {
                bytes_in = Some(b.total_in);
                bytes_out = Some(b.total_out);
            }
        }
    }

    // iroh 内容数（stub 构建为 None）
    let iroh_content_count = state.iroh_backend.content_count().await.ok();

    Ok(NodeHealth {
        app_uptime_secs,
        daemon_uptime_secs,
        kubo_running,
        num_objects,
        repo_size,
        peers,
        bytes_in,
        bytes_out,
        iroh_content_count,
    })
}

// ════════════════════════════════════════════════════════════════
// 二进制哈希校验（安全增强）
// ════════════════════════════════════════════════════════════════

/// 二进制验证信息
#[derive(Debug, Clone, Serialize)]
pub struct BinaryVerificationInfo {
    /// 二进制文件路径
    pub path: String,
    /// 版本信息
    pub version: String,
    /// 计算出的 SHA-256 哈希
    pub sha256: String,
    /// 是否匹配已知官方哈希
    pub matches_known_hash: bool,
    /// 当前平台标识
    pub platform: String,
}

/// 获取当前使用的 Kubo 二进制的验证信息
#[tauri::command]
pub async fn get_binary_verification_info(
    _state: State<'_, AppState>,
) -> Result<BinaryVerificationInfo, DaemonError> {
    // 查找二进制文件
    let binary_path = crate::daemon::BinaryFinder::find().ok_or(DaemonError::BinaryNotFound)?;

    // 获取版本
    let version = crate::daemon::BinaryFinder::get_version(&binary_path)?;

    // 计算哈希
    let sha256 = crate::daemon::BinaryFinder::calculate_hash(&binary_path)
        .map_err(|e| DaemonError::BinaryVerificationFailed(e.to_string()))?;

    // 检查是否匹配已知哈希
    let matches_known_hash =
        crate::daemon::BinaryFinder::verify_against_known_hashes(&binary_path).unwrap_or(false);

    let platform = crate::daemon::KuboHashes::get_current_platform();

    Ok(BinaryVerificationInfo {
        path: binary_path.to_string_lossy().to_string(),
        version,
        sha256,
        matches_known_hash,
        platform,
    })
}

/// 设置配置中的 Kubo 二进制 SHA-256 哈希
#[tauri::command]
pub async fn set_binary_hash(
    state: State<'_, AppState>,
    hash: Option<String>,
) -> Result<(), DaemonError> {
    let mut config = state.get_config().await;

    // 验证哈希格式（如果提供）
    if let Some(ref h) = hash {
        if h.len() != 64 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(DaemonError::ConfigError(
                "Invalid hash format: must be 64 hexadecimal characters".to_string(),
            ));
        }
    }

    config.kubo_binary_sha256 = hash;
    config.save().map_err(DaemonError::ConfigError)?;
    state.update_config(config).await;

    Ok(())
}
