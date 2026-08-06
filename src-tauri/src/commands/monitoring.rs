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
