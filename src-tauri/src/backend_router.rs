//! 双栈路由骨架 — Phase C
//!
//! 在 `Backend` 抽象缝**之上**，按「内容来源 / 策略」在 Kubo 与 iroh 之间选择后端。
//! 这是把「双后端可切换」升级为「双栈自动协同」的第一块骨架：路由决策与分发在这里，
//! 上层 GUI/缓存/队列无需感知底层是 Go 还是 Rust。
//!
//! ```text
//!   commands.rs / proxy.rs
//!            │
//!     ┌──────┴───────┐
//!     │ BackendRouter │   ← 本模块：按 CID/策略选后端并分发
//!     └──────┬───────┘
//!       ┌────┴────┐
//!   KuboBackend  IrohBackend
//! ```
//!
//! ## 现状与边界（诚实）
//!
//! - **策略**已可切换：`KuboOnly`（现网互操作，默认）/ `IrohOnly` / `Auto`（按内容分类）。
//! - **分类**目前是**基于前缀的启发式**（`classify_cid`）：IPFS CID（`Qm...` / `bafy/bafk...`）
//!   判为 Kubo，其余判为 iroh 的 BLAKE3 内容。这是骨架级实现——由于两套哈希在字符串形态上
//!   可能重叠，**生产级 Phase C 需要「内容来源标记」而非纯前缀猜测**。此处刻意保留可测的
//!   确定性行为，并把这一限制显式写在类型与测试里。
//! - 默认策略为 `KuboOnly`，即在路由层被显式启用前，行为与现有单栈完全一致（零回归）。

use crate::backend_trait::{AddOutput, Backend, BackendError, BackendType};
use crate::iroh_adapter::IrohBackend;
use crate::kubo_adapter::KuboBackend;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 从 JSON 文件载入一个 `String -> V` 映射（文件不存在/损坏时返回空表）
fn load_json_map<V: DeserializeOwned>(path: &Option<PathBuf>) -> HashMap<String, V> {
    path.as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// 把一个映射快照落盘为 JSON（best-effort）
fn persist_json_map<V: Serialize>(path: &Option<PathBuf>, map: &HashMap<String, V>) {
    let Some(p) = path else { return };
    match serde_json::to_string_pretty(map) {
        Ok(json) => {
            if let Err(e) = std::fs::write(p, json) {
                tracing::warn!("failed to persist {:?}: {e}", p);
            }
        }
        Err(e) => tracing::warn!("failed to serialize map for {:?}: {e}", p),
    }
}

/// 路由策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutePolicy {
    /// 全部走 Kubo（默认；等价于现有单栈行为，零回归）
    KuboOnly,
    /// 全部走 iroh 原生
    IrohOnly,
    /// 按内容分类自动选择（`classify_cid`）
    Auto,
}

impl RoutePolicy {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "kubo" | "kubo_only" | "kuboonly" => Some(Self::KuboOnly),
            "iroh" | "iroh_only" | "irohonly" => Some(Self::IrohOnly),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }
}

impl std::fmt::Display for RoutePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoutePolicy::KuboOnly => write!(f, "KuboOnly"),
            RoutePolicy::IrohOnly => write!(f, "IrohOnly"),
            RoutePolicy::Auto => write!(f, "Auto"),
        }
    }
}

/// 双栈路由器：持有两套后端 + 当前策略 + **内容来源标记**，负责选择与分发
#[derive(Clone)]
pub struct BackendRouter {
    kubo: Arc<KuboBackend>,
    iroh: Arc<IrohBackend>,
    policy: Arc<RwLock<RoutePolicy>>,
    /// CID → 产生它的后端（**权威来源标记**，优先于前缀启发式）
    origins: Arc<RwLock<HashMap<String, BackendType>>>,
    origins_path: Option<PathBuf>,
    /// CID → 已知 iroh provider 的 ticket（用于网络 fallback / 跨节点内容发现）
    providers: Arc<RwLock<HashMap<String, String>>>,
    providers_path: Option<PathBuf>,
}

impl BackendRouter {
    /// `data_dir` 为空时来源标记 / provider 仅存内存（测试用）；
    /// 否则分别持久化到 `<data_dir>/cid_origins.json` 与 `<data_dir>/cid_providers.json`。
    pub fn new(kubo: Arc<KuboBackend>, iroh: Arc<IrohBackend>, data_dir: Option<PathBuf>) -> Self {
        Self::new_with_policy(kubo, iroh, data_dir, RoutePolicy::KuboOnly)
    }

    pub fn new_with_policy(
        kubo: Arc<KuboBackend>,
        iroh: Arc<IrohBackend>,
        data_dir: Option<PathBuf>,
        initial_policy: RoutePolicy,
    ) -> Self {
        let origins_path = data_dir.as_ref().map(|d| d.join("cid_origins.json"));
        let providers_path = data_dir.as_ref().map(|d| d.join("cid_providers.json"));

        // 启动时从磁盘恢复
        let origins = load_json_map::<BackendType>(&origins_path);
        let providers = load_json_map::<String>(&providers_path);

        Self {
            kubo,
            iroh,
            // 默认 KuboOnly：在被显式切换前，路由层不改变任何现有行为
            policy: Arc::new(RwLock::new(initial_policy)),
            origins: Arc::new(RwLock::new(origins)),
            origins_path,
            providers: Arc::new(RwLock::new(providers)),
            providers_path,
        }
    }

    pub async fn policy(&self) -> RoutePolicy {
        *self.policy.read().await
    }

    pub async fn set_policy(&self, policy: RoutePolicy) {
        *self.policy.write().await = policy;
        tracing::info!("Backend route policy set to {}", policy);
    }

    /// 记录某个 CID 由哪个后端产生（add 成功后调用）。
    ///
    /// 这是把「前缀猜测」升级为「来源标记」的核心：一旦某 CID 被本机某后端
    /// 添加过，其归属就是**已知事实**，`Auto` 路由据此精确分发，不再依赖猜测。
    pub async fn record_origin(&self, cid: &str, backend: BackendType) {
        {
            let mut map = self.origins.write().await;
            if map.get(cid) == Some(&backend) {
                return; // 无变化，免去落盘
            }
            map.insert(cid.to_string(), backend);
        }
        let snapshot = self.origins.read().await.clone();
        persist_json_map(&self.origins_path, &snapshot);
    }

    /// 查询已知来源标记
    pub async fn known_origin(&self, cid: &str) -> Option<BackendType> {
        self.origins.read().await.get(cid.trim()).copied()
    }

    /// 记录某 CID 的 iroh provider（一段 BlobTicket 字符串），供网络 fallback 用。
    ///
    /// 这把 fallback 从「本地两后端」扩展到「iroh 网络远端」：即便本地都没有，
    /// 只要知道 provider，就能按内容 hash 从网络取回（跨节点内容发现）。
    pub async fn record_provider(&self, cid: &str, ticket: &str) {
        {
            let mut map = self.providers.write().await;
            if map.get(cid).map(|s| s.as_str()) == Some(ticket) {
                return;
            }
            map.insert(cid.to_string(), ticket.to_string());
        }
        let snapshot = self.providers.read().await.clone();
        persist_json_map(&self.providers_path, &snapshot);
    }

    /// 删除失效或不再信任的 provider ticket。
    pub async fn forget_provider(&self, cid: &str) {
        self.providers.write().await.remove(cid.trim());
        let snapshot = self.providers.read().await.clone();
        persist_json_map(&self.providers_path, &snapshot);
    }

    /// 查询已知 provider ticket
    pub async fn known_provider(&self, cid: &str) -> Option<String> {
        self.providers.read().await.get(cid.trim()).cloned()
    }

    /// 网络 fallback：若已知该 CID 的 iroh provider，则从网络拉取（仅 `Auto`）。
    ///
    /// 返回 `None` 表示「无网络 provider 可试」；`Some(Ok/Err)` 为一次实际尝试结果。
    /// 成功后回填来源标记为 iroh（自愈，下次可直达本地）。
    pub async fn try_network_fetch(&self, cid: &str) -> Option<Result<Vec<u8>, BackendError>> {
        if !matches!(self.policy().await, RoutePolicy::Auto) {
            return None;
        }
        let ticket = self.known_provider(cid).await?;
        match self.iroh.fetch_ticket(&ticket).await {
            Ok(bytes) => {
                self.record_origin(cid, BackendType::Iroh).await;
                tracing::info!("cat network-fallback hit via provider for {}", cid);
                Some(Ok(bytes))
            }
            Err(e) => {
                // ticket 失效时不无限重试，也避免长期保存过期访问凭证。
                self.forget_provider(cid).await;
                Some(Err(e))
            }
        }
    }

    /// 基于前缀的**启发式**内容分类（仅在无来源标记时作为回退）。
    ///
    /// - IPFS CID（`Qm...` v0 / `baf...` v1 常见多编码前缀）→ Kubo
    /// - 其余（含 iroh 的 BLAKE3 内容哈希）→ Iroh
    pub fn classify_cid(cid: &str) -> BackendType {
        let c = cid.trim();
        let looks_ipfs = c.starts_with("Qm")            // CIDv0 base58btc
            || c.starts_with("baf")                     // CIDv1 base32（bafy/bafk/bafr/bafz/baga...）
            || c.starts_with("bag")
            || c.starts_with("Qmb");
        if looks_ipfs {
            BackendType::Kubo
        } else {
            BackendType::Iroh
        }
    }

    /// 针对某个 CID 的读操作，按当前策略决定后端。
    ///
    /// `Auto` 决策链（按内容实际所在路由，Phase C 核心）：
    /// 1. **来源标记**（已知事实）——最强；
    /// 2. **内容发现**——iroh 本地确有该 blob 则走 iroh（不靠猜测，靠实测）；
    /// 3. **前缀启发式**——兜底（`Qm.../baf...` → Kubo，其余 → iroh）。
    pub async fn choose_for_cid(&self, cid: &str) -> BackendType {
        match self.policy().await {
            RoutePolicy::KuboOnly => BackendType::Kubo,
            RoutePolicy::IrohOnly => BackendType::Iroh,
            RoutePolicy::Auto => {
                // 1. 已知来源标记
                if let Some(t) = self.known_origin(cid).await {
                    return t;
                }
                // 2. 内容发现：iroh 本地真有该 blob 就走 iroh
                if self.iroh.has(cid).await.unwrap_or(false) {
                    return BackendType::Iroh;
                }
                // 3. 兜底启发式
                Self::classify_cid(cid)
            }
        }
    }

    /// 写操作（add）后端选择：可带偏好；无偏好时按策略（Auto 默认落 Kubo 以保证公网可寻址）
    pub async fn choose_for_add(&self, prefer: Option<BackendType>) -> BackendType {
        if let Some(p) = prefer {
            return p;
        }
        match self.policy().await {
            RoutePolicy::KuboOnly | RoutePolicy::Auto => BackendType::Kubo,
            RoutePolicy::IrohOnly => BackendType::Iroh,
        }
    }

    fn backend(&self, t: BackendType) -> &dyn Backend {
        match t {
            BackendType::Kubo => self.kubo.as_ref(),
            BackendType::Iroh => self.iroh.as_ref(),
        }
    }

    /// 读取内容的**后端尝试顺序**（单一决策来源，供 router 与 commands 共用）。
    ///
    /// `Auto` 下返回 `[主选, 另一个]`——即「主后端取不到就 fallback」；
    /// `KuboOnly`/`IrohOnly` 是显式选择，只返回单个后端，不做跨栈 fallback。
    pub async fn cat_order(&self, cid: &str) -> Vec<BackendType> {
        let primary = self.choose_for_cid(cid).await;
        if matches!(self.policy().await, RoutePolicy::Auto) {
            let other = match primary {
                BackendType::Kubo => BackendType::Iroh,
                BackendType::Iroh => BackendType::Kubo,
            };
            vec![primary, other]
        } else {
            vec![primary]
        }
    }

    /// 按路由读取内容（cat），**带跨后端 fallback-on-miss**（双栈韧性）。
    ///
    /// 主后端失败时（Auto 下）自动试另一个；fallback 命中后回填来源标记（自愈，
    /// 下次直达）。全部失败则返回**主后端**的错误（信息量更大）。
    pub async fn cat(&self, cid: &str) -> Result<(BackendType, Vec<u8>), BackendError> {
        let order = self.cat_order(cid).await;
        let mut first_err: Option<BackendError> = None;
        for (i, t) in order.iter().enumerate() {
            match self.backend(*t).cat(cid).await {
                Ok(bytes) => {
                    if i > 0 {
                        // fallback 命中 → 回填来源标记，实现自愈路由
                        self.record_origin(cid, *t).await;
                        tracing::info!("cat fallback hit on {:?} for {}", t, cid);
                    }
                    return Ok((*t, bytes));
                }
                Err(e) => {
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
            }
        }
        // 本地两栈都 miss → 网络 fallback（若已知 iroh provider，跨节点取回）
        if let Some(Ok(bytes)) = self.try_network_fetch(cid).await {
            return Ok((BackendType::Iroh, bytes));
        }
        Err(first_err.unwrap_or_else(|| BackendError::not_found("no backend produced content")))
    }

    /// 按路由添加文件（add），成功后记录内容来源标记
    pub async fn add_file(
        &self,
        path: &std::path::Path,
        prefer: Option<BackendType>,
    ) -> Result<(BackendType, AddOutput), BackendError> {
        let t = self.choose_for_add(prefer).await;
        let out = self.backend(t).add_file(path).await?;
        self.record_origin(&out.cid, t).await;
        Ok((t, out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn router() -> BackendRouter {
        let kubo = Arc::new(KuboBackend::new("http://127.0.0.1:5001".to_string()));
        let dir = std::env::temp_dir().join("router-test-iroh");
        let iroh = Arc::new(IrohBackend::new(dir));
        // 测试用内存来源标记（origins_path = None）
        BackendRouter::new(kubo, iroh, None)
    }

    #[test]
    fn test_classify_ipfs_cids_go_to_kubo() {
        // CIDv0
        assert_eq!(
            BackendRouter::classify_cid("QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG"),
            BackendType::Kubo
        );
        // CIDv1 (dag-pb / raw)
        assert_eq!(
            BackendRouter::classify_cid(
                "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi"
            ),
            BackendType::Kubo
        );
        assert_eq!(
            BackendRouter::classify_cid("bafkreib2random"),
            BackendType::Kubo
        );
    }

    #[test]
    fn test_classify_non_ipfs_goes_to_iroh() {
        // iroh 的 BLAKE3 内容哈希（非 IPFS 多编码前缀）
        assert_eq!(
            BackendRouter::classify_cid("2fd4e1c67a2d28fced849ee1bb76e7391b93eb12"),
            BackendType::Iroh
        );
    }

    #[tokio::test]
    async fn test_default_policy_is_kubo_only() {
        let r = router();
        assert_eq!(r.policy().await, RoutePolicy::KuboOnly);
        // KuboOnly 下即便是 iroh 形态的 cid 也走 Kubo（零回归）
        assert_eq!(r.choose_for_cid("2fd4e1c67a").await, BackendType::Kubo);
    }

    #[tokio::test]
    async fn test_auto_policy_routes_by_content() {
        let r = router();
        r.set_policy(RoutePolicy::Auto).await;
        assert_eq!(r.choose_for_cid("QmHash").await, BackendType::Kubo);
        assert_eq!(r.choose_for_cid("2fd4e1c67a").await, BackendType::Iroh);
    }

    #[tokio::test]
    async fn test_origin_tag_overrides_heuristic() {
        let r = router();
        r.set_policy(RoutePolicy::Auto).await;

        // 一个「看起来像 IPFS」的 cid，但被标记为 iroh 产生 → 来源标记胜出
        r.record_origin("QmLooksLikeIpfsButIroh", BackendType::Iroh)
            .await;
        assert_eq!(
            r.choose_for_cid("QmLooksLikeIpfsButIroh").await,
            BackendType::Iroh,
            "explicit origin tag must override prefix heuristic"
        );

        // 反向：非 IPFS 形态但标记为 Kubo
        r.record_origin("deadbeef00", BackendType::Kubo).await;
        assert_eq!(r.choose_for_cid("deadbeef00").await, BackendType::Kubo);

        // 无标记则回退启发式
        assert_eq!(r.choose_for_cid("QmUntagged").await, BackendType::Kubo);
        assert_eq!(r.choose_for_cid("ffee00untagged").await, BackendType::Iroh);
    }

    #[tokio::test]
    async fn test_origin_and_provider_persistence_roundtrip() {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "router-persist-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let kubo = Arc::new(KuboBackend::new("http://127.0.0.1:5001".to_string()));
        let iroh = Arc::new(IrohBackend::new(
            std::env::temp_dir().join("router-persist-iroh"),
        ));

        {
            let r = BackendRouter::new(kubo.clone(), iroh.clone(), Some(dir.clone()));
            r.record_origin("cidX", BackendType::Iroh).await;
            r.record_provider("cidX", "ticket-abc").await;
        }
        // 新实例从磁盘恢复来源标记 + provider
        let r2 = BackendRouter::new(kubo, iroh, Some(dir.clone()));
        assert_eq!(r2.known_origin("cidX").await, Some(BackendType::Iroh));
        assert_eq!(
            r2.known_provider("cidX").await,
            Some("ticket-abc".to_string())
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_iroh_only_policy_overrides() {
        let r = router();
        r.set_policy(RoutePolicy::IrohOnly).await;
        assert_eq!(r.choose_for_cid("QmHash").await, BackendType::Iroh);
        assert_eq!(r.choose_for_add(None).await, BackendType::Iroh);
    }

    #[tokio::test]
    async fn test_add_prefer_overrides_policy() {
        let r = router();
        // 默认 KuboOnly，但显式偏好 iroh 应被尊重
        assert_eq!(
            r.choose_for_add(Some(BackendType::Iroh)).await,
            BackendType::Iroh
        );
    }

    /// Phase C 内容发现：无来源标记时，Auto 靠「iroh 本地确有该 blob」路由到 iroh
    #[cfg(feature = "iroh-backend")]
    #[tokio::test]
    async fn test_auto_content_discovery_probes_iroh_local() {
        let kubo = Arc::new(KuboBackend::new("http://127.0.0.1:5001".to_string()));
        let iroh = Arc::new(IrohBackend::new(
            std::env::temp_dir().join(format!("router-probe-iroh-{}", std::process::id())),
        ));
        let r = BackendRouter::new(kubo, iroh.clone(), None);
        r.set_policy(RoutePolicy::Auto).await;

        // 直接经 iroh 后端 add（绕过 router → 不记录来源标记）
        let payload = vec![9u8, 8, 7, 6, 5, 4, 3, 2, 1];
        let tmp = std::env::temp_dir().join(format!("router-probe-{}.bin", std::process::id()));
        tokio::fs::write(&tmp, &payload).await.unwrap();
        let out = iroh.add_file(&tmp).await.expect("iroh add");

        // 无来源标记，但 iroh 本地确有 → 靠内容发现路由到 Iroh
        assert!(
            r.known_origin(&out.cid).await.is_none(),
            "no origin tag expected"
        );
        assert_eq!(
            r.choose_for_cid(&out.cid).await,
            BackendType::Iroh,
            "Auto should route by real local presence, not by tag"
        );

        // iroh 本地没有的 IPFS CID → 探测 miss → 兜底启发式走 Kubo
        assert_eq!(
            r.choose_for_cid("QmSomethingNotLocalXYZ").await,
            BackendType::Kubo
        );

        let _ = tokio::fs::remove_file(&tmp).await;
    }

    /// 双栈韧性：primary 取不到 → 自动 fallback 到另一个后端 + 命中后自愈回填标记
    #[cfg(feature = "iroh-backend")]
    #[tokio::test]
    async fn test_auto_cat_fallback_on_miss() {
        // Kubo 指向一个没有服务的端口 → cat 必然失败（模拟「主后端取不到」）
        let kubo = Arc::new(KuboBackend::new("http://127.0.0.1:59998".to_string()));
        let iroh = Arc::new(IrohBackend::new(
            std::env::temp_dir().join(format!("router-fallback-iroh-{}", std::process::id())),
        ));
        let r = BackendRouter::new(kubo, iroh.clone(), None);
        r.set_policy(RoutePolicy::Auto).await;

        // 内容只存在于 iroh
        let payload = vec![42u8; 1234];
        let tmp = std::env::temp_dir().join(format!("router-fallback-{}.bin", std::process::id()));
        tokio::fs::write(&tmp, &payload).await.unwrap();
        let out = iroh.add_file(&tmp).await.expect("iroh add");

        // 故意打上**错误**的来源标记（Kubo），强制 primary=Kubo
        r.record_origin(&out.cid, BackendType::Kubo).await;
        assert_eq!(r.choose_for_cid(&out.cid).await, BackendType::Kubo);

        // cat：primary=Kubo（死端口→失败）→ fallback 到 iroh → 命中
        let (used, bytes) = r.cat(&out.cid).await.expect("fallback should succeed");
        assert_eq!(used, BackendType::Iroh, "should fall back to iroh");
        assert_eq!(bytes, payload);

        // 自愈：fallback 命中后来源标记应被回填为 iroh
        assert_eq!(r.known_origin(&out.cid).await, Some(BackendType::Iroh));

        let _ = tokio::fs::remove_file(&tmp).await;
    }

    /// 跨节点内容发现：本地两栈都没有，但已知 iroh provider → 从网络取回。
    /// 节点 A add + 分享 ticket；B 的 router 登记该 provider；B cat 时从 A 拉取。
    #[cfg(feature = "iroh-backend")]
    #[tokio::test]
    async fn test_auto_cat_network_fallback_via_provider() {
        // A：内容提供者
        let node_a = IrohBackend::new(
            std::env::temp_dir().join(format!("router-netfb-A-{}", std::process::id())),
        );
        let payload = vec![7u8; 4096];
        let tmp = std::env::temp_dir().join(format!("router-netfb-{}.bin", std::process::id()));
        tokio::fs::write(&tmp, &payload).await.unwrap();
        let added = node_a.add_file(&tmp).await.expect("A add");
        let ticket = node_a.share_ticket(&added.cid).await.expect("A ticket");

        // B：本地没有该内容；Kubo 指向死端口
        let kubo = Arc::new(KuboBackend::new("http://127.0.0.1:59997".to_string()));
        let iroh_b = Arc::new(IrohBackend::new(
            std::env::temp_dir().join(format!("router-netfb-B-{}", std::process::id())),
        ));
        let r = BackendRouter::new(kubo, iroh_b, None);
        r.set_policy(RoutePolicy::Auto).await;

        // 登记 provider（B 知道去哪拿，但还没拿）
        r.record_provider(&added.cid, &ticket).await;

        // cat：本地 kubo(死端口) + iroh_b(无) 都 miss → 网络 fallback 从 A 取回
        let (used, bytes) =
            tokio::time::timeout(std::time::Duration::from_secs(30), r.cat(&added.cid))
                .await
                .expect("network fallback timed out")
                .expect("network fallback should succeed");

        assert_eq!(used, BackendType::Iroh);
        assert_eq!(bytes, payload, "content fetched cross-node must match");
        // 自愈：取回后标记为 iroh，下次本地直达
        assert_eq!(r.known_origin(&added.cid).await, Some(BackendType::Iroh));

        let _ = tokio::fs::remove_file(&tmp).await;
    }

    #[test]
    fn test_policy_parse_roundtrip() {
        assert_eq!(RoutePolicy::parse("auto"), Some(RoutePolicy::Auto));
        assert_eq!(RoutePolicy::parse("Kubo"), Some(RoutePolicy::KuboOnly));
        assert_eq!(RoutePolicy::parse("IROH_ONLY"), Some(RoutePolicy::IrohOnly));
        assert_eq!(RoutePolicy::parse("nope"), None);
    }
}
