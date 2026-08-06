//! 统一 IPFS Backend Trait — Phase 4 核心
//!
//! 定义所有 IPFS 后端（Kubo、Iroh、未来可能的其他实现）的统一接口。
//! 这使得上层代码（commands.rs、proxy.rs）无需关心底层是 Go 还是 Rust 实现。
//!
//! 设计原则：
//! - 所有操作为 async（适配不同后端的异步模型）
//! - 错误类型统一为 `BackendError`
//! - 读写分离：只读操作用 `&self`，写入操作用 `&self`（内部加锁）
//! - 支持后端能力查询（feature flags）

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ════════════════════════════════════════════════════════════════
// 错误类型
// ════════════════════════════════════════════════════════════════

/// 后端无关的统一错误类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendError {
    pub kind: BackendErrorKind,
    pub message: String,
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}] {}", self.kind, self.message)
    }
}

impl std::error::Error for BackendError {}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BackendErrorKind {
    /// 后端不可用（未启动/已崩溃）
    Unavailable,
    /// 操作超时
    Timeout,
    /// 无效参数
    InvalidArgument,
    /// 内容未找到
    NotFound,
    /// 内部错误
    Internal,
    /// 不支持的操作
    Unsupported,
    /// 网络错误
    Network,
}

impl BackendError {
    pub fn invalid_argument(msg: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::InvalidArgument,
            message: msg.into(),
        }
    }
    pub fn unavailable(msg: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::Unavailable,
            message: msg.into(),
        }
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::NotFound,
            message: msg.into(),
        }
    }
    pub fn unsupported(msg: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::Unsupported,
            message: msg.into(),
        }
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::Internal,
            message: msg.into(),
        }
    }
    pub fn network(msg: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::Network,
            message: msg.into(),
        }
    }
}

// ════════════════════════════════════════════════════════════════
// 通用数据类型
// ════════════════════════════════════════════════════════════════

/// 节点标识信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub peer_id: String,
    pub agent_version: String,
    pub protocol_version: String,
    pub addresses: Vec<String>,
}

/// 仓库统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    pub num_objects: u64,
    pub repo_size: u64,
    pub version: String,
}

/// 对等节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub address: String,
    pub direction: Option<String>,
}

/// 文件添加结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddOutput {
    pub cid: String,
    pub size: u64,
    pub name: String,
}

/// Pin 条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinEntry {
    pub cid: String,
    pub pin_type: String, // "recursive" | "direct" | "indirect"
}

/// 带宽统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthInfo {
    pub total_in: u64,
    pub total_out: u64,
    pub rate_in: f64,
    pub rate_out: f64,
}

/// Bitswap 统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitswapInfo {
    pub blocks_received: u64,
    pub blocks_sent: u64,
    pub data_received: u64,
    pub data_sent: u64,
}

/// IPNS 发布结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpnsOutput {
    pub name: String,
    pub value: String,
}

/// IPNS 解析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpnsPath {
    pub path: String,
}

/// 后端类型标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackendType {
    Kubo,
    Iroh,
}

/// A backend-qualified, semantically validated content identifier.
///
/// IPFS references are parsed as real CIDs. Iroh hashes must be explicitly
/// qualified (or supplied by a trusted backend result), so routing never has
/// to infer a backend from a string prefix.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ContentRef {
    Ipfs(cid::Cid),
    Iroh(String),
}

impl ContentRef {
    pub fn parse(value: &str) -> Result<Self, BackendError> {
        let value = value.trim();
        if let Some(cid) = value.strip_prefix("ipfs:") {
            return cid
                .parse::<cid::Cid>()
                .map(Self::Ipfs)
                .map_err(|e| BackendError::invalid_argument(format!("invalid IPFS CID: {e}")));
        }
        if let Some(hash) = value.strip_prefix("iroh:") {
            return Self::iroh(hash);
        }
        value.parse::<cid::Cid>().map(Self::Ipfs).map_err(|_| {
            BackendError::invalid_argument(
                "ambiguous content reference; use iroh:<hash> for iroh content",
            )
        })
    }

    pub fn from_backend(value: &str, backend: BackendType) -> Result<Self, BackendError> {
        match backend {
            BackendType::Kubo => value.parse::<cid::Cid>().map(Self::Ipfs).map_err(|e| {
                BackendError::invalid_argument(format!("backend returned invalid CID: {e}"))
            }),
            BackendType::Iroh => Self::iroh(value),
        }
    }

    fn iroh(value: &str) -> Result<Self, BackendError> {
        let value = value.trim();
        if value.is_empty()
            || value.len() > 256
            || !value.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return Err(BackendError::invalid_argument("invalid iroh content hash"));
        }
        Ok(Self::Iroh(value.to_owned()))
    }

    pub fn backend_type(&self) -> BackendType {
        match self {
            Self::Ipfs(_) => BackendType::Kubo,
            Self::Iroh(_) => BackendType::Iroh,
        }
    }

    pub fn value(&self) -> String {
        match self {
            Self::Ipfs(cid) => cid.to_string(),
            Self::Iroh(hash) => hash.clone(),
        }
    }
}

impl std::fmt::Display for ContentRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ipfs(cid) => write!(f, "ipfs:{cid}"),
            Self::Iroh(hash) => write!(f, "iroh:{hash}"),
        }
    }
}

impl std::fmt::Display for BackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendType::Kubo => write!(f, "Kubo (Go)"),
            BackendType::Iroh => write!(f, "Iroh (Rust)"),
        }
    }
}

/// 后端能力标志
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendCapabilities {
    pub backend_type: BackendType,
    /// 是否支持 IPNS
    pub ipns: bool,
    /// 是否支持 Pin 管理
    pub pinning: bool,
    /// 是否支持垃圾回收
    pub gc: bool,
    /// 是否支持 pubsub
    pub pubsub: bool,
    /// 是否支持 MFS
    pub mfs: bool,
    /// 是否支持 Bitswap
    pub bitswap: bool,
    /// CID 版本
    pub cid_version: u8,
}

/// Minimal identity and health capability shared by every backend.
#[async_trait]
pub trait BackendIdentity: Send + Sync {
    fn backend_type(&self) -> BackendType;
    fn capabilities(&self) -> BackendCapabilities;
    async fn is_available(&self) -> bool;
}

#[async_trait]
pub trait NodeBackend: Send + Sync {
    async fn node_info(&self) -> Result<NodeInfo, BackendError>;
    async fn version(&self) -> Result<String, BackendError>;
}

#[async_trait]
pub trait RepositoryBackend: Send + Sync {
    async fn repo_stat(&self) -> Result<RepoInfo, BackendError>;
    async fn repo_gc(&self) -> Result<(), BackendError>;
}

/// Content-addressed read/write capability used by the router.
#[async_trait]
pub trait ContentBackend: Send + Sync {
    async fn add_file(&self, path: &Path) -> Result<AddOutput, BackendError>;
    async fn cat(&self, reference: &str) -> Result<Vec<u8>, BackendError>;
    async fn file_size(&self, reference: &str) -> Result<u64, BackendError>;
}

/// Pin/retention capability. Backends without this capability can keep
/// returning `Unsupported` through the compatibility `Backend` trait.
#[async_trait]
pub trait PinningBackend: Send + Sync {
    async fn pin_ls(&self) -> Result<Vec<PinEntry>, BackendError>;
    async fn pin_add(&self, reference: &str) -> Result<(), BackendError>;
    async fn pin_rm(&self, reference: &str) -> Result<(), BackendError>;
}

/// Naming capability (IPNS on Kubo; optional for future native backends).
#[async_trait]
pub trait NamingBackend: Send + Sync {
    async fn name_publish(
        &self,
        reference: &str,
        key_name: &str,
        lifetime: &str,
    ) -> Result<IpnsOutput, BackendError>;
    async fn name_resolve(&self, name: &str) -> Result<IpnsPath, BackendError>;
}

#[async_trait]
pub trait NetworkBackend: Send + Sync {
    async fn swarm_peers(&self) -> Result<Vec<PeerInfo>, BackendError>;
    async fn bandwidth_stats(&self) -> Result<BandwidthInfo, BackendError>;
    async fn bitswap_stats(&self) -> Result<BitswapInfo, BackendError>;
}

#[async_trait]
pub trait LifecycleBackend: Send + Sync {
    async fn shutdown(&self) -> Result<(), BackendError>;
}

// ════════════════════════════════════════════════════════════════
// Backend Trait
// ════════════════════════════════════════════════════════════════

/// IPFS 后端统一接口
///
/// 所有 IPFS 后端实现（Kubo HTTP、Iroh 原生、未来 Wasm 等）
/// 必须实现此 trait。
///
/// # 使用示例
///
/// ```ignore
/// let backend: Box<dyn Backend> = if use_iroh {
///     Box::new(IrohBackend::new().await?)
/// } else {
///     Box::new(KuboBackend::new("http://127.0.0.1:5001"))
/// };
///
/// let info = backend.node_info().await?;
/// println!("Peer ID: {}", info.peer_id);
/// ```
#[async_trait]
pub trait Backend: Send + Sync {
    // ── 元数据 ──

    /// 后端类型标识
    fn backend_type(&self) -> BackendType;

    /// 获取后端能力
    fn capabilities(&self) -> BackendCapabilities;

    /// 检查后端是否可用
    async fn is_available(&self) -> bool;

    // ── 节点信息 ──

    /// 获取节点标识信息
    async fn node_info(&self) -> Result<NodeInfo, BackendError>;

    /// 获取版本信息
    async fn version(&self) -> Result<String, BackendError>;

    // ── 仓库 ──

    /// 获取仓库统计
    async fn repo_stat(&self) -> Result<RepoInfo, BackendError>;

    /// 触发垃圾回收
    async fn repo_gc(&self) -> Result<(), BackendError>;

    // ── 文件操作 ──

    /// 添加文件到 IPFS
    async fn add_file(&self, path: &Path) -> Result<AddOutput, BackendError>;

    /// 从 IPFS 读取文件内容
    async fn cat(&self, cid: &str) -> Result<Vec<u8>, BackendError>;

    /// 获取文件大小
    async fn file_size(&self, cid: &str) -> Result<u64, BackendError>;

    // ── Pin 管理 ──

    /// 列出所有 Pins
    async fn pin_ls(&self) -> Result<Vec<PinEntry>, BackendError>;

    /// 添加 Pin
    async fn pin_add(&self, cid: &str) -> Result<(), BackendError>;

    /// 移除 Pin
    async fn pin_rm(&self, cid: &str) -> Result<(), BackendError>;

    // ── 网络 ──

    /// 获取连接的节点列表
    async fn swarm_peers(&self) -> Result<Vec<PeerInfo>, BackendError>;

    /// 获取带宽统计
    async fn bandwidth_stats(&self) -> Result<BandwidthInfo, BackendError>;

    /// 获取 Bitswap 统计
    async fn bitswap_stats(&self) -> Result<BitswapInfo, BackendError>;

    // ── IPNS ──

    /// 发布 IPNS 名称
    async fn name_publish(
        &self,
        cid: &str,
        key_name: &str,
        lifetime: &str,
    ) -> Result<IpnsOutput, BackendError>;

    /// 解析 IPNS 名称
    async fn name_resolve(&self, name: &str) -> Result<IpnsPath, BackendError>;

    // ── 生命周期 ──

    /// 关闭后端
    async fn shutdown(&self) -> Result<(), BackendError>;
}

// Transitional adapters keep existing backend implementations source-compatible
// while allowing new code to depend only on the capability it actually uses.
#[async_trait]
impl<T: Backend + ?Sized> BackendIdentity for T {
    fn backend_type(&self) -> BackendType {
        Backend::backend_type(self)
    }
    fn capabilities(&self) -> BackendCapabilities {
        Backend::capabilities(self)
    }
    async fn is_available(&self) -> bool {
        Backend::is_available(self).await
    }
}

#[async_trait]
impl<T: Backend + ?Sized> NodeBackend for T {
    async fn node_info(&self) -> Result<NodeInfo, BackendError> {
        Backend::node_info(self).await
    }
    async fn version(&self) -> Result<String, BackendError> {
        Backend::version(self).await
    }
}

#[async_trait]
impl<T: Backend + ?Sized> RepositoryBackend for T {
    async fn repo_stat(&self) -> Result<RepoInfo, BackendError> {
        Backend::repo_stat(self).await
    }
    async fn repo_gc(&self) -> Result<(), BackendError> {
        Backend::repo_gc(self).await
    }
}

#[async_trait]
impl<T: Backend + ?Sized> ContentBackend for T {
    async fn add_file(&self, path: &Path) -> Result<AddOutput, BackendError> {
        Backend::add_file(self, path).await
    }
    async fn cat(&self, reference: &str) -> Result<Vec<u8>, BackendError> {
        Backend::cat(self, reference).await
    }
    async fn file_size(&self, reference: &str) -> Result<u64, BackendError> {
        Backend::file_size(self, reference).await
    }
}

#[async_trait]
impl<T: Backend + ?Sized> PinningBackend for T {
    async fn pin_ls(&self) -> Result<Vec<PinEntry>, BackendError> {
        Backend::pin_ls(self).await
    }
    async fn pin_add(&self, reference: &str) -> Result<(), BackendError> {
        Backend::pin_add(self, reference).await
    }
    async fn pin_rm(&self, reference: &str) -> Result<(), BackendError> {
        Backend::pin_rm(self, reference).await
    }
}

#[async_trait]
impl<T: Backend + ?Sized> NamingBackend for T {
    async fn name_publish(
        &self,
        reference: &str,
        key_name: &str,
        lifetime: &str,
    ) -> Result<IpnsOutput, BackendError> {
        Backend::name_publish(self, reference, key_name, lifetime).await
    }
    async fn name_resolve(&self, name: &str) -> Result<IpnsPath, BackendError> {
        Backend::name_resolve(self, name).await
    }
}

#[async_trait]
impl<T: Backend + ?Sized> NetworkBackend for T {
    async fn swarm_peers(&self) -> Result<Vec<PeerInfo>, BackendError> {
        Backend::swarm_peers(self).await
    }
    async fn bandwidth_stats(&self) -> Result<BandwidthInfo, BackendError> {
        Backend::bandwidth_stats(self).await
    }
    async fn bitswap_stats(&self) -> Result<BitswapInfo, BackendError> {
        Backend::bitswap_stats(self).await
    }
}

#[async_trait]
impl<T: Backend + ?Sized> LifecycleBackend for T {
    async fn shutdown(&self) -> Result<(), BackendError> {
        Backend::shutdown(self).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用 stub 后端
    struct StubBackend {
        available: bool,
    }

    #[async_trait]
    impl Backend for StubBackend {
        fn backend_type(&self) -> BackendType {
            BackendType::Kubo
        }
        fn capabilities(&self) -> BackendCapabilities {
            BackendCapabilities {
                backend_type: BackendType::Kubo,
                ipns: true,
                pinning: true,
                gc: true,
                pubsub: false,
                mfs: false,
                bitswap: true,
                cid_version: 1,
            }
        }
        async fn is_available(&self) -> bool {
            self.available
        }
        async fn node_info(&self) -> Result<NodeInfo, BackendError> {
            Ok(NodeInfo {
                peer_id: "stub".into(),
                agent_version: "test".into(),
                protocol_version: "test".into(),
                addresses: vec![],
            })
        }
        async fn version(&self) -> Result<String, BackendError> {
            Ok("stub-0.1".into())
        }
        async fn repo_stat(&self) -> Result<RepoInfo, BackendError> {
            Ok(RepoInfo {
                num_objects: 0,
                repo_size: 0,
                version: "stub".into(),
            })
        }
        async fn repo_gc(&self) -> Result<(), BackendError> {
            Ok(())
        }
        async fn add_file(&self, _path: &Path) -> Result<AddOutput, BackendError> {
            Ok(AddOutput {
                cid: "QmStub".into(),
                size: 0,
                name: "stub".into(),
            })
        }
        async fn cat(&self, _cid: &str) -> Result<Vec<u8>, BackendError> {
            Ok(vec![])
        }
        async fn file_size(&self, _cid: &str) -> Result<u64, BackendError> {
            Ok(0)
        }
        async fn pin_ls(&self) -> Result<Vec<PinEntry>, BackendError> {
            Ok(vec![])
        }
        async fn pin_add(&self, _cid: &str) -> Result<(), BackendError> {
            Ok(())
        }
        async fn pin_rm(&self, _cid: &str) -> Result<(), BackendError> {
            Ok(())
        }
        async fn swarm_peers(&self) -> Result<Vec<PeerInfo>, BackendError> {
            Ok(vec![])
        }
        async fn bandwidth_stats(&self) -> Result<BandwidthInfo, BackendError> {
            Ok(BandwidthInfo {
                total_in: 0,
                total_out: 0,
                rate_in: 0.0,
                rate_out: 0.0,
            })
        }
        async fn bitswap_stats(&self) -> Result<BitswapInfo, BackendError> {
            Ok(BitswapInfo {
                blocks_received: 0,
                blocks_sent: 0,
                data_received: 0,
                data_sent: 0,
            })
        }
        async fn name_publish(
            &self,
            _cid: &str,
            _key: &str,
            _lt: &str,
        ) -> Result<IpnsOutput, BackendError> {
            Ok(IpnsOutput {
                name: "stub".into(),
                value: "stub".into(),
            })
        }
        async fn name_resolve(&self, _name: &str) -> Result<IpnsPath, BackendError> {
            Ok(IpnsPath {
                path: "/ipfs/QmStub".into(),
            })
        }
        async fn shutdown(&self) -> Result<(), BackendError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_stub_backend() {
        let b = StubBackend { available: true };
        assert!(BackendIdentity::is_available(&b).await);
        assert_eq!(BackendIdentity::backend_type(&b), BackendType::Kubo);
        let info = NodeBackend::node_info(&b).await.unwrap();
        assert_eq!(info.peer_id, "stub");
    }

    #[tokio::test]
    async fn test_stub_unavailable() {
        let b = StubBackend { available: false };
        assert!(!BackendIdentity::is_available(&b).await);
    }

    #[test]
    fn test_backend_error_display() {
        let err = BackendError::unavailable("test error");
        assert!(err.to_string().contains("Unavailable"));
        assert!(err.to_string().contains("test error"));
    }

    #[test]
    fn test_backend_type_display() {
        assert_eq!(BackendType::Kubo.to_string(), "Kubo (Go)");
        assert_eq!(BackendType::Iroh.to_string(), "Iroh (Rust)");
    }
}
