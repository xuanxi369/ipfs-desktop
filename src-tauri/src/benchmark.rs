//! 性能基准测试框架 — Phase 4
//!
//! 提供 Kubo vs Iroh 的规范化性能对比。
//!
//! 基准指标：
//! - 操作延迟（单次 API 调用）
//! - 吞吐量（批量操作）
//! - 内存占用（后台监控）
//! - 冷启动时间
//!
//! 运行方式：
//! ```bash
//! cargo test --test benchmark -- --ignored --nocapture
//! ```

use crate::backend_trait::{Backend, BackendType};
use crate::kubo_adapter::KuboBackend;
use crate::iroh_adapter::IrohBackend;
use serde::{Deserialize, Serialize};
use std::time::Instant;

// ════════════════════════════════════════════════════════════════
// 基准结果类型
// ════════════════════════════════════════════════════════════════

/// 单个操作基准结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchOpResult {
    /// 操作名称
    pub operation: String,
    /// 后端类型
    pub backend: String,
    /// 迭代次数
    pub iterations: u32,
    /// 最小延迟（毫秒）
    pub min_ms: f64,
    /// 最大延迟（毫秒）
    pub max_ms: f64,
    /// 平均延迟（毫秒）
    pub avg_ms: f64,
    /// 中位数延迟（毫秒）
    pub median_ms: f64,
    /// P99 延迟（毫秒）
    pub p99_ms: f64,
    /// 总吞吐量（ops/sec）
    pub throughput_ops: f64,
}

/// 完整基准套件结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchSuiteResult {
    /// 测试时间戳
    pub timestamp: String,
    /// 各操作结果
    pub operations: Vec<BenchOpResult>,
    /// 总耗时（毫秒）
    pub total_duration_ms: u64,
    /// 胜出后端（整体更快）
    pub winner: Option<String>,
    /// Kubo 平均延迟 / Iroh 平均延迟
    pub speedup_ratio: Option<f64>,
}

// ════════════════════════════════════════════════════════════════
// 基准执行器
// ════════════════════════════════════════════════════════════════

/// 微基准执行器
pub struct MicroBenchmark {
    kubo: KuboBackend,
    iroh: IrohBackend,
    /// 预热迭代数
    warmup: u32,
    /// 正式迭代数
    iterations: u32,
}

impl MicroBenchmark {
    pub fn new(kubo: KuboBackend, iroh: IrohBackend) -> Self {
        Self {
            kubo,
            iroh,
            warmup: 3,
            iterations: 10,
        }
    }

    /// 对单个后端执行基准测试
    async fn bench_op<F, Fut>(
        &self,
        name: &str,
        backend_type: BackendType,
        f: F,
    ) -> BenchOpResult
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output -> Result<(), String>>,
    {
        let mut latencies = Vec::with_capacity(self.iterations as usize);

        // 预热
        for _ in 0..self.warmup {
            let _ = f().await;
        }

        // 正式测量
        for _ in 0..self.iterations {
            let started = Instant::now();
            match f().await {
                Ok(()) => {
                    latencies.push(started.elapsed().as_secs_f64() * 1000.0);
                }
                Err(e) => {
                    tracing::warn!("Bench op '{}' failed: {}", name, e);
                    // 失败时记录一个很大的值
                    latencies.push(9999.0);
                }
            }
        }

        // 统计
        latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = latencies.len() as f64;
        let sum: f64 = latencies.iter().sum();
        let avg = sum / n;
        let min = latencies.first().copied().unwrap_or(0.0);
        let max = latencies.last().copied().unwrap_or(0.0);
        let median = if n > 0.0 {
            latencies[latencies.len() / 2]
        } else { 0.0 };
        let p99_idx = ((n * 0.99) as usize).min(latencies.len().saturating_sub(1));
        let p99 = latencies.get(p99_idx).copied().unwrap_or(max);

        let total_time = sum;
        let throughput = if total_time > 0.0 {
            (self.iterations as f64 / (total_time / 1000.0))
        } else {
            0.0
        };

        BenchOpResult {
            operation: name.to_string(),
            backend: backend_type.to_string(),
            iterations: self.iterations,
            min_ms: min,
            max_ms: max,
            avg_ms: avg,
            median_ms: median,
            p99_ms: p99,
            throughput_ops: throughput,
        }
    }

    /// 对两个后端执行同一个操作
    async fn bench_both<Fk, Fi, FutK, FutI>(
        &mut self,
        name: &str,
        f_kubo: Fk,
        f_iroh: Fi,
    ) -> Vec<BenchOpResult>
    where
        Fk: Fn() -> FutK,
        Fi: Fn() -> FutI,
        FutK: std::future::Future<Output -> Result<(), String>>,
        FutI: std::future::Future<Output -> Result<(), String>>,
    {
        let kubos = self.bench_op(name, BackendType::Kubo, f_kubo);
        let irohs = self.bench_op(name, BackendType::Iroh, f_iroh);
        vec![kubos.await, irohs.await]
    }

    // ── 具体基准测试 ──

    /// 基准: node_info 延迟
    pub async fn bench_node_info(&mut self) -> Vec<BenchOpResult> {
        let kubo = &self.kubo;
        let iroh = &self.iroh;
        self.bench_both(
            "node_info",
            || async move { kubo.node_info().await.map(|_| ()).map_err(|e| e.to_string()) },
            || async move { iroh.node_info().await.map(|_| ()).map_err(|e| e.to_string()) },
        ).await
    }

    /// 基准: repo_stat 延迟
    pub async fn bench_repo_stat(&mut self) -> Vec<BenchOpResult> {
        let kubo = &self.kubo;
        let iroh = &self.iroh;
        self.bench_both(
            "repo_stat",
            || async move { kubo.repo_stat().await.map(|_| ()).map_err(|e| e.to_string()) },
            || async move { iroh.repo_stat().await.map(|_| ()).map_err(|e| e.to_string()) },
        ).await
    }

    /// 基准: swarm_peers 延迟
    pub async fn bench_swarm_peers(&mut self) -> Vec<BenchOpResult> {
        let kubo = &self.kubo;
        let iroh = &self.iroh;
        self.bench_both(
            "swarm_peers",
            || async move { kubo.swarm_peers().await.map(|_| ()).map_err(|e| e.to_string()) },
            || async move { iroh.swarm_peers().await.map(|_| ()).map_err(|e| e.to_string()) },
        ).await
    }

    /// 运行全部基准测试
    pub async fn run_all(&mut self) -> BenchSuiteResult {
        let started = Instant::now();
        let mut all_results = Vec::new();

        // 按顺序执行基准
        all_results.extend(self.bench_node_info().await);
        all_results.extend(self.bench_repo_stat().await);
        all_results.extend(self.bench_swarm_peers().await);

        // 计算总延迟和胜出者
        let kubo_avg: f64 = all_results.iter()
            .filter(|r| r.backend == "Kubo (Go)")
            .map(|r| r.avg_ms)
            .sum();
        let iroh_avg: f64 = all_results.iter()
            .filter(|r| r.backend == "Iroh (Rust)")
            .map(|r| r.avg_ms)
            .sum();

        let speedup = if iroh_avg > 0.0 { Some(kubo_avg / iroh_avg) } else { None };
        let winner = if kubo_avg < iroh_avg {
            Some("Kubo".to_string())
        } else if iroh_avg > 0.0 {
            Some("Iroh".to_string())
        } else {
            None
        };

        BenchSuiteResult {
            timestamp: chrono::Local::now().to_rfc3339(),
            operations: all_results,
            total_duration_ms: started.elapsed().as_millis() as u64,
            winner,
            speedup_ratio: speedup,
        }
    }
}

// ════════════════════════════════════════════════════════════════
// 吞吐量测试
// ════════════════════════════════════════════════════════════════

/// 吞吐量测试器
///
/// 测试批量文件添加的吞吐量（MB/s）。
pub struct ThroughputBenchmark {
    kubo: KuboBackend,
    iroh: IrohBackend,
}

impl ThroughputBenchmark {
    pub fn new(kubo: KuboBackend, iroh: IrohBackend) -> Self {
        Self { kubo, iroh }
    }

    /// 测试文件添加吞吐量
    ///
    /// 生成指定大小和数量的随机文件，测量两个后端的添加吞吐量。
    pub async fn bench_add_throughput(
        &self,
        file_size: usize,
        file_count: u32,
    ) -> Result<(f64, Option<f64>), String> {
        // 生成测试文件
        let data: Vec<u8> = (0..file_size).map(|i| (i % 256) as u8).collect();
        let tmp = std::env::temp_dir().join("bench-throughput.bin");
        std::fs::write(&tmp, &data).map_err(|e| e.to_string())?;

        // Kubo 吞吐量测试
        let k_started = Instant::now();
        let mut k_success = 0u32;
        for _ in 0..file_count {
            if self.kubo.add_file(&tmp).await.is_ok() {
                k_success += 1;
            }
        }
        let k_elapsed = k_started.elapsed().as_secs_f64();
        let k_throughput = if k_elapsed > 0.0 {
            (file_size * k_success as usize) as f64 / k_elapsed / 1_000_000.0
        } else { 0.0 };

        // Iroh 吞吐量（stub 目前不支持 add_file）
        let i_throughput = match self.iroh.add_file(&tmp).await {
            Ok(_) => {
                let i_started = Instant::now();
                let mut i_success = 0u32;
                for _ in 0..file_count {
                    if self.iroh.add_file(&tmp).await.is_ok() {
                        i_success += 1;
                    }
                }
                let i_elapsed = i_started.elapsed().as_secs_f64();
                Some(if i_elapsed > 0.0 {
                    (file_size * i_success as usize) as f64 / i_elapsed / 1_000_000.0
                } else { 0.0 })
            }
            Err(_) => None,
        };

        let _ = std::fs::remove_file(&tmp);
        Ok((k_throughput, i_throughput))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_micro_benchmark_creation() {
        let kubo = KuboBackend::new("http://127.0.0.1:5001".to_string());
        let dir = std::env::temp_dir().join("bench-iroh");
        let iroh = IrohBackend::new(dir);
        let mut bench = MicroBenchmark::new(kubo, iroh);
        let result = bench.run_all().await;
        assert!(!result.operations.is_empty());
        println!("Benchmark complete: {} ops, winner: {:?}",
            result.operations.len(), result.winner);
    }

    #[tokio::test]
    async fn test_throughput_benchmark() {
        let kubo = KuboBackend::new("http://127.0.0.1:5001".to_string());
        let dir = std::env::temp_dir().join("tp-iroh");
        let iroh = IrohBackend::new(dir);
        let tb = ThroughputBenchmark::new(kubo, iroh);

        if tb.kubo.is_available().await {
            let (k_tp, i_tp) = tb.bench_add_throughput(1024, 3).await.unwrap();
            println!("Kubo throughput: {:.2} MB/s", k_tp);
            println!("Iroh throughput: {:?} MB/s", i_tp);
        } else {
            println!("SKIP: Kubo not running");
        }
    }

    #[test]
    fn test_bench_result_serialization() {
        let result = BenchOpResult {
            operation: "test".into(), backend: "Kubo (Go)".into(),
            iterations: 10, min_ms: 1.0, max_ms: 5.0, avg_ms: 2.5,
            median_ms: 2.0, p99_ms: 4.5, throughput_ops: 400.0,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: BenchOpResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.avg_ms, 2.5);
    }
}
