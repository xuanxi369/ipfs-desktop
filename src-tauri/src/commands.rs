use crate::backend_trait::{Backend, BackendType};
use crate::config::AppConfig;
use crate::daemon::{
    BandwidthStats, BitswapStats, IpfsApiClient, NodeId, PinAddResult, PinList, PinRmResult,
    RepoStats, SwarmPeers,
};
use crate::error::DaemonError;
use crate::path_security::validate_cid as validate_ipfs_cid;
use crate::state::AppState;
use crate::types::DaemonStatus;
use serde::Serialize;
use tauri::Emitter;
use tauri::State;

include!("commands/daemon.rs");
include!("commands/config.rs");
include!("commands/content.rs");
include!("commands/ipns.rs");
include!("commands/monitoring.rs");
include!("commands/iroh.rs");
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

include!("commands/identity.rs");
