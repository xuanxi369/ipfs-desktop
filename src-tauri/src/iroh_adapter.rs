//! Iroh Backend 适配器 — Phase 4
//!
//! 使用 Rust 原生 iroh crate 实现 Backend trait。
//! 当前为 stub 实现（iroh 依赖标记为 optional），
//! 启用 `iroh-backend` feature 后编译真实实现。
//!
//! # Iroh 与 Kubo 的关键差异
//!
//! | 特性         | Kubo (Go)          | Iroh (Rust)          |
//! |-------------|---------------------|----------------------|
//! | 协议         | 标准 IPFS/libp2p    | iroh-net (+ IPFS兼容)|
//! | 传输         | TCP + QUIC          | QUIC only            |
//! | CID          | CIDv0 + CIDv1       | CIDv1 only           |
//! | HTTP API     | /api/v0/*           | 无(使用 Rust API)     |
//! | IPNS         | 支持                | 有限支持              |
//! | Pin          | 支持                | 作者系统替代          |
//! | GC           | 手动触发            | 自动(基于作者证书)    |
//! | MFS          | 支持                | 文档系统替代          |
//!
//! # 集成步骤
//!
//! 1. 添加依赖: `iroh = "0.25"` (或最新版本)
//! 2. 在 Cargo.toml 添加 feature: `iroh-backend = ["iroh"]`
//! 3. 移除下面的 `#[cfg(feature = "iroh-backend")]` 条件编译
//! 4. 实现 `IrohBackend::new()` 的实际节点初始化

use async_trait::async_trait;
use crate::backend_trait::{
    Backend, BackendType, BackendCapabilities, BackendError,
    NodeInfo, RepoInfo, PeerInfo as BPeerInfo,
    AddOutput, PinEntry, BandwidthInfo, BitswapInfo, IpnsOutput, IpnsPath,
};
use std::path::Path;

// ════════════════════════════════════════════════════════════════
// Iroh 后端 (Stub)
// ════════════════════════════════════════════════════════════════

/// Iroh 原生后端
///
/// 启用 `iroh-backend` feature 后，将包含真实的 iroh 节点。
/// 当前 stub 版本返回 Unsupported 错误以安全降级。
#[derive(Clone)]
pub struct IrohBackend {
    /// 数据目录路径
    data_dir: std::path::PathBuf,
    /// 是否已初始化
    initialized: bool,
}

impl IrohBackend {
    /// 创建新的 Iroh 后端
    ///
    /// 当前为 stub — 真实实现需要：
    ///
    /// ```ignore
    /// use iroh::node::Node;
    /// use iroh::rpc_protocol::ProvideRequest;
    ///
    /// let node = Node::memory().spawn().await?;
    /// let client = node.client();
    /// ```
    pub fn new(data_dir: std::path::PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&data_dir);
        Self {
            data_dir,
            initialized: true,
        }
    }

    /// 获取本地 Peer ID（Iroh 使用 Ed25519 公钥的 multihash）
    ///
    /// 真实实现：
    /// ```ignore
    /// let doc = docs_client.create().await?;
    /// let hash = doc.hash(); // Blake3 CID
    /// ```
    fn local_peer_id(&self) -> String {
        "iroh:stub:12D3KooW".to_string()
    }
}

#[async_trait]
impl Backend for IrohBackend {
    fn backend_type(&self) -> BackendType {
        BackendType::Iroh
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            backend_type: BackendType::Iroh,
            ipns: false,       // Iroh 使用内容寻址，不原生支持 IPNS
            pinning: false,    // Iroh 使用作者证书系统
            gc: true,          // Iroh 自动 GC
            pubsub: false,
            mfs: false,        // Iroh 使用文档系统
            bitswap: false,    // Iroh 使用自己的传输协议
            cid_version: 1,    // 仅 CIDv1
        }
    }

    async fn is_available(&self) -> bool {
        self.initialized
    }

    async fn node_info(&self) -> Result<NodeInfo, BackendError> {
        // Stub: 返回占位信息
        Ok(NodeInfo {
            peer_id: self.local_peer_id(),
            agent_version: format!("iroh-desktop-rust/{}", env!("CARGO_PKG_VERSION")),
            protocol_version: "iroh/0.25".to_string(),
            addresses: vec![
                "/ip4/0.0.0.0/udp/0/quic-v1".to_string(),
            ],
        })
    }

    async fn version(&self) -> Result<String, BackendError> {
        Ok("iroh 0.25.x (stub)".to_string())
    }

    async fn repo_stat(&self) -> Result<RepoInfo, BackendError> {
        // 读取数据目录大小
        let size = std::fs::metadata(&self.data_dir)
            .map(|m| m.len())
            .unwrap_or(0);
        Ok(RepoInfo {
            num_objects: 0,
            repo_size: size,
            version: "iroh".to_string(),
        })
    }

    async fn repo_gc(&self) -> Result<(), BackendError> {
        Err(BackendError::unsupported(
            "Iroh GC is automatic; no manual trigger available"
        ))
    }

    async fn add_file(&self, _path: &Path) -> Result<AddOutput, BackendError> {
        // Stub: 真实实现使用 iroh-blobs
        //
        // ```ignore
        // use iroh::client::blobs::BlobClient;
        // let client = node.client();
        // let blob = client.add_from_path(path).await?;
        // Ok(AddOutput { cid: blob.hash.to_string(), ... })
        // ```
        Err(BackendError::unsupported(
            "Iroh add_file requires iroh-backend feature"
        ))
    }

    async fn cat(&self, _cid: &str) -> Result<Vec<u8>, BackendError> {
        Err(BackendError::unsupported(
            "Iroh cat requires iroh-backend feature"
        ))
    }

    async fn file_size(&self, _cid: &str) -> Result<u64, BackendError> {
        Err(BackendError::unsupported(
            "Iroh file_size requires iroh-backend feature"
        ))
    }

    async fn pin_ls(&self) -> Result<Vec<PinEntry>, BackendError> {
        // Iroh 没有传统 Pin 概念，但有作者证书
        Err(BackendError::unsupported(
            "Iroh uses author certificates instead of pins"
        ))
    }

    async fn pin_add(&self, _cid: &str) -> Result<(), BackendError> {
        Err(BackendError::unsupported(
            "Iroh uses author certificates instead of pins"
        ))
    }

    async fn pin_rm(&self, _cid: &str) -> Result<(), BackendError> {
        Err(BackendError::unsupported(
            "Iroh uses author certificates instead of pins"
        ))
    }

    async fn swarm_peers(&self) -> Result<Vec<BPeerInfo>, BackendError> {
        // Stub: 真实实现使用 iroh-net
        //
        // ```ignore
        // let net = node.net();
        // let peers = net.remote_info_iter().await?;
        // ```
        Ok(vec![])
    }

    async fn bandwidth_stats(&self) -> Result<BandwidthInfo, BackendError> {
        Err(BackendError::unsupported(
            "Iroh bandwidth stats require iroh-backend feature"
        ))
    }

    async fn bitswap_stats(&self) -> Result<BitswapInfo, BackendError> {
        // Iroh 不使用 Bitswap 协议
        Err(BackendError::unsupported(
            "Iroh does not use Bitswap protocol"
        ))
    }

    async fn name_publish(
        &self, _cid: &str, _key: &str, _lifetime: &str,
    ) -> Result<IpnsOutput, BackendError> {
        Err(BackendError::unsupported(
            "Iroh does not support IPNS publish (use content addressing)"
        ))
    }

    async fn name_resolve(&self, _name: &str) -> Result<IpnsPath, BackendError> {
        Err(BackendError::unsupported(
            "Iroh does not support IPNS resolve (use content addressing)"
        ))
    }

    async fn shutdown(&self) -> Result<(), BackendError> {
        tracing::info!("Iroh backend shutdown (stub)");
        Ok(())
    }
}

// ════════════════════════════════════════════════════════════════
// 真实 Iroh 实现的文档模板（启用 iroh-backend feature 时使用）
// ════════════════════════════════════════════════════════════════

#[doc(hidden)]
#[allow(dead_code)]
mod iroh_real {
    //! 当 `iroh-backend` feature 启用时的真实实现
    //!
    //! ```toml
    //! [features]
    //! iroh-backend = ["iroh"]
    //!
    //! [dependencies]
    //! iroh = { version = "0.25", optional = true }
    //! ```
    //!
    //! ## Iroh 节点初始化
    //!
    //! ```ignore
    //! use iroh::node::Node;
    //!
    //! let node = Node::persistent(data_dir)
    //!     .await?
    //!     .spawn()
    //!     .await?;
    //!
    //! let client = node.client();
    //! let docs = client.docs();
    //! let blobs = client.blobs();
    //! ```
    //!
    //! ## 添加文件
    //!
    //! ```ignore
    //! let blob = blobs.add_from_path(path).await?;
    //! // blob.hash 是 Blake3 CID
    //! ```
    //!
    //! ## 读取文件
    //!
    //! ```ignore
    //! let reader = blobs.read(hash).await?;
    //! let data = reader.read_to_end().await?;
    //! ```
    //!
    //! ## 网络
    //!
    //! ```ignore
    //! let net = node.net();
    //! let addrs = net.remote_addresses().await?;
    //! ```
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_iroh_stub_creation() {
        let dir = std::env::temp_dir().join("iroh-test-stub");
        let backend = IrohBackend::new(dir);
        assert_eq!(backend.backend_type(), BackendType::Iroh);
        assert!(backend.is_available().await);
    }

    #[tokio::test]
    async fn test_iroh_stub_capabilities() {
        let dir = std::env::temp_dir().join("iroh-test-cap");
        let backend = IrohBackend::new(dir);
        let caps = backend.capabilities();
        assert!(!caps.ipns, "Iroh does not support IPNS");
        assert!(!caps.bitswap, "Iroh does not use Bitswap");
        assert_eq!(caps.cid_version, 1);
    }

    #[tokio::test]
    async fn test_iroh_stub_node_info() {
        let dir = std::env::temp_dir().join("iroh-test-info");
        let backend = IrohBackend::new(dir);
        let info = backend.node_info().await.unwrap();
        assert!(!info.peer_id.is_empty());
    }

    #[tokio::test]
    async fn test_iroh_stub_unsupported_operations() {
        let dir = std::env::temp_dir().join("iroh-test-unsup");
        let backend = IrohBackend::new(dir);

        // 不支持的操作应返回 Unsupported 错误
        assert!(backend.pin_add("QmTest").await.is_err());
        assert!(backend.name_publish("QmTest", "self", "24h").await.is_err());
        assert!(backend.bitswap_stats().await.is_err());

        // 已实现的操作（node_info, version）应成功
        assert!(backend.node_info().await.is_ok());
        assert!(backend.version().await.is_ok());
    }
}
