use tauri::State;
use crate::state::AppState;
use crate::config::AppConfig;
use crate::types::DaemonStatus;
use crate::daemon::{BinaryFinder, DaemonController};

/// 获取守护进程状态
#[tauri::command]
pub async fn get_daemon_status(
    state: State<'_, AppState>
) -> Result<DaemonStatus, String> {
    Ok(state.get_daemon_status().await)
}

/// 启动守护进程
#[tauri::command]
pub async fn start_daemon(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let current_status = state.get_daemon_status().await;
    
    // 检查当前状态
    if !matches!(current_status, DaemonStatus::Stopped | DaemonStatus::Failed { .. }) {
        return Err("Daemon is not in stopped state".to_string());
    }
    
    // 更新状态为启动中
    state.set_daemon_status(DaemonStatus::Starting).await;
    
    // 发送事件到前端
    app_handle.emit("daemon-status-changed", &DaemonStatus::Starting)
        .map_err(|e| e.to_string())?;
    
    tracing::info!("Starting IPFS daemon...");
    
    // 查找 IPFS 二进制文件
    let binary_path = match BinaryFinder::find() {
        Some(path) => {
            tracing::info!("Found IPFS binary: {:?}", path);
            path
        }
        None => {
            let error = "Could not find IPFS binary. Please install Kubo or set IPFS_GO_EXEC environment variable.".to_string();
            tracing::error!("{}", error);
            state.set_daemon_status(DaemonStatus::Failed { error: error.clone() }).await;
            app_handle.emit("daemon-status-changed", &state.get_daemon_status().await)
                .map_err(|e| e.to_string())?;
            return Err(error);
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
                            .map_err(|e| e.to_string())?;
                        
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
            
            Ok(())
        }
        Err(e) => {
            tracing::error!("Failed to start daemon: {}", e);
            state.set_daemon_status(DaemonStatus::Failed { error: e.clone() }).await;
            app_handle.emit("daemon-status-changed", &state.get_daemon_status().await)
                .map_err(|e| e.to_string())?;
            Err(e)
        }
    }
}

/// 停止守护进程
#[tauri::command]
pub async fn stop_daemon(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let current_status = state.get_daemon_status().await;
    
    // 检查当前状态
    if matches!(current_status, DaemonStatus::Stopped) {
        return Ok(());
    }
    
    // 更新状态为停止中
    state.set_daemon_status(DaemonStatus::Stopping).await;
    
    // 发送事件到前端
    app_handle.emit("daemon-status-changed", &DaemonStatus::Stopping)
        .map_err(|e| e.to_string())?;
    
    tracing::info!("Stopping IPFS daemon...");
    
    // 获取控制器
    if let Some(controller) = state.get_daemon_controller().await {
        match controller.stop().await {
            Ok(_) => {
                tracing::info!("Daemon stopped successfully");
                state.set_daemon_status(DaemonStatus::Stopped).await;
                state.set_daemon_controller(None).await;
                
                app_handle.emit("daemon-status-changed", &DaemonStatus::Stopped)
                    .map_err(|e| e.to_string())?;
                
                Ok(())
            }
            Err(e) => {
                tracing::error!("Failed to stop daemon: {}", e);
                state.set_daemon_status(DaemonStatus::Failed { error: e.clone() }).await;
                app_handle.emit("daemon-status-changed", &state.get_daemon_status().await)
                    .map_err(|e| e.to_string())?;
                Err(e)
            }
        }
    } else {
        // 没有控制器，直接设置为已停止
        state.set_daemon_status(DaemonStatus::Stopped).await;
        app_handle.emit("daemon-status-changed", &DaemonStatus::Stopped)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// 重启守护进程
#[tauri::command]
pub async fn restart_daemon(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    tracing::info!("Restarting IPFS daemon...");
    
    // 先停止
    if let Err(e) = stop_daemon(state.clone(), app_handle.clone()).await {
        tracing::warn!("Stop daemon failed during restart: {}", e);
    }
    
    // 等待一小段时间
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    // 再启动
    start_daemon(state, app_handle).await
}

/// 获取配置
#[tauri::command]
pub async fn get_config(
    state: State<'_, AppState>
) -> Result<AppConfig, String> {
    Ok(state.get_config().await)
}

/// 更新配置
#[tauri::command]
pub async fn update_config(
    state: State<'_, AppState>,
    new_config: AppConfig,
) -> Result<(), String> {
    // 验证配置
    new_config.validate()
        .map_err(|e| format!("Invalid configuration: {}", e))?;
    
    // 保存配置到磁盘
    new_config.save()
        .map_err(|e| format!("Failed to save configuration: {}", e))?;
    
    // 更新状态
    state.update_config(new_config.clone()).await;
    
    tracing::info!("Configuration updated and saved");
    
    Ok(())
}

/// 获取节点 ID（用于测试 API 连接）
#[tauri::command]
pub async fn get_node_id(
    state: State<'_, AppState>,
) -> Result<String, String> {
    tracing::info!("Getting node ID...");
    
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
        Err("API client not initialized".to_string())
    }
}
