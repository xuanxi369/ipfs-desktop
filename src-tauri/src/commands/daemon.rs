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

async fn ensure_kubo_running(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), DaemonError> {
    if matches!(state.get_daemon_status().await, DaemonStatus::Running { .. }) {
        return Ok(());
    }
    tracing::info!("Starting Kubo on demand for a compatibility operation");
    start_daemon(state, app_handle).await
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
