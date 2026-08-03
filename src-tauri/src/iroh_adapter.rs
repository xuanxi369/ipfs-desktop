//! Iroh Backend 适配器 — Phase 4 / Phase B
//!
//! 使用 Rust 原生 iroh 栈实现 Backend trait。
//!
//! 本文件有两套实现，由 `iroh-backend` feature 切换：
//!
//! - **默认（无 feature）**：`IrohBackend` 是 stub，仅 `node_info`/`version` 可用，
//!   其余操作返回 `Unsupported`，用于让主程序在不拉入 iroh 重依赖时也能编译。
//! - **`iroh-backend` feature**：`IrohBackend` 是**真实实现**——惰性 spawn 一个
//!   持久化的 iroh 节点（`iroh-blobs` 的 `FsStore` + `iroh` 的 `Endpoint`），
//!   实装 `node_info` / `add_file` / `cat`（本机内容寻址 add→cat 往返）。
//!
//! # Iroh 与 Kubo 的关键差异
//!
//! | 特性         | Kubo (Go)          | Iroh (Rust)          |
//! |-------------|---------------------|----------------------|
//! | 协议         | 标准 IPFS/libp2p    | iroh (QUIC + relay)  |
//! | CID          | CIDv0 + CIDv1       | BLAKE3 hash          |
//! | HTTP API     | /api/v0/*           | 无(原生 Rust API)     |
//! | IPNS         | 支持                | 不支持(内容寻址)      |
//! | Pin          | 支持                | tags / 作者系统替代   |
//! | Bitswap      | 支持                | 自有传输协议          |
//!
//! # iroh 1.0 API 说明（本实现依据）
//!
//! iroh 1.0 起，blobs 能力从主 crate 拆到独立的 `iroh-blobs`。本机存取只需
//! 一个 store，无需 Router/网络：
//!
//! ```ignore
//! use iroh_blobs::store::fs::FsStore;
//! let store = FsStore::load(dir).await?;
//! let tag  = store.add_path(path).await?;   // tag.hash 是 BLAKE3 内容哈希
//! let bytes = store.get_bytes(tag.hash).await?;
//! ```

use crate::backend_trait::{
    AddOutput, Backend, BackendCapabilities, BackendError, BackendType, BandwidthInfo, BitswapInfo,
    IpnsOutput, IpnsPath, NodeInfo, PeerInfo as BPeerInfo, PinEntry, RepoInfo,
};
use async_trait::async_trait;
use std::path::Path;

/// 从 BlobTicket 字符串中解析出内容 hash（cid）。
///
/// 启用 feature 时真实解析；否则返回 `None`（无 iroh 依赖时不可解析）。
/// 用于收取内容后为其打上 iroh 来源标记。
#[cfg(feature = "iroh-backend")]
pub fn ticket_cid(ticket: &str) -> Option<String> {
    ticket
        .trim()
        .parse::<iroh_blobs::ticket::BlobTicket>()
        .ok()
        .map(|t| t.hash().to_string())
}

/// 从 BlobTicket 字符串中解析出内容 hash（cid）—— stub：无 iroh 依赖时不可解析。
#[cfg(not(feature = "iroh-backend"))]
pub fn ticket_cid(_ticket: &str) -> Option<String> {
    None
}

/// iroh 的能力声明（两套实现共用，避免重复）
fn iroh_capabilities() -> BackendCapabilities {
    BackendCapabilities {
        backend_type: BackendType::Iroh,
        ipns: false,    // Iroh 使用内容寻址，不原生支持 IPNS
        pinning: false, // Iroh 使用 tags / 作者证书系统
        gc: true,       // Iroh 自动 GC
        pubsub: false,
        mfs: false,     // Iroh 使用文档系统
        bitswap: false, // Iroh 使用自己的传输协议
        cid_version: 1, // BLAKE3 内容标识，语义上对应 CIDv1
    }
}

// ════════════════════════════════════════════════════════════════
// 真实实现（启用 `iroh-backend` feature）
// ════════════════════════════════════════════════════════════════

#[cfg(feature = "iroh-backend")]
mod real {
    use super::*;
    use iroh::protocol::Router;
    use iroh::{Endpoint, EndpointAddr, SecretKey};
    use iroh_blobs::store::fs::FsStore;
    use iroh_blobs::{BlobsProtocol, Hash};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    /// 已启动的网络/serving 栈（惰性构建，可被 `shutdown` 清空后重建）
    #[derive(Clone)]
    struct IrohNet {
        endpoint: Endpoint,
        /// 保持 Router 存活即持续对外提供 blobs（serving）；`shutdown` 时关闭它
        router: Router,
        node_id: String,
    }

    /// Iroh 原生后端（真实实现）
    ///
    /// `new()` 保持同步、廉价（仅登记数据目录）；真正的资源在首次使用时惰性初始化。
    /// **刻意把 blob 存储与网络栈解耦**：add/cat 只依赖 `FsStore`（纯本机、可离线），
    /// 不会因端点绑定（relay/发现）问题而阻塞内容读写；`node_info`/serving/互传
    /// 才会触发网络栈初始化。
    ///
    /// `net` 用 `RwLock<Option<_>>` 而非 `OnceCell`，以支持 `shutdown` 后**重启**
    /// （清空 → 下次使用时重建）——这是 Phase D2 iroh 生命周期的基础。
    #[derive(Clone)]
    pub struct IrohBackend {
        data_dir: PathBuf,
        /// 持久化 blob 存储（内容寻址 add/cat 的核心）。可重置：`shutdown` 会连带
        /// 停掉与 Router 共享的 store actor，故清空后下次从磁盘重新 `load`（内容持久在盘）。
        store: Arc<RwLock<Option<FsStore>>>,
        /// 网络 + serving 栈（Endpoint + Router(BlobsProtocol)），可重置以支持重启
        net: Arc<RwLock<Option<IrohNet>>>,
        /// 已连接过的对等节点 node_id → 方向（会话内追踪）。iroh 1.0 无节点枚举 API，
        /// 只能自行记录：`fetch_from` 主动连接对端登记 outbound；BlobsProtocol 的
        /// `ClientConnected` 事件登记 inbound（别人来我这取内容）。
        peers: Arc<RwLock<std::collections::BTreeMap<String, String>>>,
    }

    /// 记录/合并对等节点方向（"outbound" / "inbound" / "both"）
    async fn record_peer(
        peers: &RwLock<std::collections::BTreeMap<String, String>>,
        node_id: String,
        dir: &str,
    ) {
        let mut g = peers.write().await;
        match g.get(&node_id).map(|s| s.as_str()) {
            None => {
                g.insert(node_id, dir.to_string());
            }
            Some(existing) if existing != dir && existing != "both" => {
                g.insert(node_id, "both".to_string());
            }
            Some(_) => {}
        }
    }

    impl IrohBackend {
        pub fn new(data_dir: PathBuf) -> Self {
            let _ = std::fs::create_dir_all(&data_dir);
            Self {
                data_dir,
                store: Arc::new(RwLock::new(None)),
                net: Arc::new(RwLock::new(None)),
                peers: Arc::new(RwLock::new(std::collections::BTreeMap::new())),
            }
        }

        /// 惰性打开（或重开）持久化 blob 存储，返回其克隆句柄
        async fn store(&self) -> Result<FsStore, BackendError> {
            if let Some(s) = self.store.read().await.as_ref() {
                return Ok(s.clone());
            }
            let mut guard = self.store.write().await;
            if let Some(s) = guard.as_ref() {
                return Ok(s.clone());
            }
            let s = FsStore::load(self.data_dir.join("blobs"))
                .await
                .map_err(|e| BackendError::internal(format!("iroh FsStore load failed: {e}")))?;
            tracing::info!("iroh FsStore opened at {:?}", self.data_dir.join("blobs"));
            *guard = Some(s.clone());
            Ok(s)
        }

        /// 读取或生成**持久化**节点身份密钥（存于 data_dir/node.secret）
        ///
        /// 这样节点身份（EndpointId/PeerID）跨重启稳定——不再是每次绑定的临时密钥。
        fn load_or_create_secret(&self) -> SecretKey {
            let path = self.data_dir.join("node.secret");
            if let Ok(bytes) = std::fs::read(&path) {
                if bytes.len() == 32 {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&bytes);
                    return SecretKey::from_bytes(&arr);
                }
            }
            let sk = SecretKey::generate();
            if let Err(e) = std::fs::write(&path, sk.to_bytes()) {
                tracing::warn!("failed to persist iroh node.secret: {e}");
            }
            sk
        }

        /// 惰性构建（或重建）网络 + serving 栈，返回其克隆句柄。
        /// 持久身份 → Endpoint → BlobsProtocol → Router。
        async fn net(&self) -> Result<IrohNet, BackendError> {
            // 快路径：已就绪则直接返回克隆
            if let Some(net) = self.net.read().await.as_ref() {
                return Ok(net.clone());
            }
            // 慢路径：加写锁构建（双检，避免竞态重复构建）
            let mut guard = self.net.write().await;
            if let Some(net) = guard.as_ref() {
                return Ok(net.clone());
            }

            let store = self.store().await?;
            let secret = self.load_or_create_secret();
            let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
                .secret_key(secret)
                .bind()
                .await
                .map_err(|e| BackendError::internal(format!("iroh endpoint bind failed: {e}")))?;
            let node_id = endpoint.id().to_string();

            // 入站连接事件：给 BlobsProtocol 传 EventSender，后台任务把「连来的对端」
            // 登记为 inbound（别人来我这取内容也可观测）。
            use iroh_blobs::provider::events::{
                ConnectMode, EventMask, EventSender, ProviderMessage,
            };
            let mask = EventMask {
                connected: ConnectMode::Notify,
                ..EventMask::DEFAULT
            };
            let (events, mut event_rx) = EventSender::channel(64, mask);

            let blobs = BlobsProtocol::new(&store, Some(events));
            let router = Router::builder(endpoint.clone())
                .accept(iroh_blobs::ALPN, blobs)
                .spawn();

            // 后台排空事件通道（否则背压会阻塞 serving），并记录入站对端。
            // Router 关闭时 EventSender 被丢弃 → recv 返回 None → 任务自然结束。
            let peers = self.peers.clone();
            tokio::spawn(async move {
                while let Some(msg) = event_rx.recv().await {
                    // ConnectMode::Notify 下入站连接走 ClientConnectedNotify 变体
                    if let ProviderMessage::ClientConnectedNotify(ev) = msg {
                        if let Some(eid) = ev.endpoint_id {
                            record_peer(&peers, eid.to_string(), "inbound").await;
                        }
                    }
                }
            });

            tracing::info!(
                "iroh node serving: {} (data_dir={:?})",
                node_id,
                self.data_dir
            );
            let net = IrohNet {
                endpoint,
                router,
                node_id,
            };
            *guard = Some(net.clone());
            Ok(net)
        }

        /// 本节点可被拨号的地址（含直连 socket 地址），供其它节点连接拉取
        pub async fn addr(&self) -> Result<EndpointAddr, BackendError> {
            Ok(self.net().await?.endpoint.addr())
        }

        /// 从远端节点按内容 hash 拉取 blob（serving/互传的接收侧）
        ///
        /// 接收侧：连接 `addr` → 走 blobs 协议按 hash 拉取 → 落入本地 store → 读回字节。
        pub async fn fetch_from(
            &self,
            addr: EndpointAddr,
            cid: &str,
        ) -> Result<Vec<u8>, BackendError> {
            let hash = Self::parse_hash(cid)?;
            let net = self.net().await?;
            let store = self.store().await?;

            // 登记对端节点（连接前记录其 node_id），供 swarm_peers 报告
            record_peer(&self.peers, addr.id.to_string(), "outbound").await;

            let conn = net
                .endpoint
                .connect(addr, iroh_blobs::ALPN)
                .await
                .map_err(|e| BackendError::network(format!("iroh connect failed: {e}")))?;
            store
                .remote()
                .fetch(conn, hash)
                .await
                .map_err(|e| BackendError::network(format!("iroh fetch failed: {e}")))?;

            let bytes = store
                .get_bytes(hash)
                .await
                .map_err(|e| BackendError::not_found(format!("iroh get_bytes after fetch: {e}")))?;
            Ok(bytes.to_vec())
        }

        /// 为本地某个 blob 生成**可分享的 ticket**（含本节点地址 + 内容 hash）。
        ///
        /// 对方拿到该字符串即可用 [`fetch_ticket`](Self::fetch_ticket) 收取内容。
        /// 调用会确保 serving 栈已启动（否则对方连不上）。
        pub async fn share_ticket(&self, cid: &str) -> Result<String, BackendError> {
            let hash = Self::parse_hash(cid)?;
            let net = self.net().await?; // 确保 Router/serving 已就绪且地址可用
            let addr = net.endpoint.addr();
            let ticket =
                iroh_blobs::ticket::BlobTicket::new(addr, hash, iroh_blobs::BlobFormat::Raw);
            Ok(ticket.to_string())
        }

        /// 用 ticket 收取内容：解析出「提供者地址 + 内容 hash」→ 连接并按 hash 拉取。
        pub async fn fetch_ticket(&self, ticket_str: &str) -> Result<Vec<u8>, BackendError> {
            let ticket: iroh_blobs::ticket::BlobTicket =
                ticket_str.trim().parse().map_err(|e| BackendError {
                    kind: crate::backend_trait::BackendErrorKind::InvalidArgument,
                    message: format!("invalid blob ticket: {e}"),
                })?;
            let addr = ticket.addr().clone();
            let cid = ticket.hash().to_string();
            self.fetch_from(addr, &cid).await
        }

        /// 本地是否已存有该 blob（内容发现：供 Auto 路由「按内容所在」分发）。
        ///
        /// 非 iroh 形态的 cid（解析失败）视为「本地没有」，返回 `Ok(false)`，
        /// 从而让路由回退到 Kubo/启发式，而不是报错。
        pub async fn has(&self, cid: &str) -> Result<bool, BackendError> {
            let hash = match Self::parse_hash(cid) {
                Ok(h) => h,
                Err(_) => return Ok(false),
            };
            let store = self.store().await?;
            store
                .has(hash)
                .await
                .map_err(|e| BackendError::internal(format!("iroh has() failed: {e}")))
        }

        /// keep-alive：为内容设置一个**命名持久 tag**，使其不被 GC 回收。
        ///
        /// iroh-blobs 的 GC 只保留「被 tag 引用」的内容；`add` 产生的临时引用可能过期，
        /// 显式 `keep` 保证「我要长期保留这些」的语义（对应 Kubo 的 pin）。
        pub async fn keep(&self, cid: &str) -> Result<(), BackendError> {
            let hash = Self::parse_hash(cid)?;
            let store = self.store().await?;
            let name = format!("keep/{hash}");
            store
                .tags()
                .set(name.as_bytes(), hash)
                .await
                .map_err(|e| BackendError::internal(format!("iroh keep(set tag) failed: {e}")))?;
            tracing::info!("iroh keep: tagged {}", hash);
            Ok(())
        }

        /// 取消 keep-alive：删除命名 tag（内容此后可被 GC）。
        pub async fn unkeep(&self, cid: &str) -> Result<(), BackendError> {
            let hash = Self::parse_hash(cid)?;
            let store = self.store().await?;
            let name = format!("keep/{hash}");
            store.tags().delete(name.as_bytes()).await.map_err(|e| {
                BackendError::internal(format!("iroh unkeep(delete tag) failed: {e}"))
            })?;
            Ok(())
        }

        /// 本地内容条目数（Phase D3 可观测性）：流式统计 tag 数量（≈ 被引用的内容数）。
        pub async fn content_count(&self) -> Result<u64, BackendError> {
            use futures_util::StreamExt;
            let store = self.store().await?;
            let mut stream = store
                .tags()
                .list()
                .await
                .map_err(|e| BackendError::internal(format!("iroh tags list failed: {e}")))?;
            let mut count = 0u64;
            while let Some(item) = stream.next().await {
                if item.is_ok() {
                    count += 1;
                }
            }
            Ok(count)
        }

        fn parse_hash(cid: &str) -> Result<Hash, BackendError> {
            let c = cid.trim();
            let invalid = |msg: String| BackendError {
                kind: crate::backend_trait::BackendErrorKind::InvalidArgument,
                message: msg,
            };
            // 关键：iroh 的 Hash::from_str 在 debug 下对畸形输入会 panic（data-encoding
            // 内部断言），因此**先做形态校验**再解析。iroh Hash 是 32 字节：
            //   - Display/to_string() → 64 位十六进制（本应用产生的 cid 都是这个形态）
            //   - FromStr 亦兼收 52 位 base32（外部粘贴的 ticket 里可能是此形态）
            // 非这两种形态（如 Kubo 的 Qm.../baf... CID）直接判无效，绝不喂给会 panic 的解析器。
            let is_hex = c.len() == 64 && c.bytes().all(|b| b.is_ascii_hexdigit());
            let is_b32 = c.len() == 52 && c.bytes().all(|b| matches!(b, b'a'..=b'z' | b'2'..=b'7'));
            if !(is_hex || is_b32) {
                return Err(invalid(format!("not an iroh blob hash: '{cid}'")));
            }
            c.parse::<Hash>()
                .map_err(|e| invalid(format!("invalid iroh hash '{cid}': {e}")))
        }
    }

    #[async_trait]
    impl Backend for IrohBackend {
        fn backend_type(&self) -> BackendType {
            BackendType::Iroh
        }
        fn capabilities(&self) -> BackendCapabilities {
            iroh_capabilities()
        }

        async fn is_available(&self) -> bool {
            // 以 blob 存储可打开为准（add/cat 的前提），不依赖网络端点
            self.store().await.is_ok()
        }

        async fn node_info(&self) -> Result<NodeInfo, BackendError> {
            let net = self.net().await?;
            Ok(NodeInfo {
                peer_id: net.node_id.clone(),
                agent_version: format!("iroh-desktop-rust/{}", env!("CARGO_PKG_VERSION")),
                protocol_version: "iroh/1.0".to_string(),
                addresses: vec![],
            })
        }

        async fn version(&self) -> Result<String, BackendError> {
            Ok(format!(
                "iroh 1.x + iroh-blobs (native, {})",
                env!("CARGO_PKG_VERSION")
            ))
        }

        async fn repo_stat(&self) -> Result<RepoInfo, BackendError> {
            // 数据目录大小（近似）
            let size = std::fs::metadata(self.data_dir.join("blobs"))
                .or_else(|_| std::fs::metadata(&self.data_dir))
                .map(|m| m.len())
                .unwrap_or(0);
            Ok(RepoInfo {
                num_objects: 0,
                repo_size: size,
                version: "iroh-blobs".to_string(),
            })
        }

        async fn repo_gc(&self) -> Result<(), BackendError> {
            Err(BackendError::unsupported(
                "Iroh GC is automatic; no manual trigger",
            ))
        }

        async fn add_file(&self, path: &Path) -> Result<AddOutput, BackendError> {
            let store = self.store().await?;
            let size = tokio::fs::metadata(path)
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unnamed")
                .to_string();

            // add_path 返回进度句柄；await 得到含 BLAKE3 hash 的 TagInfo
            let tag = store
                .add_path(path)
                .await
                .map_err(|e| BackendError::internal(format!("iroh add_path failed: {e}")))?;

            Ok(AddOutput {
                cid: tag.hash.to_string(),
                size,
                name,
            })
        }

        async fn cat(&self, cid: &str) -> Result<Vec<u8>, BackendError> {
            let store = self.store().await?;
            let hash = Self::parse_hash(cid)?;
            let bytes = store
                .get_bytes(hash)
                .await
                .map_err(|e| BackendError::not_found(format!("iroh get_bytes failed: {e}")))?;
            Ok(bytes.to_vec())
        }

        async fn file_size(&self, cid: &str) -> Result<u64, BackendError> {
            // 轻量实现暂缺 blob 元数据查询；读回内容取长度（本机开销可接受）
            let bytes = self.cat(cid).await?;
            Ok(bytes.len() as u64)
        }

        async fn pin_ls(&self) -> Result<Vec<PinEntry>, BackendError> {
            Err(BackendError::unsupported("Iroh uses tags instead of pins"))
        }
        async fn pin_add(&self, _cid: &str) -> Result<(), BackendError> {
            Err(BackendError::unsupported("Iroh uses tags instead of pins"))
        }
        async fn pin_rm(&self, _cid: &str) -> Result<(), BackendError> {
            Err(BackendError::unsupported("Iroh uses tags instead of pins"))
        }

        async fn swarm_peers(&self) -> Result<Vec<BPeerInfo>, BackendError> {
            // iroh 1.0 无「枚举所有节点」的 API，改为会话内自行追踪：
            //   - outbound：fetch_from 主动连接对端时登记；
            //   - inbound：BlobsProtocol 的 ClientConnectedNotify 事件（别人来我这取内容）；
            //   - both：双向都发生过。
            let peers = self.peers.read().await;
            Ok(peers
                .iter()
                .map(|(id, dir)| BPeerInfo {
                    peer_id: id.clone(),
                    address: format!("iroh://{id}"),
                    direction: Some(dir.clone()),
                })
                .collect())
        }

        async fn bandwidth_stats(&self) -> Result<BandwidthInfo, BackendError> {
            Err(BackendError::unsupported(
                "Iroh bandwidth stats not wired yet",
            ))
        }
        async fn bitswap_stats(&self) -> Result<BitswapInfo, BackendError> {
            Err(BackendError::unsupported(
                "Iroh does not use Bitswap protocol",
            ))
        }

        async fn name_publish(
            &self,
            _cid: &str,
            _key: &str,
            _lifetime: &str,
        ) -> Result<IpnsOutput, BackendError> {
            Err(BackendError::unsupported(
                "Iroh does not support IPNS (use content addressing)",
            ))
        }
        async fn name_resolve(&self, _name: &str) -> Result<IpnsPath, BackendError> {
            Err(BackendError::unsupported(
                "Iroh does not support IPNS (use content addressing)",
            ))
        }

        async fn shutdown(&self) -> Result<(), BackendError> {
            // 关闭网络/serving 栈并清空持有槽——下次使用时 net()/store() 会自动重建（重启）。
            // Router::shutdown 会连带关闭 Endpoint，也会停掉与之共享的 store actor，
            // 因此 store 同样清空；内容持久在磁盘，下次 store() 从盘重新 load。
            if let Some(net) = self.net.write().await.take() {
                if let Err(e) = net.router.shutdown().await {
                    tracing::warn!("iroh router shutdown error: {e}");
                }
            }
            let _ = self.store.write().await.take();
            tracing::info!(
                "iroh node shut down (net+store cleared; re-init from disk on next use)"
            );
            Ok(())
        }
    }
}

#[cfg(feature = "iroh-backend")]
pub use real::IrohBackend;

// ════════════════════════════════════════════════════════════════
// Stub 实现（默认，无 `iroh-backend` feature）
// ════════════════════════════════════════════════════════════════

/// Iroh 原生后端（stub）
///
/// 未启用 `iroh-backend` feature 时使用：仅 `node_info` / `version` 返回占位数据，
/// 其余操作安全降级为 `Unsupported`。
#[cfg(not(feature = "iroh-backend"))]
#[derive(Clone)]
pub struct IrohBackend {
    data_dir: std::path::PathBuf,
    initialized: bool,
}

#[cfg(not(feature = "iroh-backend"))]
impl IrohBackend {
    pub fn new(data_dir: std::path::PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&data_dir);
        Self {
            data_dir,
            initialized: true,
        }
    }

    fn local_peer_id(&self) -> String {
        "iroh:stub:12D3KooW".to_string()
    }

    /// stub：需启用 iroh-backend feature 才能生成分享 ticket
    pub async fn share_ticket(&self, _cid: &str) -> Result<String, BackendError> {
        Err(BackendError::unsupported(
            "iroh share requires the iroh-backend feature",
        ))
    }

    /// stub：需启用 iroh-backend feature 才能用 ticket 收取
    pub async fn fetch_ticket(&self, _ticket: &str) -> Result<Vec<u8>, BackendError> {
        Err(BackendError::unsupported(
            "iroh fetch requires the iroh-backend feature",
        ))
    }

    /// stub：无本地 iroh 存储，恒为 false（Auto 路由回退到 Kubo/启发式）
    pub async fn has(&self, _cid: &str) -> Result<bool, BackendError> {
        Ok(false)
    }

    /// stub：需启用 iroh-backend feature 才能 keep
    pub async fn keep(&self, _cid: &str) -> Result<(), BackendError> {
        Err(BackendError::unsupported(
            "iroh keep requires the iroh-backend feature",
        ))
    }

    /// stub：需启用 iroh-backend feature 才能 unkeep
    pub async fn unkeep(&self, _cid: &str) -> Result<(), BackendError> {
        Err(BackendError::unsupported(
            "iroh unkeep requires the iroh-backend feature",
        ))
    }

    /// stub：无真实 iroh 存储 → 内容计数不可用
    pub async fn content_count(&self) -> Result<u64, BackendError> {
        Err(BackendError::unsupported(
            "iroh content_count requires the iroh-backend feature",
        ))
    }
}

#[cfg(not(feature = "iroh-backend"))]
#[async_trait]
impl Backend for IrohBackend {
    fn backend_type(&self) -> BackendType {
        BackendType::Iroh
    }
    fn capabilities(&self) -> BackendCapabilities {
        iroh_capabilities()
    }

    async fn is_available(&self) -> bool {
        self.initialized
    }

    async fn node_info(&self) -> Result<NodeInfo, BackendError> {
        Ok(NodeInfo {
            peer_id: self.local_peer_id(),
            agent_version: format!("iroh-desktop-rust/{} (stub)", env!("CARGO_PKG_VERSION")),
            protocol_version: "iroh/stub".to_string(),
            addresses: vec!["/ip4/0.0.0.0/udp/0/quic-v1".to_string()],
        })
    }

    async fn version(&self) -> Result<String, BackendError> {
        Ok("iroh (stub — build with --features iroh-backend for real node)".to_string())
    }

    async fn repo_stat(&self) -> Result<RepoInfo, BackendError> {
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
            "Iroh GC is automatic; no manual trigger available",
        ))
    }

    async fn add_file(&self, _path: &Path) -> Result<AddOutput, BackendError> {
        Err(BackendError::unsupported(
            "Iroh add_file requires iroh-backend feature",
        ))
    }
    async fn cat(&self, _cid: &str) -> Result<Vec<u8>, BackendError> {
        Err(BackendError::unsupported(
            "Iroh cat requires iroh-backend feature",
        ))
    }
    async fn file_size(&self, _cid: &str) -> Result<u64, BackendError> {
        Err(BackendError::unsupported(
            "Iroh file_size requires iroh-backend feature",
        ))
    }

    async fn pin_ls(&self) -> Result<Vec<PinEntry>, BackendError> {
        Err(BackendError::unsupported("Iroh uses tags instead of pins"))
    }
    async fn pin_add(&self, _cid: &str) -> Result<(), BackendError> {
        Err(BackendError::unsupported("Iroh uses tags instead of pins"))
    }
    async fn pin_rm(&self, _cid: &str) -> Result<(), BackendError> {
        Err(BackendError::unsupported("Iroh uses tags instead of pins"))
    }

    async fn swarm_peers(&self) -> Result<Vec<BPeerInfo>, BackendError> {
        Ok(vec![])
    }

    async fn bandwidth_stats(&self) -> Result<BandwidthInfo, BackendError> {
        Err(BackendError::unsupported(
            "Iroh bandwidth stats require iroh-backend feature",
        ))
    }
    async fn bitswap_stats(&self) -> Result<BitswapInfo, BackendError> {
        Err(BackendError::unsupported(
            "Iroh does not use Bitswap protocol",
        ))
    }

    async fn name_publish(
        &self,
        _cid: &str,
        _key: &str,
        _lifetime: &str,
    ) -> Result<IpnsOutput, BackendError> {
        Err(BackendError::unsupported(
            "Iroh does not support IPNS publish (use content addressing)",
        ))
    }
    async fn name_resolve(&self, _name: &str) -> Result<IpnsPath, BackendError> {
        Err(BackendError::unsupported(
            "Iroh does not support IPNS resolve (use content addressing)",
        ))
    }

    async fn shutdown(&self) -> Result<(), BackendError> {
        tracing::info!("Iroh backend shutdown (stub)");
        Ok(())
    }
}

// ════════════════════════════════════════════════════════════════
// 测试
// ════════════════════════════════════════════════════════════════

#[cfg(all(test, not(feature = "iroh-backend")))]
mod stub_tests {
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
    async fn test_iroh_stub_unsupported_operations() {
        let dir = std::env::temp_dir().join("iroh-test-unsup");
        let backend = IrohBackend::new(dir);
        assert!(backend.pin_add("QmTest").await.is_err());
        assert!(backend.name_publish("QmTest", "self", "24h").await.is_err());
        assert!(backend.bitswap_stats().await.is_err());
        assert!(backend.node_info().await.is_ok());
        assert!(backend.version().await.is_ok());
    }
}

#[cfg(all(test, feature = "iroh-backend"))]
mod real_tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn unique_dir(tag: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("iroh-real-{}-{}-{}", tag, std::process::id(), n))
    }

    #[tokio::test]
    async fn test_iroh_real_available_and_node_info() {
        let backend = IrohBackend::new(unique_dir("info"));
        assert!(backend.is_available().await, "real iroh node should init");
        let info = backend.node_info().await.expect("node_info");
        assert!(!info.peer_id.is_empty(), "node_id should be non-empty");
        assert_ne!(
            info.peer_id, "iroh:stub:12D3KooW",
            "should be a real identity, not stub"
        );
    }

    /// 核心断言：本机 add → cat 往返 + BLAKE3 内容完整性
    #[tokio::test]
    async fn test_iroh_add_cat_roundtrip_integrity() {
        let dir = unique_dir("roundtrip");
        let backend = IrohBackend::new(dir);

        // 写一个临时文件（含可辨识内容）
        let payload: Vec<u8> = (0..64_000u32).map(|i| (i % 251) as u8).collect();
        let tmp = std::env::temp_dir().join(format!("iroh-roundtrip-{}.bin", std::process::id()));
        tokio::fs::write(&tmp, &payload).await.unwrap();

        // add → 得到 BLAKE3 hash
        let added = backend
            .add_file(&tmp)
            .await
            .expect("add_file should succeed");
        assert!(!added.cid.is_empty());
        assert_eq!(added.size, payload.len() as u64);

        // cat → 读回，内容必须逐字节相等
        let got = backend.cat(&added.cid).await.expect("cat should succeed");
        assert_eq!(got, payload, "round-trip content must be byte-identical");

        // 内容寻址自证：同一内容再 add 必得同一 hash（BLAKE3 完整性）
        let added2 = backend.add_file(&tmp).await.expect("re-add");
        assert_eq!(
            added.cid, added2.cid,
            "same content must yield same BLAKE3 hash"
        );

        let _ = tokio::fs::remove_file(&tmp).await;
    }

    /// #1 身份持久化：同一 data_dir 两次启动，节点身份必须一致
    #[tokio::test]
    async fn test_iroh_identity_persists_across_restart() {
        let dir = unique_dir("ident");

        let id1 = {
            let b = IrohBackend::new(dir.clone());
            b.node_info().await.expect("node_info #1").peer_id
        };
        // 模拟重启：同一目录、全新实例
        let id2 = {
            let b = IrohBackend::new(dir.clone());
            b.node_info().await.expect("node_info #2").peer_id
        };

        assert_eq!(
            id1, id2,
            "node identity must persist across restarts (data_dir/node.secret)"
        );
        assert!(!id1.is_empty());
    }

    /// #2 serving / 两节点互传：A add → B 经 QUIC 按 hash 从 A 拉取 → 内容一致
    #[tokio::test]
    async fn test_iroh_two_node_transfer() {
        let node_a = IrohBackend::new(unique_dir("nodeA"));
        let node_b = IrohBackend::new(unique_dir("nodeB"));

        // A 添加内容
        let payload: Vec<u8> = (0..40_000u32).map(|i| (i % 253) as u8).collect();
        let tmp = std::env::temp_dir().join(format!("iroh-2node-{}.bin", std::process::id()));
        tokio::fs::write(&tmp, &payload).await.unwrap();
        let added = node_a.add_file(&tmp).await.expect("A add_file");

        // A 启动 serving 栈并给出可拨号地址
        let addr_a = node_a.addr().await.expect("A addr");

        // B 从 A 按 hash 拉取（限时，避免网络问题挂死）
        let got = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            node_b.fetch_from(addr_a, &added.cid),
        )
        .await
        .expect("two-node fetch timed out")
        .expect("B fetch_from A");

        assert_eq!(
            got, payload,
            "B must receive byte-identical content served by A"
        );

        // B 本地也应能直接 cat（内容已落入 B 的 store）
        let local = node_b
            .cat(&added.cid)
            .await
            .expect("B local cat after fetch");
        assert_eq!(local, payload);

        // swarm_peers（出站）：B 连接过 A 后，应在其对等节点列表里记录到 A
        let a_id = node_a.node_info().await.unwrap().peer_id;
        let peers = node_b.swarm_peers().await.expect("swarm_peers");
        assert!(
            peers
                .iter()
                .any(|p| p.peer_id == a_id && p.direction.as_deref() == Some("outbound")),
            "B's swarm_peers should include A ({}) as outbound; got {:?}",
            a_id,
            peers
        );

        // swarm_peers（入站）：A 作为提供方，应记录到连来的 B（ClientConnected 事件异步，轮询等待）
        let b_id = node_b.node_info().await.unwrap().peer_id;
        let mut a_has_b = false;
        for _ in 0..20 {
            let a_peers = node_a.swarm_peers().await.unwrap();
            if a_peers.iter().any(|p| p.peer_id == b_id) {
                a_has_b = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        assert!(
            a_has_b,
            "A should track inbound peer B ({}) after serving it content",
            b_id
        );

        let _ = tokio::fs::remove_file(&tmp).await;
    }

    /// (a) BlobTicket 分享流程：A add → A 生成 ticket → B 用 ticket 收取
    #[tokio::test]
    async fn test_iroh_ticket_share_and_fetch() {
        let node_a = IrohBackend::new(unique_dir("ticketA"));
        let node_b = IrohBackend::new(unique_dir("ticketB"));

        let payload: Vec<u8> = (0..24_000u32).map(|i| (i % 249) as u8).collect();
        let tmp = std::env::temp_dir().join(format!("iroh-ticket-{}.bin", std::process::id()));
        tokio::fs::write(&tmp, &payload).await.unwrap();

        // A 添加 → 生成可分享 ticket
        let added = node_a.add_file(&tmp).await.expect("A add");
        let ticket = node_a
            .share_ticket(&added.cid)
            .await
            .expect("A share_ticket");
        assert!(!ticket.is_empty(), "ticket string should be non-empty");

        // B 仅凭 ticket 字符串即可收取（限时）
        let got = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            node_b.fetch_ticket(&ticket),
        )
        .await
        .expect("ticket fetch timed out")
        .expect("B fetch_ticket");

        assert_eq!(
            got, payload,
            "B must receive byte-identical content via ticket"
        );

        let _ = tokio::fs::remove_file(&tmp).await;
    }

    /// D2：shutdown 后自动重启——身份持久（同一 node_id）+ 内容留存（store 跨 shutdown）
    #[tokio::test]
    async fn test_iroh_shutdown_and_reinit() {
        let backend = IrohBackend::new(unique_dir("shutdown"));

        // 起网络栈，记录身份
        let id1 = backend.node_info().await.expect("node_info 1").peer_id;

        // add 一个内容
        let payload = vec![3u8; 2000];
        let tmp = std::env::temp_dir().join(format!("iroh-shutdown-{}.bin", std::process::id()));
        tokio::fs::write(&tmp, &payload).await.unwrap();
        let added = backend.add_file(&tmp).await.expect("add");

        // 关闭网络栈
        backend.shutdown().await.expect("shutdown");

        // 关闭后：node_info 自动重建，身份持久（同一 node_id）
        let id2 = backend
            .node_info()
            .await
            .expect("node_info 2 (reinit)")
            .peer_id;
        assert_eq!(
            id1, id2,
            "node identity must persist across shutdown/restart"
        );

        // 内容仍在（store 跨 shutdown 保留）
        let got = backend.cat(&added.cid).await.expect("cat after restart");
        assert_eq!(got, payload);

        let _ = tokio::fs::remove_file(&tmp).await;
    }

    /// D2：keep-alive tag 设置/删除不报错，且不影响内容读取
    #[tokio::test]
    async fn test_iroh_keep_and_unkeep() {
        let backend = IrohBackend::new(unique_dir("keep"));
        let payload = vec![5u8; 3000];
        let tmp = std::env::temp_dir().join(format!("iroh-keep-{}.bin", std::process::id()));
        tokio::fs::write(&tmp, &payload).await.unwrap();
        let added = backend.add_file(&tmp).await.expect("add");

        // keep → 内容仍可读
        backend.keep(&added.cid).await.expect("keep");
        let got = backend.cat(&added.cid).await.expect("cat after keep");
        assert_eq!(got, payload);

        // unkeep 不报错
        backend.unkeep(&added.cid).await.expect("unkeep");

        let _ = tokio::fs::remove_file(&tmp).await;
    }

    /// D3：content_count 随 add + keep 增长
    #[tokio::test]
    async fn test_iroh_content_count() {
        let backend = IrohBackend::new(unique_dir("count"));
        let c0 = backend.content_count().await.expect("count 0");

        let payload = vec![1u8; 500];
        let tmp = std::env::temp_dir().join(format!("iroh-count-{}.bin", std::process::id()));
        tokio::fs::write(&tmp, &payload).await.unwrap();
        let added = backend.add_file(&tmp).await.expect("add");
        backend.keep(&added.cid).await.expect("keep");

        let c1 = backend.content_count().await.expect("count 1");
        assert!(
            c1 > c0,
            "content count should grow after add+keep (c0={c0}, c1={c1})"
        );

        let _ = tokio::fs::remove_file(&tmp).await;
    }
}
