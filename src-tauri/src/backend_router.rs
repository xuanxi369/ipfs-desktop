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
//! - **分类**使用 `ContentRef`、来源记录和本地内容探测。IPFS 引用必须能被 CID crate
//!   严格解析；iroh 引用必须显式写为 `iroh:<hash>`，或来自可信记录/实际探测。
//! - 默认策略为 `KuboOnly`，即在路由层被显式启用前，行为与现有单栈完全一致（零回归）。

use crate::backend_trait::{AddOutput, BackendError, BackendType, ContentBackend, ContentRef};
use crate::iroh_adapter::IrohBackend;
use crate::kubo_adapter::KuboBackend;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentMapping {
    pub kubo_cid: String,
    pub iroh_hash: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UsageMode {
    LocalFirst,
    Compatible,
    Mirrored,
}

impl UsageMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "localfirst" | "local_first" | "local" => Some(Self::LocalFirst),
            "compatible" | "compatibility" => Some(Self::Compatible),
            "mirrored" | "mirror" => Some(Self::Mirrored),
            _ => None,
        }
    }

    pub fn route_policy(self) -> RoutePolicy {
        match self {
            Self::LocalFirst => RoutePolicy::IrohOnly,
            Self::Compatible => RoutePolicy::Auto,
            Self::Mirrored => RoutePolicy::Mirror,
        }
    }

    pub fn from_legacy(policy: RoutePolicy) -> Self {
        match policy {
            RoutePolicy::IrohOnly => Self::LocalFirst,
            RoutePolicy::Mirror => Self::Mirrored,
            RoutePolicy::KuboOnly | RoutePolicy::Auto => Self::Compatible,
        }
    }
}

impl std::fmt::Display for UsageMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LocalFirst => write!(f, "LocalFirst"),
            Self::Compatible => write!(f, "Compatible"),
            Self::Mirrored => write!(f, "Mirrored"),
        }
    }
}

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
            if let Err(e) = crate::atomic_file::write_atomic(p, json.as_bytes()) {
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
    /// 按经过验证的内容引用自动选择
    Auto,
    /// Write to both backends and expose the Kubo CID as the primary reference.
    Mirror,
}

impl RoutePolicy {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "kubo" | "kubo_only" | "kuboonly" => Some(Self::KuboOnly),
            "iroh" | "iroh_only" | "irohonly" => Some(Self::IrohOnly),
            "auto" => Some(Self::Auto),
            "mirror" => Some(Self::Mirror),
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
            RoutePolicy::Mirror => write!(f, "Mirror"),
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
    mappings: Arc<RwLock<HashMap<String, ContentMapping>>>,
    mappings_path: Option<PathBuf>,
}

impl BackendRouter {
    /// `data_dir` 为空时来源标记 / provider 仅存内存（测试用）；
    /// 否则分别持久化到 `<data_dir>/cid_origins.json` 与 `<data_dir>/cid_providers.json`。
    pub fn new(kubo: Arc<KuboBackend>, iroh: Arc<IrohBackend>, data_dir: Option<PathBuf>) -> Self {
        Self::new_with_policy(kubo, iroh, data_dir, RoutePolicy::Auto)
    }

    pub fn new_with_policy(
        kubo: Arc<KuboBackend>,
        iroh: Arc<IrohBackend>,
        data_dir: Option<PathBuf>,
        initial_policy: RoutePolicy,
    ) -> Self {
        let origins_path = data_dir.as_ref().map(|d| d.join("cid_origins.json"));
        let providers_path = data_dir.as_ref().map(|d| d.join("cid_providers.json"));
        let mappings_path = data_dir.as_ref().map(|d| d.join("content_mappings.json"));

        // 启动时从磁盘恢复
        let origins = load_json_map::<BackendType>(&origins_path);
        let providers = load_json_map::<String>(&providers_path);
        let mappings = load_json_map::<ContentMapping>(&mappings_path);

        Self {
            kubo,
            iroh,
            // 默认 KuboOnly：在被显式切换前，路由层不改变任何现有行为
            policy: Arc::new(RwLock::new(initial_policy)),
            origins: Arc::new(RwLock::new(origins)),
            origins_path,
            providers: Arc::new(RwLock::new(providers)),
            providers_path,
            mappings: Arc::new(RwLock::new(mappings)),
            mappings_path,
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

    pub async fn known_mapping(&self, reference: &str) -> Option<ContentMapping> {
        self.mappings.read().await.get(reference.trim()).cloned()
    }

    async fn record_mapping(&self, mapping: ContentMapping) {
        {
            let mut mappings = self.mappings.write().await;
            mappings.insert(mapping.kubo_cid.clone(), mapping.clone());
            mappings.insert(mapping.iroh_hash.clone(), mapping);
        }
        let snapshot = self.mappings.read().await.clone();
        persist_json_map(&self.mappings_path, &snapshot);
    }

    pub async fn mapping_count(&self) -> usize {
        let mappings = self.mappings.read().await;
        mappings
            .values()
            .map(|mapping| mapping.kubo_cid.as_str())
            .collect::<std::collections::HashSet<_>>()
            .len()
    }

    /// 网络 fallback：若已知该 CID 的 iroh provider，则从网络拉取（仅 `Auto`）。
    ///
    /// 返回 `None` 表示「无网络 provider 可试」；`Some(Ok/Err)` 为一次实际尝试结果。
    /// 成功后回填来源标记为 iroh（自愈，下次可直达本地）。
    pub async fn try_network_fetch(&self, cid: &str) -> Option<Result<Vec<u8>, BackendError>> {
        if !matches!(self.policy().await, RoutePolicy::Auto | RoutePolicy::Mirror) {
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

    /// 针对某个 CID 的读操作，按当前策略决定后端。
    ///
    /// `Auto` 决策链（按内容实际所在路由，Phase C 核心）：
    /// 1. **来源标记**（已知事实）——最强；
    /// 2. **provider 记录**——已有 iroh ticket 是可验证的来源证据；
    /// 3. **内容发现**——iroh 本地确有该 blob 则走 iroh（不靠猜测，靠实测）；
    /// 4. **严格解析**——合法 CID 走 Kubo；iroh 必须显式标注或已被实际发现。
    pub async fn choose_for_cid(&self, cid: &str) -> Result<BackendType, BackendError> {
        match self.policy().await {
            RoutePolicy::KuboOnly => Ok(BackendType::Kubo),
            RoutePolicy::IrohOnly => Ok(BackendType::Iroh),
            RoutePolicy::Mirror => {
                if let Some(mapping) = self.known_mapping(cid).await {
                    if cid.trim() == mapping.iroh_hash {
                        Ok(BackendType::Iroh)
                    } else {
                        Ok(BackendType::Kubo)
                    }
                } else {
                    ContentRef::parse(cid).map(|reference| reference.backend_type())
                }
            }
            RoutePolicy::Auto => {
                // 1. 已知来源标记
                if let Some(t) = self.known_origin(cid).await {
                    return Ok(t);
                }
                // 2. 已登记 provider：ticket 明确指向 iroh 网络内容。
                if self.known_provider(cid).await.is_some() {
                    return Ok(BackendType::Iroh);
                }
                // 3. 内容发现：iroh 本地真有该 blob 就走 iroh
                if self.iroh.has(cid).await.unwrap_or(false) {
                    return Ok(BackendType::Iroh);
                }
                ContentRef::parse(cid).map(|reference| reference.backend_type())
            }
        }
    }

    /// 写操作（add）后端选择：可带偏好；Auto 默认写入 iroh，Kubo 仅用于按需兼容。
    pub async fn choose_for_add(&self, prefer: Option<BackendType>) -> BackendType {
        if let Some(p) = prefer {
            return p;
        }
        match self.policy().await {
            RoutePolicy::KuboOnly | RoutePolicy::Mirror => BackendType::Kubo,
            RoutePolicy::IrohOnly | RoutePolicy::Auto => BackendType::Iroh,
        }
    }

    fn content_backend(&self, t: BackendType) -> &dyn ContentBackend {
        match t {
            BackendType::Kubo => self.kubo.as_ref(),
            BackendType::Iroh => self.iroh.as_ref(),
        }
    }

    /// 读取内容的**后端尝试顺序**（单一决策来源，供 router 与 commands 共用）。
    ///
    /// `Auto` 下返回 `[主选, 另一个]`——即「主后端取不到就 fallback」；
    /// `KuboOnly`/`IrohOnly` 是显式选择，只返回单个后端，不做跨栈 fallback。
    pub async fn cat_order(&self, cid: &str) -> Result<Vec<BackendType>, BackendError> {
        let primary = self.choose_for_cid(cid).await?;
        if matches!(self.policy().await, RoutePolicy::Auto) {
            let other = match primary {
                BackendType::Kubo => BackendType::Iroh,
                BackendType::Iroh => BackendType::Kubo,
            };
            Ok(vec![primary, other])
        } else {
            Ok(vec![primary])
        }
    }

    /// 按路由读取内容（cat），**带跨后端 fallback-on-miss**（双栈韧性）。
    ///
    /// 主后端失败时（Auto 下）自动试另一个；fallback 命中后回填来源标记（自愈，
    /// 下次直达）。全部失败则返回**主后端**的错误（信息量更大）。
    pub async fn cat(&self, cid: &str) -> Result<(BackendType, Vec<u8>), BackendError> {
        if matches!(self.policy().await, RoutePolicy::Mirror) {
            let mapping = self
                .known_mapping(cid)
                .await
                .ok_or_else(|| BackendError::not_found("content has no verified mirror mapping"))?;
            let (kubo, iroh) = tokio::join!(
                self.kubo.cat(&mapping.kubo_cid),
                self.iroh.cat(&mapping.iroh_hash)
            );
            let kubo = kubo?;
            let iroh = iroh?;
            Self::verify_bytes(&kubo, &iroh, mapping.size, &mapping.sha256)?;
            return Ok((BackendType::Kubo, kubo));
        }
        let order = self.cat_order(cid).await?;
        let mut first_err: Option<BackendError> = None;
        for (i, t) in order.iter().enumerate() {
            match self.content_backend(*t).cat(cid).await {
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
        if prefer.is_none() && matches!(self.policy().await, RoutePolicy::Mirror) {
            return self.add_file_mirrored(path).await;
        }
        let t = self.choose_for_add(prefer).await;
        let out = self.content_backend(t).add_file(path).await?;
        ContentRef::from_backend(&out.cid, t)?;
        self.record_origin(&out.cid, t).await;
        Ok((t, out))
    }

    async fn add_file_mirrored(
        &self,
        path: &std::path::Path,
    ) -> Result<(BackendType, AddOutput), BackendError> {
        let source = tokio::fs::read(path)
            .await
            .map_err(|e| BackendError::internal(format!("failed to read mirror source: {e}")))?;
        let (kubo, iroh) = tokio::join!(self.kubo.add_file(path), self.iroh.add_file(path));
        let kubo = kubo?;
        let iroh = iroh?;
        let kubo_ref = ContentRef::from_backend(&kubo.cid, BackendType::Kubo)?;
        let iroh_ref = ContentRef::from_backend(&iroh.cid, BackendType::Iroh)?;
        let kubo_value = kubo_ref.value();
        let iroh_value = iroh_ref.value();
        let (kubo_bytes, iroh_bytes) =
            tokio::join!(self.kubo.cat(&kubo_value), self.iroh.cat(&iroh_value));
        let kubo_bytes = kubo_bytes?;
        let iroh_bytes = iroh_bytes?;
        let digest = Self::sha256(&source);
        Self::verify_bytes(&kubo_bytes, &iroh_bytes, source.len() as u64, &digest)?;
        if kubo_bytes != source {
            return Err(BackendError::internal(
                "mirror verification failed: backend bytes differ from source",
            ));
        }
        let mapping = ContentMapping {
            kubo_cid: kubo.cid.clone(),
            iroh_hash: iroh.cid.clone(),
            size: source.len() as u64,
            sha256: digest,
        };
        self.record_mapping(mapping).await;
        self.record_origin(&kubo.cid, BackendType::Kubo).await;
        self.record_origin(&iroh.cid, BackendType::Iroh).await;
        Ok((BackendType::Kubo, kubo))
    }

    fn sha256(bytes: &[u8]) -> String {
        use sha2::Digest;
        format!("{:x}", sha2::Sha256::digest(bytes))
    }

    fn verify_bytes(
        kubo: &[u8],
        iroh: &[u8],
        expected_size: u64,
        expected_sha256: &str,
    ) -> Result<(), BackendError> {
        if kubo != iroh
            || kubo.len() as u64 != expected_size
            || Self::sha256(kubo) != expected_sha256
        {
            return Err(BackendError::internal("mirror byte verification failed"));
        }
        Ok(())
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
    fn content_ref_requires_semantic_cid_or_explicit_iroh_kind() {
        let cid = "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG";
        assert_eq!(
            ContentRef::parse(cid).unwrap().backend_type(),
            BackendType::Kubo
        );
        assert_eq!(
            ContentRef::parse("iroh:2fd4e1c67a2d28fced849ee1bb76e7391b93eb12")
                .unwrap()
                .backend_type(),
            BackendType::Iroh
        );
        assert!(ContentRef::parse("bafkreib2random").is_err());
        assert!(ContentRef::parse("2fd4e1c67a2d28fced849ee1bb76e7391b93eb12").is_err());
    }

    #[tokio::test]
    async fn test_default_policy_is_auto_with_iroh_writes() {
        let r = router();
        assert_eq!(r.policy().await, RoutePolicy::Auto);
        assert_eq!(r.choose_for_add(None).await, BackendType::Iroh);
    }

    #[tokio::test]
    async fn test_auto_policy_routes_by_content() {
        let r = router();
        r.set_policy(RoutePolicy::Auto).await;
        let cid = "QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG";
        assert_eq!(r.choose_for_cid(cid).await.unwrap(), BackendType::Kubo);
        assert_eq!(
            r.choose_for_cid("iroh:2fd4e1c67a").await.unwrap(),
            BackendType::Iroh
        );
        assert!(r.choose_for_cid("2fd4e1c67a").await.is_err());
    }

    #[tokio::test]
    async fn test_origin_tag_overrides_heuristic() {
        let r = router();
        r.set_policy(RoutePolicy::Auto).await;

        // 一个「看起来像 IPFS」的 cid，但被标记为 iroh 产生 → 来源标记胜出
        r.record_origin("QmLooksLikeIpfsButIroh", BackendType::Iroh)
            .await;
        assert_eq!(
            r.choose_for_cid("QmLooksLikeIpfsButIroh").await.unwrap(),
            BackendType::Iroh,
            "explicit origin tag must override prefix heuristic"
        );

        // 反向：非 IPFS 形态但标记为 Kubo
        r.record_origin("deadbeef00", BackendType::Kubo).await;
        assert_eq!(
            r.choose_for_cid("deadbeef00").await.unwrap(),
            BackendType::Kubo
        );

        // 无标记的歧义字符串必须被拒绝，不能回退到前缀猜测。
        assert!(r.choose_for_cid("QmUntagged").await.is_err());
        assert!(r.choose_for_cid("ffee00untagged").await.is_err());
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
        assert_eq!(r.choose_for_cid("QmHash").await.unwrap(), BackendType::Iroh);
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
            r.choose_for_cid(&out.cid).await.unwrap(),
            BackendType::Iroh,
            "Auto should route by real local presence, not by tag"
        );

        // 看起来像 CID 但语义无效的值不能再靠前缀路由。
        assert!(r.choose_for_cid("QmSomethingNotLocalXYZ").await.is_err());

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
        assert_eq!(r.choose_for_cid(&out.cid).await.unwrap(), BackendType::Kubo);

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
        assert_eq!(RoutePolicy::parse("mirror"), Some(RoutePolicy::Mirror));
        assert_eq!(RoutePolicy::parse("nope"), None);
    }

    #[test]
    fn usage_modes_map_to_internal_policies() {
        assert_eq!(UsageMode::parse("local_first"), Some(UsageMode::LocalFirst));
        assert_eq!(UsageMode::LocalFirst.route_policy(), RoutePolicy::IrohOnly);
        assert_eq!(UsageMode::Compatible.route_policy(), RoutePolicy::Auto);
        assert_eq!(UsageMode::Mirrored.route_policy(), RoutePolicy::Mirror);
        assert_eq!(
            UsageMode::from_legacy(RoutePolicy::KuboOnly),
            UsageMode::Compatible
        );
    }

    #[test]
    fn mirror_byte_verification_rejects_divergence() {
        let bytes = b"same bytes";
        let digest = BackendRouter::sha256(bytes);
        BackendRouter::verify_bytes(bytes, bytes, bytes.len() as u64, &digest).unwrap();
        assert!(
            BackendRouter::verify_bytes(bytes, b"different", bytes.len() as u64, &digest).is_err()
        );
    }

    #[tokio::test]
    async fn content_mapping_persists_under_both_references() {
        let dir = tempfile::tempdir().unwrap();
        let kubo = Arc::new(KuboBackend::new("http://127.0.0.1:5001".to_string()));
        let iroh = Arc::new(IrohBackend::new(dir.path().join("iroh")));
        let mapping = ContentMapping {
            kubo_cid: "QmMapping".into(),
            iroh_hash: "irohhash".into(),
            size: 42,
            sha256: "00".repeat(32),
        };
        {
            let router = BackendRouter::new(kubo.clone(), iroh.clone(), Some(dir.path().into()));
            router.record_mapping(mapping.clone()).await;
        }
        let restored = BackendRouter::new(kubo, iroh, Some(dir.path().into()));
        assert_eq!(
            restored.known_mapping("QmMapping").await,
            Some(mapping.clone())
        );
        assert_eq!(restored.known_mapping("irohhash").await, Some(mapping));
    }

    #[cfg(feature = "iroh-backend")]
    #[tokio::test]
    async fn mirror_dual_write_and_read_verifies_real_backends_when_kubo_is_available() {
        let kubo = Arc::new(KuboBackend::new("http://127.0.0.1:5001".to_string()));
        if !crate::backend_trait::Backend::is_available(kubo.as_ref()).await {
            eprintln!("SKIP: Mirror integration requires local Kubo on 127.0.0.1:5001");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let iroh = Arc::new(IrohBackend::new(dir.path().join("iroh")));
        let router = BackendRouter::new_with_policy(
            kubo,
            iroh,
            Some(dir.path().into()),
            RoutePolicy::Mirror,
        );
        let payload: Vec<u8> = (0..8192u32).map(|n| (n % 251) as u8).collect();
        let source = dir.path().join("mirror.bin");
        tokio::fs::write(&source, &payload).await.unwrap();

        let (_, output) = router.add_file(&source, None).await.expect("mirror add");
        let mapping = router
            .known_mapping(&output.cid)
            .await
            .expect("mapping must be committed after verification");
        assert_ne!(mapping.kubo_cid, mapping.iroh_hash);
        let (_, bytes) = router.cat(&output.cid).await.expect("verified mirror read");
        assert_eq!(bytes, payload);
    }
}
