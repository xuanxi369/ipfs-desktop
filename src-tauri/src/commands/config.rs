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
pub async fn open_webui(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), DaemonError> {
    ensure_kubo_running(state.clone(), app_handle).await?;
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
