use tauri::State;
use tauri::Emitter;
use crate::state::AppState;
use crate::config::AppConfig;
use crate::types::DaemonStatus;
use crate::daemon::{
    BinaryFinder, DaemonController,
    PinList, PinAddResult, PinRmResult,
    BandwidthStats, BitswapStats, NodeId, RepoStats, SwarmPeers,
    IpfsApiClient,
};
use crate::error::DaemonError;
use crate::backend_trait::Backend;
use serde::Serialize;

/// 获取守护进程状态
#[tauri::command]
pub async fn get_daemon_status(
    state: State<'_, AppState>
) -> Result<DaemonStatus, DaemonError> {
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
    if !matches!(current_status, DaemonStatus::Stopped | DaemonStatus::Failed { .. }) {
        return Err(DaemonError::InvalidState);
    }

    // 更新状态为启动中
    state.set_daemon_status(DaemonStatus::Starting).await;

    // 发送事件到前端
    app_handle.emit("daemon-status-changed", &DaemonStatus::Starting)
        .map_err(|e| DaemonError::ConfigError(e.to_string()))?;

    tracing::info!("Starting IPFS daemon...");

    // 查找 IPFS 二进制文件
    let binary_path = match BinaryFinder::find() {
        Some(path) => {
            tracing::info!("Found IPFS binary: {:?}", path);
            path
        }
        None => {
            tracing::error!("Could not find IPFS binary");
            state.set_daemon_status(
                DaemonStatus::Failed { error: DaemonError::BinaryNotFound.to_string() }
            ).await;
            app_handle.emit("daemon-status-changed", &state.get_daemon_status().await)
                .map_err(|e| DaemonError::ConfigError(e.to_string()))?;
            return Err(DaemonError::BinaryNotFound);
        }
    };

    // 获取配置
    let config = state.get_config().await;
    let repo_path = config.get_ipfs_path();
    let flags = config.daemon_flags.clone();

    // 创建守护进程控制器
    let controller = DaemonController::new(binary_path, repo_path);

    // 启动守护进程
    match controller.start(flags).await {
        Ok(_) => {
            tracing::info!("Daemon started successfully");

            // 获取节点信息
            if let Some(api_client) = state.get_api_client().await {
                match api_client.id().await {
                    Ok(node_id) => {
                        let pid = controller.get_pid().await.unwrap_or(0);
                        let status = DaemonStatus::Running {
                            pid,
                            peer_id: node_id.id.clone(),
                            api_addr: config.api_addr.clone(),
                        };

                        state.set_daemon_status(status.clone()).await;
                        app_handle.emit("daemon-status-changed", &status)
                            .map_err(|e| DaemonError::ConfigError(e.to_string()))?;

                        tracing::info!("Node ID: {}", node_id.id);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to get node ID: {}", e);
                        // 即使无法获取节点 ID，守护进程也可能正在启动
                        state.set_daemon_status(DaemonStatus::Starting).await;
                    }
                }
            }

            // 保存控制器
            state.set_daemon_controller(Some(controller)).await;

            // 启动健康监控（必须在控制器设置之后，否则监控会检测不到控制器而退出）
            state.spawn_health_monitor(app_handle.clone()).await;

            // Phase 2: 启动仪表盘自动轮询
            state.spawn_dashboard_poller(app_handle.clone()).await;

            // Phase 3: 启动离线队列重放循环
            state.spawn_replay_loop(app_handle.clone()).await;

            Ok(())
        }
        Err(e) => {
            let err_str = e.to_string();
            tracing::error!("Failed to start daemon: {}", err_str);
            state.set_daemon_status(DaemonStatus::Failed { error: err_str.clone() }).await;
            app_handle.emit("daemon-status-changed", &state.get_daemon_status().await)
                .map_err(|e| DaemonError::ConfigError(e.to_string()))?;
            Err(e)
        }
    }
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
    app_handle.emit("daemon-status-changed", &DaemonStatus::Stopping)
        .map_err(|e| DaemonError::ConfigError(e.to_string()))?;

    tracing::info!("Stopping IPFS daemon...");

    // 获取控制器
    if let Some(controller) = state.get_daemon_controller().await {
        match controller.stop().await {
            Ok(_) => {
                tracing::info!("Daemon stopped successfully");
                state.set_daemon_status(DaemonStatus::Stopped).await;
                state.set_daemon_controller(None).await;

                app_handle.emit("daemon-status-changed", &DaemonStatus::Stopped)
                    .map_err(|e| DaemonError::ConfigError(e.to_string()))?;

                Ok(())
            }
            Err(e) => {
                let err_str = e.to_string();
                tracing::error!("Failed to stop daemon: {}", err_str);
                state.set_daemon_status(DaemonStatus::Failed { error: err_str.clone() }).await;
                app_handle.emit("daemon-status-changed", &state.get_daemon_status().await)
                    .map_err(|e| DaemonError::ConfigError(e.to_string()))?;
                Err(e)
            }
        }
    } else {
        // 没有控制器，直接设置为已停止
        state.set_daemon_status(DaemonStatus::Stopped).await;
        app_handle.emit("daemon-status-changed", &DaemonStatus::Stopped)
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
                        app_handle.emit("daemon-status-changed", &status)
                            .map_err(|e| DaemonError::ConfigError(e.to_string()))?;
                        return Ok(());
                    }
                }
                state.set_daemon_status(DaemonStatus::Running {
                    pid: controller.get_pid().await.unwrap_or(0),
                    peer_id: "unknown".to_string(),
                    api_addr: config.api_addr.clone(),
                }).await;
                let current = state.get_daemon_status().await;
                app_handle.emit("daemon-status-changed", &current)
                    .map_err(|e| DaemonError::ConfigError(e.to_string()))?;
                Ok(())
            }
            Err(e) => {
                let err_str = e.to_string();
                let failed = DaemonStatus::Failed { error: err_str.clone() };
                state.set_daemon_status(failed.clone()).await;
                app_handle.emit("daemon-status-changed", &failed)
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
pub async fn get_config(
    state: State<'_, AppState>
) -> Result<AppConfig, DaemonError> {
    Ok(state.get_config().await)
}

/// 更新配置
#[tauri::command]
pub async fn update_config(
    state: State<'_, AppState>,
    new_config: AppConfig,
) -> Result<(), DaemonError> {
    // 验证配置
    new_config.validate()
        .map_err(DaemonError::ConfigError)?;

    // 保存配置到磁盘
    new_config.save()
        .map_err(DaemonError::ConfigError)?;

    // 更新状态
    state.update_config(new_config.clone()).await;

    tracing::info!("Configuration updated and saved");

    Ok(())
}

/// 获取节点 ID（用于测试 API 连接）
#[tauri::command]
pub async fn get_node_id(
    state: State<'_, AppState>,
) -> Result<String, DaemonError> {
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
        Err(DaemonError::ApiError("API client not initialized".to_string()))
    }
}

/// 打开 IPFS WebUI
///
/// 在系统默认浏览器中打开 IPFS 自带的 WebUI。
/// 需要守护进程处于运行状态。
#[tauri::command]
pub async fn open_webui(
    state: State<'_, AppState>,
) -> Result<(), DaemonError> {
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
        _ => {
            Err(DaemonError::InvalidState)
        }
    }
}

/// 获取 WebUI URL
#[tauri::command]
pub async fn get_webui_url(
    state: State<'_, AppState>,
) -> Result<String, DaemonError> {
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
) -> Result<crate::types::AddResult, DaemonError> {
    let path = std::path::PathBuf::from(&file_path);
    if !path.exists() {
        return Err(DaemonError::IoError(format!("File not found: {}", file_path)));
    }

    let client = state.get_api_client().await
        .ok_or(DaemonError::ApiError("API client not initialized".to_string()))?;

    client.add_file(&path).await
}

/// 批量添加文件到 IPFS
#[tauri::command]
pub async fn add_files(
    state: State<'_, AppState>,
    file_paths: Vec<String>,
) -> Result<Vec<crate::types::AddResult>, DaemonError> {
    let client = state.get_api_client().await
        .ok_or(DaemonError::ApiError("API client not initialized".to_string()))?;

    let mut results = Vec::new();
    for file_path in &file_paths {
        let path = std::path::PathBuf::from(file_path);
        if path.exists() {
            results.push(client.add_file(&path).await?);
        }
    }
    Ok(results)
}

/// 设置开机自启
#[tauri::command]
pub async fn set_auto_launch(
    state: State<'_, AppState>,
    enable: bool,
) -> Result<(), DaemonError> {
    let app_path = std::env::current_exe()
        .map_err(|e| DaemonError::IoError(format!("Failed to get app path: {}", e)))?;

    let app_name = "ipfs-desktop-rust";

    let auto = auto_launch::AutoLaunchBuilder::new()
        .set_app_name(app_name)
        .set_app_path(&app_path.to_string_lossy())
        .set_use_launch_agent(true)
        .build()
        .map_err(|e| DaemonError::ConfigError(format!("Failed to build auto-launch: {}", e)))?;

    if enable {
        auto.enable()
            .map_err(|e| DaemonError::ConfigError(format!("Failed to enable auto-launch: {}", e)))?;
        tracing::info!("Auto-launch enabled");
    } else {
        auto.disable()
            .map_err(|e| DaemonError::ConfigError(format!("Failed to disable auto-launch: {}", e)))?;
        tracing::info!("Auto-launch disabled");
    }

    // 更新配置
    let mut config = state.get_config().await;
    config.auto_launch = enable;
    config.save()
        .map_err(DaemonError::ConfigError)?;
    state.update_config(config).await;

    Ok(())
}

/// 获取开机自启状态
#[tauri::command]
pub async fn get_auto_launch(
    state: State<'_, AppState>,
) -> Result<bool, DaemonError> {
    Ok(state.get_config().await.auto_launch)
}

// ════════════════════════════════════════════════════════════════
// A1: 下载功能 — cat / get
// ════════════════════════════════════════════════════════════════

/// 从 IPFS 读取文件内容（通过 CID）
///
/// 返回文件的原始字节内容。对于文本文件，前端会转换为字符串显示。
#[tauri::command]
pub async fn cat_file(
    state: State<'_, AppState>,
    cid: String,
) -> Result<Vec<u8>, DaemonError> {
    tracing::info!("cat_file: {}", cid);

    let client = state.get_api_client().await
        .ok_or(DaemonError::ApiError("API client not initialized".to_string()))?;

    client.cat(&cid).await
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

    let client = state.get_api_client().await
        .ok_or(DaemonError::ApiError("API client not initialized".to_string()))?;

    let output = std::path::PathBuf::from(&save_path);

    // 使用流式下载
    let data = client.cat_stream(&cid, |loaded, total| {
        let _ = app_handle.emit("download-progress", &DownloadProgress {
            cid: cid.clone(),
            loaded,
            total,
        });
    }).await?;

    // 确保父目录存在
    if let Some(parent) = output.parent() {
        tokio::fs::create_dir_all(parent).await
            .map_err(|e| DaemonError::IoError(format!("Failed to create output dir: {}", e)))?;
    }

    tokio::fs::write(&output, &data).await
        .map_err(|e| DaemonError::IoError(format!("Failed to write file: {}", e)))?;

    tracing::info!("Download complete: {} bytes -> {:?}", data.len(), output);
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
pub async fn get_file_size(
    state: State<'_, AppState>,
    cid: String,
) -> Result<u64, DaemonError> {
    let client = state.get_api_client().await
        .ok_or(DaemonError::ApiError("API client not initialized".to_string()))?;

    client.file_size(&cid).await
}

// ════════════════════════════════════════════════════════════════
// A2: Pin 管理
// ════════════════════════════════════════════════════════════════

/// 获取 Pin 列表
#[tauri::command]
pub async fn get_pin_list(
    state: State<'_, AppState>,
) -> Result<PinList, DaemonError> {
    tracing::info!("get_pin_list");

    // 优先走智能代理（缓存 + 熔断 + 统计）
    if let Some(proxy) = state.get_proxy_client().await {
        if let Ok(pins) = proxy.get_pin_list().await {
            return Ok(pins);
        }
    }

    let client = state.get_api_client().await
        .ok_or(DaemonError::ApiError("API client not initialized".to_string()))?;

    client.pin_ls().await
}

/// 添加 Pin
#[tauri::command]
pub async fn add_pin(
    state: State<'_, AppState>,
    cid: String,
) -> Result<PinAddResult, DaemonError> {
    tracing::info!("add_pin: {}", cid);

    let client = state.get_api_client().await
        .ok_or(DaemonError::ApiError("API client not initialized".to_string()))?;

    let result = client.pin_add(&cid).await?;
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

    let client = state.get_api_client().await
        .ok_or(DaemonError::ApiError("API client not initialized".to_string()))?;

    let result = client.pin_rm(&cid).await?;
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

    let client = state.get_api_client().await
        .ok_or(DaemonError::ApiError("API client not initialized".to_string()))?;

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

    let node_id = node_id.map_err(|e| tracing::warn!("Failed to get node id: {}", e)).ok();
    let version = version.map(|v| v.version)
        .map_err(|e| tracing::warn!("Failed to get version: {}", e)).ok();
    let repo = repo.map_err(|e| tracing::warn!("Failed to get repo stats: {}", e)).ok();
    let peers = peers.map_err(|e| tracing::warn!("Failed to get peers: {}", e)).ok();
    let bandwidth = bandwidth.map_err(|e| tracing::warn!("Failed to get bandwidth: {}", e)).ok();
    let bitswap = bitswap.map_err(|e| tracing::warn!("Failed to get bitswap stats: {}", e)).ok();
    let pin_count = match pin_ls {
        Ok(pins) => pins.pins.len(),
        Err(e) => { tracing::warn!("Failed to get pin count: {}", e); 0 }
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
        return Err(DaemonError::IoError(format!("File not found: {}", file_path)));
    }

    let file_name = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed")
        .to_string();

    let client = state.get_api_client().await
        .ok_or(DaemonError::ApiError("API client not initialized".to_string()))?;

    // 发送开始事件
    let _ = app_handle.emit("upload-progress", &UploadProgress {
        name: file_name.clone(),
        loaded: 0,
        total: 0,
    });

    // 真实分块进度：回调按字节累加，节流至每 512KB 或完成时推送一次
    let app = app_handle.clone();
    let name_cb = file_name.clone();
    let last_emit = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let result = client.add_file_streaming(&path, move |sent, total| {
        use std::sync::atomic::Ordering;
        let prev = last_emit.load(Ordering::Relaxed);
        if sent == total || sent.saturating_sub(prev) >= 512 * 1024 {
            last_emit.store(sent, Ordering::Relaxed);
            let _ = app.emit("upload-progress", &UploadProgress {
                name: name_cb.clone(),
                loaded: sent,
                total,
            });
        }
    }).await?;

    tracing::info!("Upload complete: {} -> {}", file_path, result.hash);
    Ok(result)
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

    let client = state.get_api_client().await
        .ok_or(DaemonError::ApiError("API client not initialized (start the daemon first)".to_string()))?;

    // 由 Kubo 生成密钥，返回真实的 IPNS 名称（Id）
    let kg = client.key_gen(&label).await?;
    let record = crate::keyring::KeyRecord::from_kubo(kg.name, kg.id);

    // 保存公开记录（便于离线展示）
    state.key_manager.save_record(&record)
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
            let records: Vec<crate::keyring::KeyRecord> = kl.keys.into_iter()
                .map(|k| crate::keyring::KeyRecord::from_kubo(k.name, k.id))
                .collect();
            state.key_manager.sync_from_kubo(&records);
            return Ok(records);
        }
    }

    // 守护进程不可用 → 回退到本地记录
    state.key_manager.list_records()
        .map_err(DaemonError::ConfigError)
}

/// 删除密钥（同时从 Kubo 密钥库和本地记录移除）
#[tauri::command]
pub async fn delete_key(
    state: State<'_, AppState>,
    label: String,
) -> Result<(), DaemonError> {
    // 先删 Kubo 侧（"self" 等内置密钥可能失败，记录但不阻断本地清理）
    if let Some(client) = state.get_api_client().await {
        if let Err(e) = client.key_rm(&label).await {
            tracing::warn!("Kubo key/rm failed for '{}': {}", label, e);
        }
    }
    state.key_manager.delete_record(&label)
        .map_err(DaemonError::ConfigError)
}

/// IPNS 发布：将 CID 绑定到密钥名称
#[tauri::command]
pub async fn ipns_publish(
    state: State<'_, AppState>,
    cid: String,
    key_name: String,
    lifetime: Option<String>,
) -> Result<crate::daemon::IpnsPublishResult, DaemonError> {
    let lifetime = lifetime.unwrap_or_else(|| "24h".to_string());
    tracing::info!("ipns_publish: {} -> {}", cid, key_name);

    let client = state.get_api_client().await
        .ok_or(DaemonError::ApiError("API client not initialized".to_string()))?;

    client.name_publish(&cid, &key_name, &lifetime).await
}

/// IPNS 解析：将 IPNS 名称解析为 CID
#[tauri::command]
pub async fn ipns_resolve(
    state: State<'_, AppState>,
    name: String,
) -> Result<crate::daemon::IpnsResolveResult, DaemonError> {
    tracing::info!("ipns_resolve: {}", name);

    let client = state.get_api_client().await
        .ok_or(DaemonError::ApiError("API client not initialized".to_string()))?;

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
        let pin_count = proxy.get_pin_list().await.map(|p| p.pins.len()).unwrap_or(0);

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
    let client = client
        .ok_or(DaemonError::ApiError("API client not initialized".to_string()))?;

    let node_id = match client.id().await {
        Ok(id) => {
            let json = serde_json::to_string(&id).unwrap_or_default();
            state.cache.set_node_info(&json);
            Some(id)
        }
        Err(e) => { tracing::warn!("Failed to get node id: {}", e); None }
    };

    let repo = match client.repo_stat().await {
        Ok(r) => {
            let json = serde_json::to_string(&r).unwrap_or_default();
            state.cache.set_repo_stats(&json);
            Some(r)
        }
        Err(e) => { tracing::warn!("Failed to get repo stats: {}", e); None }
    };

    let peers = match client.swarm_peers().await {
        Ok(p) => {
            let json = serde_json::to_string(&p).unwrap_or_default();
            state.cache.set_peers(&json);
            Some(p)
        }
        Err(e) => { tracing::warn!("Failed to get peers: {}", e); None }
    };

    let bandwidth = match client.stats_bw().await {
        Ok(b) => {
            let json = serde_json::to_string(&b).unwrap_or_default();
            state.cache.set_bandwidth(&json);
            Some(b)
        }
        Err(e) => { tracing::warn!("Failed to get bandwidth: {}", e); None }
    };

    let bitswap = match client.bitswap_stat().await {
        Ok(b) => {
            let json = serde_json::to_string(&b).unwrap_or_default();
            state.cache.set_bitswap(&json);
            Some(b)
        }
        Err(e) => { tracing::warn!("Failed to get bitswap stats: {}", e); None }
    };

    let pin_count = match client.pin_ls().await {
        Ok(pins) => pins.pins.len(),
        Err(e) => { tracing::warn!("Failed to get pin count: {}", e); 0 }
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
        Err(DaemonError::ApiError("Proxy client not initialized".to_string()))
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
    let entries = state.offline_queue.list_all()
        .map_err(DaemonError::ConfigError)?;
    let count = state.offline_queue.len()
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
    let client = state.get_api_client().await
        .ok_or(DaemonError::ApiError("API client not initialized".to_string()))?;

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

    tracing::info!("Bandwidth config updated: {} conns / {} streams",
        config.max_connections, config.max_streams);
    Ok(())
}

/// 获取当前带宽速率
#[tauri::command]
pub async fn get_bandwidth_status(
    state: State<'_, AppState>,
) -> Result<crate::bandwidth::BandwidthStatus, DaemonError> {
    let monitor = state.bandwidth_monitor.lock()
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
        let id = state.offline_queue.enqueue(op)
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
pub async fn get_active_backend(
    state: State<'_, AppState>,
) -> Result<String, DaemonError> {
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
        _ => return Err(DaemonError::ConfigError(format!("Unknown backend: {}", backend_type))),
    };

    // 验证目标后端可用
    let available = match new_type {
        crate::backend_trait::BackendType::Kubo => state.kubo_backend.is_available().await,
        crate::backend_trait::BackendType::Iroh => state.iroh_backend.is_available().await,
    };

    if !available {
        return Err(DaemonError::ApiError(format!(
            "Backend '{}' is not available", new_type
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


