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

/// 获取当前路由策略（KuboOnly / IrohOnly / Auto / Mirror）
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

#[tauri::command]
pub async fn get_usage_mode(state: State<'_, AppState>) -> Result<String, DaemonError> {
    let config = state.get_config().await;
    let legacy = crate::backend_router::RoutePolicy::parse(&config.route_policy)
        .unwrap_or(crate::backend_router::RoutePolicy::IrohOnly);
    Ok(config
        .usage_mode
        .as_deref()
        .and_then(crate::backend_router::UsageMode::parse)
        .unwrap_or_else(|| crate::backend_router::UsageMode::from_legacy(legacy))
        .to_string())
}

#[tauri::command]
pub async fn set_usage_mode(
    state: State<'_, AppState>,
    mode: String,
) -> Result<String, DaemonError> {
    let mode = crate::backend_router::UsageMode::parse(&mode)
        .ok_or_else(|| DaemonError::ConfigError(format!("Unknown usage mode: {mode}")))?;
    let policy = mode.route_policy();
    state.backend_router.set_policy(policy).await;
    let mut config = state.get_config().await;
    config.usage_mode = Some(mode.to_string());
    config.route_policy = policy.to_string();
    config.save().map_err(DaemonError::ConfigError)?;
    state.update_config(config).await;
    Ok(mode.to_string())
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationStatus {
    pub total: usize,
    pub iroh_native: usize,
    pub kubo_only: usize,
    pub mirrored: usize,
    pub progress_percent: f64,
    pub usage_mode: String,
    pub kubo_running: bool,
    pub ipfs_compatible: bool,
    pub ipns_available: bool,
    pub kubo_role: String,
}

#[tauri::command]
pub async fn get_migration_status(
    state: State<'_, AppState>,
) -> Result<MigrationStatus, DaemonError> {
    let records = state.content_index.list().map_err(DaemonError::IoError)?;
    let mirrored = state.backend_router.mapping_count().await;
    let iroh_native = records
        .iter()
        .filter(|record| record.backend.eq_ignore_ascii_case("iroh"))
        .count();
    let kubo_only = records
        .iter()
        .filter(|record| record.backend.eq_ignore_ascii_case("kubo"))
        .count()
        .saturating_sub(mirrored);
    let total = records.len();
    let migrated = (iroh_native + mirrored).min(total);
    let progress_percent = if total == 0 {
        100.0
    } else {
        migrated as f64 * 100.0 / total as f64
    };
    let kubo_running = matches!(
        state.get_daemon_status().await,
        crate::types::DaemonStatus::Running { .. }
    );
    let usage_mode = get_usage_mode(state.clone()).await?;
    let bridge_enabled = usage_mode != "LocalFirst";
    Ok(MigrationStatus {
        total,
        iroh_native,
        kubo_only,
        mirrored,
        progress_percent,
        usage_mode,
        kubo_running,
        ipfs_compatible: bridge_enabled || kubo_running || kubo_only > 0 || mirrored > 0,
        ipns_available: kubo_running,
        kubo_role: "ipfs_ipns_gateway_compatibility_bridge".to_string(),
    })
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
    let t = state
        .backend_router
        .choose_for_cid(&cid)
        .await
        .map_err(iroh_err)?;
    Ok(t.to_string())
}
