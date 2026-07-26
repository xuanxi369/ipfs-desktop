//! IPFS Desktop Rust — 集成测试
//!
//! 测试守护进程的完整生命周期：
//!   启动 → API 连通性验证 → 停止 → 确认进程消失
//!
//! 这些测试依赖系统中安装了 Kubo (go-ipfs)。
//! 如果未安装则自动跳过。

use ipfs_desktop_rust_lib::daemon::{BinaryFinder, DaemonController, IpfsApiClient};
use std::path::PathBuf;

/// 辅助：获取一个临时仓库路径（避免污染用户真实的 ~/.ipfs）
fn temp_repo_path(suffix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ipfs-desktop-rust-test-repo-{}", suffix))
}

/// 辅助：检查 IPFS 是否可用
fn ipfs_available() -> Option<PathBuf> {
    BinaryFinder::find()
}

// ── 测试 1: 二进制查找 ──────────────────────────────────────────

#[tokio::test]
async fn test_find_binary() {
    let binary = ipfs_available();
    if binary.is_none() {
        eprintln!("SKIP: IPFS binary not found on this system");
        return;
    }

    let path = binary.unwrap();
    println!("Found IPFS binary at: {:?}", path);
    assert!(path.exists(), "Binary should exist on disk");

    // 验证版本信息可获取
    let version = BinaryFinder::get_version(&path);
    assert!(version.is_ok(), "Should be able to get version: {:?}", version.err());
    println!("IPFS version: {}", version.unwrap());
}

// ── 测试 2: 守护进程完整生命周期 ───────────────────────────────

#[tokio::test]
async fn test_daemon_lifecycle() {
    let binary = match ipfs_available() {
        Some(p) => p,
        None => {
            eprintln!("SKIP: IPFS binary not found on this system");
            return;
        }
    };

    let repo = temp_repo_path("lifecycle");
    let api_addr = "http://127.0.0.1:5001".to_string();

    println!("Binary: {:?}", binary);
    println!("Repo: {:?}", repo);

    // 如果仓库不存在则初始化
    if !repo.join("config").exists() {
        println!("Initializing IPFS repo at {:?}...", repo);
        let output = std::process::Command::new(&binary)
            .env("IPFS_PATH", &repo)
            .arg("init")
            .output()
            .expect("Failed to run ipfs init");
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!("ipfs init failed: {}", stderr);
        }
        println!("Repo initialized");
    }

    // ── 阶段 1: 启动守护进程 ──
    println!("--- Phase 1: Starting daemon ---");
    let controller = DaemonController::new(binary.clone(), repo.clone());
    
    let flags = vec![
        "--offline".to_string(), // 离线模式，避免连接公网
    ];

    controller.start(flags)
        .await
        .expect("Daemon should start successfully");

    assert!(controller.is_running().await, "Daemon should be running after start");
    println!("Daemon started with PID: {:?}", controller.get_pid().await);

    // 给守护进程一点时间初始化 HTTP API
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // ── 阶段 2: API 连通性验证 ──
    println!("--- Phase 2: API connectivity check ---");
    let api_client = IpfsApiClient::new(api_addr.clone());
    
    // 2a. 检查可达性
    let reachable = api_client.is_reachable().await;
    assert!(reachable, "IPFS API should be reachable after daemon start");

    // 2b. 获取节点 ID
    let node_id = api_client.id().await.expect("Should get node ID");
    println!("Node ID: {}", node_id.id);
    assert!(!node_id.id.is_empty(), "Node ID should not be empty");

    // 2c. 获取版本
    let version_info = api_client.version().await.expect("Should get version");
    println!("Version: {}", version_info.version);
    assert!(!version_info.version.is_empty(), "Version should not be empty");

    // 2d. 获取仓库统计
    let repo_stat = api_client.repo_stat().await.expect("Should get repo stats");
    println!("Repo size: {} bytes, {} objects", repo_stat.repo_size, repo_stat.num_objects);

    // ── 阶段 3: 停止守护进程 ──
    println!("--- Phase 3: Stopping daemon ---");
    controller.stop().await.expect("Daemon should stop successfully");
    println!("Daemon stopped");

    // 等待进程完全退出
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // ── 阶段 4: 确认进程已消失 ──
    println!("--- Phase 4: Verify process is gone ---");
    assert!(!controller.is_running().await, "Daemon should not be running after stop");

    // API 应该不可达
    let reachable_after_stop = api_client.is_reachable().await;
    assert!(!reachable_after_stop, "API should NOT be reachable after daemon stop");
    println!("Process confirmed gone, API unreachable");

    println!("=== All lifecycle phases passed! ===");
}

// ── 测试 3: 重启守护进程 ────────────────────────────────────────

#[tokio::test]
async fn test_daemon_restart() {
    let binary = match ipfs_available() {
        Some(p) => p,
        None => {
            eprintln!("SKIP: IPFS binary not found on this system");
            return;
        }
    };

    let repo = temp_repo_path("restart");
    
    // 确保仓库存在
    if !repo.join("config").exists() {
        let output = std::process::Command::new(&binary)
            .env("IPFS_PATH", &repo)
            .arg("init")
            .output()
            .expect("Failed to run ipfs init");
        assert!(output.status.success(), "ipfs init should succeed");
    }

    let controller = DaemonController::new(binary, repo);
    let flags = vec!["--offline".to_string()];

    // 启动 → 验证 → 重启 → 验证
    controller.start(flags.clone()).await.expect("First start should succeed");
    assert!(controller.is_running().await);
    let pid_before = controller.get_pid().await;
    println!("First start PID: {:?}", pid_before);

    controller.restart(flags).await.expect("Restart should succeed");
    assert!(controller.is_running().await, "Daemon should be running after restart");
    let pid_after = controller.get_pid().await;
    println!("After restart PID: {:?}", pid_after);

    // 进程 ID 应该变了（新进程）
    if pid_before.is_some() && pid_after.is_some() {
        assert_ne!(pid_before, pid_after, "PID should change after restart");
    }

    // 清理
    controller.stop().await.expect("Final stop should succeed");
    assert!(!controller.is_running().await);
    println!("Restart test passed!");
}

// ── 测试 4: API 客户端错误处理 ──────────────────────────────────

#[tokio::test]
async fn test_api_client_connection_error() {
    // 使用一个不可能在用的地址
    let client = IpfsApiClient::new("http://127.0.0.1:59999".to_string());
    
    let reachable = client.is_reachable().await;
    assert!(!reachable, "Should not be reachable on unused port");

    let result = client.id().await;
    assert!(result.is_err(), "Should return error when daemon is not running");
    println!("Error (expected): {}", result.unwrap_err());
}
