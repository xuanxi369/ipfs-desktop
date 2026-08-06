/// 不接触任何私钥。需要守护进程处于运行状态。
#[tauri::command]
pub async fn generate_key(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    label: String,
) -> Result<crate::keyring::KeyRecord, DaemonError> {
    ensure_kubo_running(state.clone(), app_handle).await?;
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
    app_handle: tauri::AppHandle,
    cid: String,
    key_name: String,
    lifetime: Option<String>,
    ipns_base: Option<String>,
    allow_offline: Option<bool>,
) -> Result<crate::daemon::IpnsPublishResult, DaemonError> {
    ensure_kubo_running(state.clone(), app_handle).await?;
    let cid = validate_ipfs_cid(&cid)?.to_string();
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
    app_handle: tauri::AppHandle,
    name: String,
) -> Result<crate::daemon::IpnsResolveResult, DaemonError> {
    ensure_kubo_running(state.clone(), app_handle).await?;
    tracing::info!("ipns_resolve: {}", name);

    let client = state.get_api_client().await.ok_or(DaemonError::ApiError(
        "API client not initialized".to_string(),
    ))?;

    client.name_resolve(&name).await
}
