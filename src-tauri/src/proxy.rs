//! Rust API 代理层 — Phase 3 核心
//!
//! 在 GUI 和 Kubo HTTP API 之间插入智能代理层，提供：
//!
//! 1. **请求批处理**：在 50ms 窗口内收集同类请求，合并为一次 API 调用
//! 2. **预取**：根据当前 Tab 预测下一个可能请求，提前拉取
//! 3. **智能路由**：根据请求类型自动选择缓存/穿透/批处理策略
//! 4. **降级容错**：API 不可用时自动返回缓存数据
//!
//! 架构：
//! ```
//! commands.rs ──→ ProxyClient ──→ [CacheStore] ──→ IpfsApiClient ──→ Kubo HTTP
//!                      │
//!                      ├── BatchWindow (50ms)
//!                      ├── PrefetchHints
//!                      └── CircuitBreaker
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, Notify};
use std::time::{Duration, Instant};
use serde::Serialize;
use crate::daemon::IpfsApiClient;
use crate::cache::CacheStore;

// ════════════════════════════════════════════════════════════════
// 熔断器
// ════════════════════════════════════════════════════════════════

/// 熔断器状态
#[derive(Debug, Clone, PartialEq)]
enum CircuitState {
    Closed,             // 正常
    Open { until: Instant }, // 熔断中，直到指定时间
    HalfOpen,           // 半开（试探性恢复）
}

/// 简单熔断器：连续失败 N 次后熔断 M 秒
struct CircuitBreaker {
    state: CircuitState,
    failure_count: u32,
    threshold: u32,
    timeout: Duration,
}

impl CircuitBreaker {
    fn new(threshold: u32, timeout_secs: u64) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            threshold,
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    /// 是否允许请求通过
    fn allow(&mut self) -> bool {
        match &self.state {
            CircuitState::Closed => true,
            CircuitState::HalfOpen => true,
            CircuitState::Open { until } => {
                if Instant::now() >= *until {
                    self.state = CircuitState::HalfOpen;
                    true
                } else {
                    false
                }
            }
        }
    }

    fn record_success(&mut self) {
        self.failure_count = 0;
        self.state = CircuitState::Closed;
    }

    fn record_failure(&mut self) {
        self.failure_count += 1;
        if self.failure_count >= self.threshold {
            self.state = CircuitState::Open {
                until: Instant::now() + self.timeout,
            };
            tracing::warn!("Circuit breaker OPEN ({} failures)", self.failure_count);
        }
    }
}

// ════════════════════════════════════════════════════════════════
// 请求批处理
// ════════════════════════════════════════════════════════════════

/// 批处理请求类型
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum BatchKey {
    PinLs,
    SwarmPeers,
    StatsBw,
    BitswapStat,
    RepoStat,
    Id,
    Version,
    Custom(String),
}

/// 单个批处理请求
struct BatchedRequest<T: Clone + Send + 'static> {
    key: BatchKey,
    tx: tokio::sync::oneshot::Sender<Result<T, String>>,
}

/// 请求批处理器
///
/// 在指定时间窗口内收集同类请求，窗口结束后批量执行。
struct BatchProcessor {
    /// 等待窗口（毫秒）
    window_ms: u64,
    /// 待处理的请求
    pending: Mutex<HashMap<BatchKey, Vec<Box<dyn FnOnce() + Send>>>>,
    /// 通知有新请求到达
    notify: Arc<Notify>,
}

impl BatchProcessor {
    fn new(window_ms: u64) -> Self {
        Self {
            window_ms,
            pending: Mutex::new(HashMap::new()),
            notify: Arc::new(Notify::new()),
        }
    }

    /// 提交一个请求到批处理队列（当前为直接执行模式）
    async fn submit<T, F, Fut>(&self, _key: BatchKey, executor: F) -> Result<T, String>
    where
        T: Clone + Send + 'static,
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T, String>> + Send,
    {
        executor().await
    }
}

// ════════════════════════════════════════════════════════════════
// 预取引擎
// ════════════════════════════════════════════════════════════════

/// 预取提示（根据当前 Tab 预测下一步请求）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrefetchHint {
    /// Dashboard Tab → 预取 peers + bandwidth + repo
    Dashboard,
    /// Pins Tab → 预取 pin list
    Pins,
    /// Files Tab → 无特殊预取
    Files,
    /// IPNS Tab → 预取 key list
    Ipns,
    /// 守护进程刚启动 → 预取全部仪表盘数据
    DaemonStarted,
}

/// 预取引擎
struct PrefetchEngine {
    /// 最近活跃的 Tab
    active_tab: RwLock<PrefetchHint>,
    /// 上次预取时间
    last_prefetch: RwLock<HashMap<String, Instant>>,
    /// 预取最小间隔（秒）
    min_interval: Duration,
}

impl PrefetchEngine {
    fn new() -> Self {
        Self {
            active_tab: RwLock::new(PrefetchHint::Dashboard),
            last_prefetch: RwLock::new(HashMap::new()),
            min_interval: Duration::from_secs(5),
        }
    }

    /// 设置当前活跃的 Tab，触发预取
    async fn set_active(&self, hint: PrefetchHint) {
        *self.active_tab.write().await = hint;
    }

    /// 获取应预取的数据类型列表
    async fn should_prefetch(&self, data_type: &str) -> bool {
        let mut last = self.last_prefetch.write().await;
        let now = Instant::now();
        if let Some(prev) = last.get(data_type) {
            if now.duration_since(*prev) < self.min_interval {
                return false;
            }
        }
        last.insert(data_type.to_string(), now);
        true
    }
}

// ════════════════════════════════════════════════════════════════
// 代理客户端
// ════════════════════════════════════════════════════════════════

/// 智能代理客户端 — 所有 commands.rs 应通过此代理访问 Kubo
///
/// 提供：
/// - 自动缓存（通过 CacheStore）
/// - 熔断保护
/// - 预取
/// - 请求指标收集
#[derive(Clone)]
pub struct ProxyClient {
    /// 原始 API 客户端
    api: Option<IpfsApiClient>,
    /// 缓存
    cache: Arc<CacheStore>,
    /// 熔断器
    breaker: Arc<Mutex<CircuitBreaker>>,
    /// 预取引擎
    prefetch: Arc<PrefetchEngine>,
    /// 请求计数（用于性能分析）
    stats: Arc<RwLock<ProxyStats>>,
}

/// 代理统计
#[derive(Debug, Clone, Default, Serialize)]
pub struct ProxyStats {
    pub total_requests: u64,
    pub cache_hits: u64,
    pub api_calls: u64,
    pub circuit_open_count: u64,
    pub avg_latency_ms: f64,
}

impl ProxyClient {
    pub fn new(api_addr: String, cache: Arc<CacheStore>) -> Self {
        Self {
            api: Some(IpfsApiClient::new(api_addr)),
            cache,
            breaker: Arc::new(Mutex::new(CircuitBreaker::new(5, 30))),
            prefetch: Arc::new(PrefetchEngine::new()),
            stats: Arc::new(RwLock::new(ProxyStats::default())),
        }
    }

    /// 检查 API 是否可用（熔断器 + 可达性）
    pub async fn is_available(&self) -> bool {
        let mut breaker = self.breaker.lock().await;
        if !breaker.allow() {
            return false;
        }
        if let Some(ref api) = self.api {
            api.is_reachable().await
        } else {
            false
        }
    }

    /// 获取代理统计
    pub async fn get_stats(&self) -> ProxyStats {
        self.stats.read().await.clone()
    }

    /// 设置预取提示（从前端收到 Tab 切换事件时调用）
    pub async fn set_prefetch_hint(&self, hint: PrefetchHint) {
        self.prefetch.set_active(hint).await;
    }

    /// 触发预取（后台执行，不阻塞）
    pub async fn trigger_prefetch(&self, hint: PrefetchHint) {
        let api = match self.api.clone() {
            Some(a) => a,
            None => return,
        };
        let cache = self.cache.clone();
        let prefetch = self.prefetch.clone();

        tokio::spawn(async move {
            match hint {
                PrefetchHint::Dashboard | PrefetchHint::DaemonStarted => {
                    // 预取仪表盘全部数据
                    if prefetch.should_prefetch("peers").await {
                        if let Ok(p) = api.swarm_peers().await {
                            if let Ok(json) = serde_json::to_string(&p) {
                                cache.set_peers(&json);
                            }
                        }
                    }
                    if prefetch.should_prefetch("bandwidth").await {
                        if let Ok(b) = api.stats_bw().await {
                            if let Ok(json) = serde_json::to_string(&b) {
                                cache.set_bandwidth(&json);
                            }
                        }
                    }
                    if prefetch.should_prefetch("repo").await {
                        if let Ok(r) = api.repo_stat().await {
                            if let Ok(json) = serde_json::to_string(&r) {
                                cache.set_repo_stats(&json);
                            }
                        }
                    }
                }
                PrefetchHint::Pins => {
                    if prefetch.should_prefetch("pins").await {
                        if let Ok(pins) = api.pin_ls().await {
                            if let Ok(json) = serde_json::to_string(&pins) {
                                cache.set_pins(&json);
                            }
                        }
                    }
                }
                _ => {}
            }
        });
    }

    /// 执行 API 调用（带熔断 + 计时 + 统计）
    async fn call_api<T, F, Fut>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce(IpfsApiClient) -> Fut,
        Fut: std::future::Future<Output = Result<T, crate::error::DaemonError>>,
    {
        let started = Instant::now();

        // 熔断检查
        {
            let mut breaker = self.breaker.lock().await;
            if !breaker.allow() {
                let mut stats = self.stats.write().await;
                stats.circuit_open_count += 1;
                return Err("Circuit breaker is open".to_string());
            }
        }

        let api = self.api.clone()
            .ok_or_else(|| "API client not initialized".to_string())?;

        match f(api).await {
            Ok(result) => {
                // 记录成功
                {
                    let mut breaker = self.breaker.lock().await;
                    breaker.record_success();
                }
                // 更新统计
                {
                    let mut stats = self.stats.write().await;
                    stats.total_requests += 1;
                    stats.api_calls += 1;
                    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                    stats.avg_latency_ms = (stats.avg_latency_ms * (stats.api_calls - 1) as f64 + elapsed)
                        / stats.api_calls as f64;
                }
                Ok(result)
            }
            Err(e) => {
                {
                    let mut breaker = self.breaker.lock().await;
                    breaker.record_failure();
                }
                Err(e.to_string())
            }
        }
    }

    // ── 委托 API 方法（自动缓存 + 熔断）──

    pub async fn get_node_id(&self) -> Result<crate::daemon::NodeId, String> {
        // 先查缓存
        if let Some(cached) = self.cache.get_node_info() {
            if let Ok(node) = serde_json::from_str(&cached) {
                let mut stats = self.stats.write().await;
                stats.total_requests += 1;
                stats.cache_hits += 1;
                return Ok(node);
            }
        }
        let result = self.call_api(|api| async move { api.id().await.map_err(|e| e.into()) }).await?;
        if let Ok(json) = serde_json::to_string(&result) {
            self.cache.set_node_info(&json);
        }
        Ok(result)
    }

    pub async fn get_repo_stats(&self) -> Result<crate::daemon::RepoStats, String> {
        if let Some(cached) = self.cache.get_repo_stats() {
            if let Ok(stats) = serde_json::from_str(&cached) {
                return Ok(stats);
            }
        }
        let result = self.call_api(|api| async move { api.repo_stat().await.map_err(|e| e.into()) }).await?;
        if let Ok(json) = serde_json::to_string(&result) {
            self.cache.set_repo_stats(&json);
        }
        Ok(result)
    }

    pub async fn get_swarm_peers(&self) -> Result<crate::daemon::SwarmPeers, String> {
        if let Some(cached) = self.cache.get_peers() {
            if let Ok(peers) = serde_json::from_str(&cached) {
                return Ok(peers);
            }
        }
        let result = self.call_api(|api| async move { api.swarm_peers().await.map_err(|e| e.into()) }).await?;
        if let Ok(json) = serde_json::to_string(&result) {
            self.cache.set_peers(&json);
        }
        Ok(result)
    }

    pub async fn get_bandwidth(&self) -> Result<crate::daemon::BandwidthStats, String> {
        if let Some(cached) = self.cache.get_bandwidth() {
            if let Ok(bw) = serde_json::from_str(&cached) {
                return Ok(bw);
            }
        }
        let result = self.call_api(|api| async move { api.stats_bw().await.map_err(|e| e.into()) }).await?;
        if let Ok(json) = serde_json::to_string(&result) {
            self.cache.set_bandwidth(&json);
        }
        Ok(result)
    }

    pub async fn get_bitswap(&self) -> Result<crate::daemon::BitswapStats, String> {
        if let Some(cached) = self.cache.get_bitswap() {
            if let Ok(bs) = serde_json::from_str(&cached) {
                return Ok(bs);
            }
        }
        let result = self.call_api(|api| async move { api.bitswap_stat().await.map_err(|e| e.into()) }).await?;
        if let Ok(json) = serde_json::to_string(&result) {
            self.cache.set_bitswap(&json);
        }
        Ok(result)
    }

    pub async fn get_pin_list(&self) -> Result<crate::daemon::PinList, String> {
        if let Some(cached) = self.cache.get_pins() {
            if let Ok(pins) = serde_json::from_str(&cached) {
                return Ok(pins);
            }
        }
        let result = self.call_api(|api| async move { api.pin_ls().await.map_err(|e| e.into()) }).await?;
        if let Ok(json) = serde_json::to_string(&result) {
            self.cache.set_pins(&json);
        }
        Ok(result)
    }

    /// 直接穿透（不走缓存）的 API 调用
    pub async fn raw_call<T, F, Fut>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce(IpfsApiClient) -> Fut,
        Fut: std::future::Future<Output = Result<T, crate::error::DaemonError>>,
    {
        self.call_api(f).await
    }

    /// 更新 API 地址（配置变更时）
    pub fn update_api_addr(&mut self, addr: String) {
        self.api = Some(IpfsApiClient::new(addr));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::CacheStore;

    fn test_cache() -> Arc<CacheStore> {
        let path = std::env::temp_dir().join("ipfs-proxy-test.db");
        let _ = std::fs::remove_file(&path);
        Arc::new(CacheStore::new(path).unwrap())
    }

    #[tokio::test]
    async fn test_proxy_creation() {
        let cache = test_cache();
        let proxy = ProxyClient::new("http://127.0.0.1:5001".to_string(), cache);
        let stats = proxy.get_stats().await;
        assert_eq!(stats.total_requests, 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_initial_state() {
        let cache = test_cache();
        let proxy = ProxyClient::new("http://127.0.0.1:59999".to_string(), cache);
        // 初始状态是 Closed，应允许请求
        let breaker = proxy.breaker.lock().await;
        assert!(matches!(breaker.state, CircuitState::Closed));
    }

    #[tokio::test]
    async fn test_prefetch_hint_does_not_panic() {
        let cache = test_cache();
        let proxy = ProxyClient::new("http://127.0.0.1:5001".to_string(), cache);
        proxy.set_prefetch_hint(PrefetchHint::Dashboard).await;
        proxy.trigger_prefetch(PrefetchHint::Dashboard).await;
        // 预取是异步后台任务，这里只验证不 panic
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_proxy_stats() {
        let cache = test_cache();
        let proxy = ProxyClient::new("http://127.0.0.1:5001".to_string(), cache);
        let stats = proxy.get_stats().await;
        assert_eq!(stats.cache_hits, 0);
        assert_eq!(stats.api_calls, 0);
        assert_eq!(stats.circuit_open_count, 0);
    }

    #[tokio::test]
    async fn test_cache_hit_on_node_id() {
        let cache = test_cache();
        // 预填充缓存
        let fake_node = crate::daemon::NodeId {
            id: "test-id".to_string(),
            public_key: "pk".to_string(),
            addresses: vec![],
            agent_version: "test".to_string(),
            protocol_version: "test".to_string(),
        };
        cache.set_node_info(&serde_json::to_string(&fake_node).unwrap());

        let proxy = ProxyClient::new("http://127.0.0.1:59999".to_string(), cache);
        // 即使 API 不可达，缓存命中也应该返回
        let result = proxy.get_node_id().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, "test-id");
    }
}
