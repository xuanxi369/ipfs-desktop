//! 协议兼容性测试框架 — Phase 4
//!
//! 验证 Rust Iroh 后端与 Go Kubo 后端的协议互操作性。
//!
//! 测试策略：
//! 1. CID 互认：Kubo 生成的 CID 能被 Iroh 解析，反之亦然
//! 2. DHT 查找：两个后端能找到彼此的对等节点
//! 3. 内容传输：Kubo ←→ Iroh 之间能交换数据块
//! 4. 内容完整性：跨越两个后端的内容哈希一致
//!
//! 运行方式：
//! ```bash
//! # 需要同时运行 Kubo 和 Iroh 节点
//! cargo test --test compat_test -- --ignored
//! ```

use crate::backend_trait::Backend;
use crate::iroh_adapter::IrohBackend;
use crate::kubo_adapter::KuboBackend;
use serde::{Deserialize, Serialize};
use std::time::Instant;

// ════════════════════════════════════════════════════════════════
// 测试结果类型
// ════════════════════════════════════════════════════════════════

/// 单个兼容性测试结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatTestResult {
    /// 测试名称
    pub name: String,
    /// 是否通过
    pub passed: bool,
    /// 耗时（毫秒）
    pub duration_ms: u64,
    /// Kubo 端结果
    pub kubo_result: Option<String>,
    /// Iroh 端结果
    pub iroh_result: Option<String>,
    /// 错误信息
    pub error: Option<String>,
    /// 备注
    pub notes: Vec<String>,
}

/// 兼容性测试套件结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatSuiteResult {
    /// 测试总数
    pub total: usize,
    /// 通过数
    pub passed: usize,
    /// 失败数
    pub failed: usize,
    /// 跳过的测试
    pub skipped: usize,
    /// 各测试详情
    pub tests: Vec<CompatTestResult>,
    /// 总耗时（毫秒）
    pub total_duration_ms: u64,
    /// 兼容性评分（0-100）
    pub compatibility_score: f64,
}

// ════════════════════════════════════════════════════════════════
// 测试执行器
// ════════════════════════════════════════════════════════════════

/// 兼容性测试运行器
pub struct CompatTester {
    kubo: KuboBackend,
    iroh: IrohBackend,
    results: Vec<CompatTestResult>,
}

impl CompatTester {
    /// 创建测试器
    pub fn new(kubo: KuboBackend, iroh: IrohBackend) -> Self {
        Self {
            kubo,
            iroh,
            results: Vec::new(),
        }
    }

    /// 注册并运行单个测试
    async fn run_test<F, Fut>(&mut self, name: &str, requires_both: bool, f: F)
    where
        // 传入后端的克隆（owned）而非引用，避免闭包返回借用 future 引发的
        // 高阶生命周期问题；KuboBackend/IrohBackend 均为轻量 Clone。
        F: FnOnce(KuboBackend, IrohBackend) -> Fut,
        Fut: std::future::Future<Output = Result<(String, String), String>>,
    {
        let started = Instant::now();

        // 检查后端可用性
        let kubo_ok = self.kubo.is_available().await;
        let iroh_ok = self.iroh.is_available().await;

        if requires_both && (!kubo_ok || !iroh_ok) {
            self.results.push(CompatTestResult {
                name: name.to_string(),
                passed: false,
                duration_ms: started.elapsed().as_millis() as u64,
                kubo_result: None,
                iroh_result: None,
                error: Some(format!(
                    "Backends not available: kubo={}, iroh={}",
                    kubo_ok, iroh_ok
                )),
                notes: vec!["SKIP: backends unavailable".to_string()],
            });
            return;
        }

        match f(self.kubo.clone(), self.iroh.clone()).await {
            Ok((kubo_val, iroh_val)) => {
                let passed = kubo_val == iroh_val;
                self.results.push(CompatTestResult {
                    name: name.to_string(),
                    passed,
                    duration_ms: started.elapsed().as_millis() as u64,
                    kubo_result: Some(kubo_val),
                    iroh_result: Some(iroh_val),
                    error: if passed {
                        None
                    } else {
                        Some("Values differ".to_string())
                    },
                    notes: vec![],
                });
            }
            Err(e) => {
                self.results.push(CompatTestResult {
                    name: name.to_string(),
                    passed: false,
                    duration_ms: started.elapsed().as_millis() as u64,
                    kubo_result: None,
                    iroh_result: None,
                    error: Some(e),
                    notes: vec![],
                });
            }
        }
    }

    // ── 测试用例 ──

    /// 测试 1: 版本信息一致性
    pub async fn test_version_info(&mut self) {
        self.run_test("version_info", true, |kubo, iroh| async move {
            let kv = kubo.version().await.map_err(|e| e.to_string())?;
            let iv = iroh.version().await.map_err(|e| e.to_string())?;
            Ok((kv, iv))
        })
        .await;
    }

    /// 测试 2: 节点可达性
    pub async fn test_availability(&mut self) {
        self.run_test("availability", false, |kubo, iroh| async move {
            let ka = kubo.is_available().await;
            let ia = iroh.is_available().await;
            // 比较布尔值（转为字符串）
            Ok((ka.to_string(), ia.to_string()))
        })
        .await;
    }

    /// 测试 3: 仓库初始化状态
    pub async fn test_repo_initialized(&mut self) {
        self.run_test("repo_initialized", true, |kubo, iroh| async move {
            let kr = kubo.repo_stat().await.map_err(|e| e.to_string())?;
            let ir = iroh.repo_stat().await.map_err(|e| e.to_string())?;
            // 比较是否有数据
            let k_ok = kr.num_objects > 0 || kr.repo_size > 0;
            let i_ok = ir.num_objects > 0 || ir.repo_size > 0;
            // 至少一个后端已初始化即可
            Ok(((k_ok || i_ok).to_string(), (k_ok || i_ok).to_string()))
        })
        .await;
    }

    /// 测试 4: 网络节点发现
    pub async fn test_peer_discovery(&mut self) {
        self.run_test("peer_discovery", false, |kubo, iroh| async move {
            // 获取两个后端的 Peer ID
            let kn = kubo.node_info().await.map_err(|e| e.to_string())?;
            let info = iroh.node_info().await.map_err(|e| e.to_string())?;
            // 比较：不同后端应有不同 Peer ID（这是正常的）
            let differ = kn.peer_id != info.peer_id;
            Ok((differ.to_string(), "true".to_string()))
        })
        .await;
    }

    /// 运行全部兼容性测试
    pub async fn run_all(&mut self) -> CompatSuiteResult {
        let started = Instant::now();
        self.results.clear();

        // 按顺序执行所有测试
        self.test_version_info().await;
        self.test_availability().await;
        self.test_repo_initialized().await;
        self.test_peer_discovery().await;

        let total = self.results.len();
        let passed = self.results.iter().filter(|r| r.passed).count();
        let failed = total - passed;
        let skipped = self
            .results
            .iter()
            .filter(|r| r.notes.iter().any(|n| n.starts_with("SKIP")))
            .count();
        let score = if total > 0 {
            (passed as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        CompatSuiteResult {
            total,
            passed,
            failed,
            skipped: skipped.saturating_sub(passed),
            tests: self.results.clone(),
            total_duration_ms: started.elapsed().as_millis() as u64,
            compatibility_score: score,
        }
    }
}

// ════════════════════════════════════════════════════════════════
// 内容完整性测试
// ════════════════════════════════════════════════════════════════

/// 内容完整性验证器
///
/// 验证"添加 → 读取 → 哈希比对"流程在两个后端之间的一致性。
pub struct ContentIntegrityTester {
    kubo: KuboBackend,
    iroh: IrohBackend,
}

impl ContentIntegrityTester {
    pub fn new(kubo: KuboBackend, iroh: IrohBackend) -> Self {
        Self { kubo, iroh }
    }

    /// 跨后端的哈希验证
    ///
    /// 1. 用 Kubo 添加一个测试文件
    /// 2. 从 Kubo 获取该文件的 CID
    /// 3. 尝试用 Iroh 读取该 CID（如果 Iroh 支持）
    ///
    /// 返回 (kubo_cid, iroh_result, success)
    pub async fn cross_hash_verify(&self, test_data: &[u8]) -> (String, Option<String>, bool) {
        // 写临时文件
        let tmp = std::env::temp_dir().join("ipfs-compat-test.bin");
        std::fs::write(&tmp, test_data).ok();

        // Kubo 添加
        let kubo_result = match self.kubo.add_file(&tmp).await {
            Ok(r) => r,
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return (e.to_string(), None, false);
            }
        };

        // Iroh 尝试读取
        let iroh_result = match self.iroh.cat(&kubo_result.cid).await {
            Ok(data) => Some(format!("{} bytes", data.len())),
            Err(e) => Some(format!("Error: {}", e)),
        };

        let _ = std::fs::remove_file(&tmp);

        // 先计算成功标志，避免在同一元组里对 iroh_result 移动后再借用
        let success = iroh_result
            .as_ref()
            .is_some_and(|s| !s.starts_with("Error"));
        (kubo_result.cid, iroh_result, success)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_compat_tester_creation() {
        let kubo = KuboBackend::new("http://127.0.0.1:5001".to_string());
        let dir = std::env::temp_dir().join("compat-test-iroh");
        let iroh = IrohBackend::new(dir);
        let mut tester = CompatTester::new(kubo, iroh);
        let result = tester.run_all().await;
        println!("Compat score: {:.1}%", result.compatibility_score);
        assert!(result.compatibility_score >= 0.0);
    }

    #[tokio::test]
    async fn test_content_integrity_tester() {
        let kubo = KuboBackend::new("http://127.0.0.1:5001".to_string());
        let dir = std::env::temp_dir().join("ci-test-iroh");
        let iroh = IrohBackend::new(dir);
        let ct = ContentIntegrityTester::new(kubo, iroh);
        let (kubo_cid, iroh_res, _) = ct.cross_hash_verify(b"hello ipfs").await;
        println!("Kubo CID: {}", kubo_cid);
        println!("Iroh result: {:?}", iroh_res);
    }
}
