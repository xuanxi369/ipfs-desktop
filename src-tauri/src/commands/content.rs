/// 接受本地文件路径，上传到 IPFS 并返回哈希。
#[tauri::command]
pub async fn add_file(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
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
    let policy = state.backend_router.policy().await;
    let selected = state.backend_router.choose_for_add(prefer_backend).await;
    if matches!(selected, BackendType::Kubo)
        || matches!(policy, crate::backend_router::RoutePolicy::Mirror)
    {
        ensure_kubo_running(state.clone(), app_handle.clone()).await?;
    }
    if prefer_backend.is_none()
        && matches!(
            state.backend_router.policy().await,
            crate::backend_router::RoutePolicy::Mirror
        )
    {
        let (_, out) = state
            .backend_router
            .add_file(&path, None)
            .await
            .map_err(iroh_err)?;
        return Ok(crate::types::AddResult {
            hash: out.cid,
            size: out.size.to_string(),
            name: out.name,
        });
    }
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
    app_handle: tauri::AppHandle,
    file_paths: Vec<String>,
) -> Result<Vec<crate::types::AddResult>, DaemonError> {
    validate_path_count(&file_paths)?;
    let mut results = Vec::new();
    for file_path in &file_paths {
        results.push(
            add_file(
                state.clone(),
                app_handle.clone(),
                file_path.clone(),
                None,
            )
            .await?,
        );
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
pub async fn cat_file(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    cid: String,
) -> Result<Vec<u8>, DaemonError> {
    let cid = normalize_content_reference(&state, &cid).await?;
    tracing::info!("cat_file: {}", cid);

    if matches!(
        state.backend_router.policy().await,
        crate::backend_router::RoutePolicy::Mirror
    ) {
        ensure_kubo_running(state.clone(), app_handle).await?;
        return state
            .backend_router
            .cat(&cid)
            .await
            .map(|(_, bytes)| bytes)
            .map_err(iroh_err);
    }

    // 双栈韧性：按路由顺序尝试（Auto 下主后端取不到 → 自动 fallback 到另一个）。
    // Kubo 腿仍用实时 api_client（尊重运行时改地址）。
    let order = state
        .backend_router
        .cat_order(&cid)
        .await
        .map_err(iroh_err)?;
    let mut first_err: Option<DaemonError> = None;
    for (i, backend) in order.iter().enumerate() {
        if matches!(backend, BackendType::Kubo) {
            ensure_kubo_running(state.clone(), app_handle.clone()).await?;
        }
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
    let cid = normalize_content_reference(&state, &cid).await?;
    tracing::info!("download_file: {} -> {}", cid, save_path);

    let output = std::path::PathBuf::from(&save_path);
    validate_cid(&cid)?;
    validate_output_path(&output)?;
    let backend = state
        .backend_router
        .choose_for_cid(&cid)
        .await
        .map_err(iroh_err)?;
    let written = match backend {
        BackendType::Kubo => {
            ensure_kubo_running(state.clone(), app_handle.clone()).await?;
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
pub async fn get_file_size(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    cid: String,
) -> Result<u64, DaemonError> {
    let cid = normalize_content_reference(&state, &cid).await?;
    match state
        .backend_router
        .choose_for_cid(&cid)
        .await
        .map_err(iroh_err)?
    {
        BackendType::Kubo => {
            ensure_kubo_running(state.clone(), app_handle).await?;
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

async fn normalize_content_reference(
    state: &AppState,
    value: &str,
) -> Result<String, DaemonError> {
    if let Ok(cid) = validate_ipfs_cid(value) {
        return Ok(cid.to_string());
    }
    validate_cid(value)?;
    match state
        .backend_router
        .choose_for_cid(value)
        .await
        .map_err(iroh_err)?
    {
        BackendType::Iroh => Ok(value.trim().to_string()),
        BackendType::Kubo => Err(DaemonError::ApiError(
            "invalid IPFS CID for Kubo compatibility bridge".to_string(),
        )),
    }
}

// ════════════════════════════════════════════════════════════════
// A2: Pin 管理
// ════════════════════════════════════════════════════════════════

/// 获取 Pin 列表
#[tauri::command]
pub async fn get_pin_list(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<PinList, DaemonError> {
    tracing::info!("get_pin_list");
    ensure_kubo_running(state.clone(), app_handle).await?;

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
pub async fn add_pin(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    cid: String,
) -> Result<PinAddResult, DaemonError> {
    ensure_kubo_running(state.clone(), app_handle).await?;
    let cid = validate_ipfs_cid(&cid)?.to_string();
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
    app_handle: tauri::AppHandle,
    cid: String,
) -> Result<PinRmResult, DaemonError> {
    ensure_kubo_running(state.clone(), app_handle).await?;
    let cid = validate_ipfs_cid(&cid)?.to_string();
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

    let policy = state.backend_router.policy().await;
    if !matches!(policy, crate::backend_router::RoutePolicy::KuboOnly) {
        let total = tokio::fs::metadata(&path)
            .await
            .map_err(|e| DaemonError::IoError(e.to_string()))?
            .len();
        let _ = app_handle.emit(
            "upload-progress",
            &UploadProgress {
                name: file_name.clone(),
                loaded: 0,
                total,
            },
        );
        let result = add_file(
            state.clone(),
            app_handle.clone(),
            file_path.clone(),
            None,
        )
        .await?;
        let _ = app_handle.emit(
            "upload-progress",
            &UploadProgress {
                name: file_name,
                loaded: total,
                total,
            },
        );
        state
            .content_index
            .upsert(&crate::content_index::ContentRecord {
                cid: result.hash.clone(),
                name: result.name.clone(),
                size: result.size.parse().unwrap_or(total),
                backend: if matches!(policy, crate::backend_router::RoutePolicy::Mirror) {
                    "Mirror".to_string()
                } else {
                    "Iroh".to_string()
                },
                added_at: crate::state::now_secs() as i64,
            })
            .map_err(DaemonError::IoError)?;
        return Ok(result);
    }

    ensure_kubo_running(state.clone(), app_handle.clone()).await?;

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
