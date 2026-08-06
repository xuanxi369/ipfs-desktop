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
