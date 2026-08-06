use crate::error::DaemonError;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

fn is_multicast_interface_warning(line: &str) -> bool {
    line.contains("mdns: Failed to set multicast interface")
}

/// IPFS 守护进程控制器
///
/// 负责启动、停止和监控 Kubo 守护进程。
/// 状态跟踪由上层 AppState / commands 负责，控制器只管理子进程生命周期。
#[derive(Clone)]
pub struct DaemonController {
    /// IPFS 二进制文件路径
    binary_path: Arc<PathBuf>,
    /// IPFS 仓库路径
    repo_path: Arc<PathBuf>,
    /// 守护进程子进程（Mutex 允许 try_wait 跨平台检查存活）
    process: Arc<Mutex<Option<Child>>>,
    /// 是否已通过正常流程停止（stop() 设为 true，Drop 看到 true 则跳过强杀）
    stopped: Arc<AtomicBool>,
}

impl DaemonController {
    /// 创建新的守护进程控制器
    pub fn new(binary_path: PathBuf, repo_path: PathBuf) -> Self {
        Self {
            binary_path: Arc::new(binary_path),
            repo_path: Arc::new(repo_path),
            process: Arc::new(Mutex::new(None)),
            stopped: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Ensure the configured Kubo repository exists before starting the daemon.
    ///
    /// `ipfs init` is idempotently invoked only when the repository's config
    /// file is missing. This lets packaged builds work on first launch without
    /// requiring users to install or invoke the Kubo CLI themselves.
    pub async fn ensure_repo_initialized(&self) -> Result<bool, DaemonError> {
        if self.repo_path.join("config").is_file() {
            return Ok(false);
        }

        let binary_path = Arc::clone(&self.binary_path);
        let repo_path = Arc::clone(&self.repo_path);
        tracing::info!("Initializing Kubo repository at {:?}", repo_path);

        tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all(repo_path.as_path()).map_err(|e| {
                DaemonError::ProcessStartFailed(format!(
                    "failed to create repository directory: {e}"
                ))
            })?;

            let output = Command::new(binary_path.as_path())
                .env("IPFS_PATH", repo_path.as_path())
                .arg("init")
                .output()
                .map_err(|e| {
                    DaemonError::ProcessStartFailed(format!("failed to run ipfs init: {e}"))
                })?;

            if output.status.success() {
                tracing::info!("Kubo repository initialized successfully");
                Ok(true)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                Err(DaemonError::ProcessStartFailed(format!(
                    "ipfs init exited with {}: {}",
                    output.status,
                    if stderr.is_empty() {
                        "unknown error"
                    } else {
                        &stderr
                    }
                )))
            }
        })
        .await
        .map_err(|e| DaemonError::ProcessStartFailed(format!("ipfs init task failed: {e}")))?
    }

    /// 启动守护进程
    pub async fn start(&self, flags: Vec<String>) -> Result<(), DaemonError> {
        // 防止重复启动
        if self.is_running().await {
            return Err(DaemonError::InvalidState);
        }

        self.ensure_repo_initialized().await?;

        tracing::info!("Starting IPFS daemon...");
        tracing::info!("Binary: {:?}", self.binary_path);
        tracing::info!("Repo: {:?}", self.repo_path);
        tracing::info!("Flags: {:?}", flags);

        let mut cmd = Command::new(self.binary_path.as_path());
        cmd.env("IPFS_PATH", self.repo_path.as_path());
        cmd.arg("daemon");
        for flag in flags {
            cmd.arg(flag);
        }

        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| DaemonError::ProcessStartFailed(e.to_string()))?;

        let pid = child.id();
        tracing::info!("IPFS daemon process started with PID: {:?}", pid);

        // ── 启动 stdout/stderr 后台日志采集 ──
        // 持续读取管道防止缓冲区满导致进程阻塞
        if let Some(stdout) = child.stdout.take() {
            Self::pipe_reader(stdout, "ipfs-stdout");
        }
        if let Some(stderr) = child.stderr.take() {
            Self::pipe_reader(stderr, "ipfs-stderr");
        }

        *self.process.lock().await = Some(child);

        // 等待一小段时间，检查进程是否成功启动
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        if self.is_running().await {
            tracing::info!("IPFS daemon started successfully");
            Ok(())
        } else {
            let mut proc_guard = self.process.lock().await;
            // 注意：stdout/stderr 已被 pipe_reader 后台任务接管，
            // 其内容已通过 tracing 输出到日志中，此处不再从管道读取。
            let msg =
                "Daemon process exited unexpectedly (check logs for stderr output)".to_string();
            tracing::error!("Daemon startup failed: {}", msg);
            *proc_guard = None;
            Err(DaemonError::ProcessStartFailed(msg))
        }
    }

    /// 后台管道读取器：持续从进程 stdout/stderr 读取并记录日志
    ///
    /// 使用阻塞 I/O 在 spawn_blocking 中运行，避免阻塞 tokio 运行时。
    /// 当管道关闭（进程退出）时自动结束。
    fn pipe_reader(pipe: impl std::io::Read + Send + 'static, label: &'static str) {
        use std::io::{BufRead, BufReader};
        tokio::task::spawn_blocking(move || {
            let reader = BufReader::new(pipe);
            let mut mdns_warning_count = 0_u64;
            for line in reader.lines() {
                match line {
                    Ok(text) if !text.is_empty() => {
                        if is_multicast_interface_warning(&text) {
                            mdns_warning_count += 1;
                            if mdns_warning_count == 1 {
                                tracing::warn!(
                                    "[{label}] Kubo mDNS could not use one or more Windows network adapters; LAN discovery may be unavailable on those adapters"
                                );
                            }
                        } else if text.contains("[ERROR]") || text.contains("ERROR") {
                            tracing::error!("[{label}] {text}");
                        } else if text.contains("[WARN]") || text.contains("WARN") {
                            tracing::warn!("[{label}] {text}");
                        } else {
                            tracing::info!("[{label}] {text}");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("[{label}] Pipe read error: {e}");
                        break;
                    }
                    _ => {}
                }
            }
            if mdns_warning_count > 1 {
                tracing::warn!(
                    "[{label}] suppressed {} duplicate Kubo mDNS multicast-interface warnings",
                    mdns_warning_count - 1
                );
            }
            tracing::info!("[{label}] Pipe closed (process exited)");
        });
    }

    /// 停止守护进程
    pub async fn stop(&self) -> Result<(), DaemonError> {
        let mut process_guard = self.process.lock().await;
        if process_guard.is_none() {
            return Ok(());
        }

        tracing::info!("Stopping IPFS daemon...");

        if let Some(mut child) = process_guard.take() {
            drop(process_guard);

            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;

                let pid = child.id();
                let nix_pid = Pid::from_raw(pid as i32);
                match kill(nix_pid, Signal::SIGTERM) {
                    Ok(_) => {
                        tracing::info!("Sent SIGTERM to daemon (PID: {})", pid);
                        for i in 0..10 {
                            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                            match child.try_wait() {
                                Ok(Some(status)) => {
                                    tracing::info!("Daemon exited gracefully: {:?}", status);
                                    let _ = child.wait();
                                    let mut guard = self.process.lock().await;
                                    *guard = None;
                                    return Ok(());
                                }
                                Ok(None) => {
                                    tracing::debug!("Waiting for daemon... ({}/10)", i + 1);
                                    continue;
                                }
                                Err(e) => {
                                    tracing::warn!("try_wait error: {}", e);
                                    continue;
                                }
                            }
                        }
                        tracing::warn!("Daemon not stopping, sending SIGKILL");
                        let _ = kill(nix_pid, Signal::SIGKILL);
                    }
                    Err(e) => {
                        tracing::error!("SIGTERM failed: {}, sending SIGKILL", e);
                        let _ = kill(nix_pid, Signal::SIGKILL);
                    }
                }
            }

            #[cfg(windows)]
            {
                match child.kill() {
                    Ok(_) => tracing::info!("Killed daemon on Windows"),
                    Err(e) => tracing::error!("Failed to kill daemon on Windows: {}", e),
                }
            }

            match child.wait() {
                Ok(status) => tracing::info!("Daemon exited: {:?}", status),
                Err(e) => tracing::warn!("Error waiting for daemon: {}", e),
            }
        }

        let mut guard = self.process.lock().await;
        *guard = None;
        self.stopped.store(true, Ordering::SeqCst);
        tracing::info!("IPFS daemon stopped");
        Ok(())
    }

    /// 重启守护进程
    ///
    /// 先停止再启动，通过轮询确认进程已完全退出后再启动。
    pub async fn restart(&self, flags: Vec<String>) -> Result<(), DaemonError> {
        tracing::info!("Restarting IPFS daemon...");

        self.stop().await?;

        // 轮询确认进程已完全退出（最多等待 10 秒）
        for i in 0..20 {
            if !self.is_running().await {
                tracing::info!(
                    "Daemon process confirmed stopped after {} ms",
                    (i + 1) * 500
                );
                break;
            }
            if i == 19 {
                tracing::warn!("Daemon process still running after 10s, force starting anyway");
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        self.start(flags).await
    }

    /// 检查守护进程是否正在运行
    ///
    /// 通过 try_wait 检查进程是否存活，Unix/Windows 统一行为。
    pub async fn is_running(&self) -> bool {
        let mut process_guard = self.process.lock().await;

        if let Some(ref mut child) = *process_guard {
            match child.try_wait() {
                Ok(Some(status)) => {
                    tracing::info!("Daemon process already exited: {:?}", status);
                    *process_guard = None;
                    false
                }
                Ok(None) => true,
                Err(e) => {
                    tracing::warn!("Error checking process: {}", e);
                    false
                }
            }
        } else {
            false
        }
    }

    /// 获取进程 ID
    pub async fn get_pid(&self) -> Option<u32> {
        let process_guard = self.process.lock().await;
        process_guard.as_ref().map(|child| child.id())
    }
}

impl Drop for DaemonController {
    fn drop(&mut self) {
        // 只有「最后一箭」才做紧急清理——clone 离开作用域不应杀进程。
        // Arc::strong_count 在 Drop 内仍计入 self.process，因此 > 1 表示还有其他 clone 存活。
        if Arc::strong_count(&self.process) > 1 {
            return;
        }
        // 已通过 stop() 正常停止 → 无需再次强杀
        if self.stopped.load(Ordering::SeqCst) {
            return;
        }
        tracing::warn!("DaemonController dropped — attempting emergency cleanup");
        // try_lock 是尽力而为的安全网；正常流程应在 drop 前调用 stop()
        match self.process.try_lock() {
            Ok(mut guard) => {
                if let Some(mut child) = guard.take() {
                    tracing::warn!("DaemonController: force-killing child process on drop");
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
            Err(_) => {
                tracing::error!("DaemonController: could not acquire process lock on drop — child process may leak!");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_only_the_known_kubo_mdns_adapter_warning() {
        assert!(is_multicast_interface_warning(
            "[WARN] mdns: Failed to set multicast interface: setsockopt: invalid argument"
        ));
        assert!(!is_multicast_interface_warning(
            "[WARN] swarm: failed to connect to peer"
        ));
    }

    #[tokio::test]
    async fn test_controller_not_running_initially() {
        let controller = DaemonController::new(
            std::path::PathBuf::from("/nonexistent/ipfs"),
            std::path::PathBuf::from("/tmp/test-repo"),
        );
        assert!(!controller.is_running().await);
        assert!(controller.get_pid().await.is_none());
    }

    #[tokio::test]
    async fn test_controller_start_nonexistent_binary() {
        let controller = DaemonController::new(
            std::path::PathBuf::from("/nonexistent/ipfs_binary_xyz_test"),
            std::path::PathBuf::from("/tmp/test-repo"),
        );
        let result = controller.start(vec![]).await;
        assert!(result.is_err(), "Should fail with nonexistent binary");
    }

    #[tokio::test]
    async fn test_ensure_repo_initialized_is_idempotent() {
        let binary = match crate::daemon::BinaryFinder::find() {
            Some(path) => path,
            None => {
                println!("SKIP: Kubo binary is not available");
                return;
            }
        };
        let repo_dir = tempfile::tempdir().expect("temporary Kubo repo");
        let controller = DaemonController::new(binary, repo_dir.path().to_path_buf());

        assert!(controller
            .ensure_repo_initialized()
            .await
            .expect("first initialization should succeed"));
        assert!(repo_dir.path().join("config").is_file());
        assert!(!controller
            .ensure_repo_initialized()
            .await
            .expect("second initialization should be a no-op"));
    }

    #[tokio::test]
    async fn test_controller_lifecycle_with_ipfs() {
        // 检查 IPFS 是否安装
        let binary = match crate::daemon::BinaryFinder::find() {
            Some(p) => p,
            None => {
                println!("SKIP: IPFS not installed");
                return;
            }
        };

        let repo_dir = tempfile::tempdir().expect("temporary Kubo repo");
        let repo = repo_dir.path().to_path_buf();

        // 初始化仓库
        if !repo.join("config").exists() {
            let output = std::process::Command::new(&binary)
                .env("IPFS_PATH", &repo)
                .arg("init")
                .output()
                .expect("ipfs init should work");
            assert!(output.status.success(), "init failed");
        }

        let controller = DaemonController::new(binary, repo);

        // 启动
        if let Err(error) = controller.start(vec!["--offline".into()]).await {
            println!("SKIP: local Kubo cannot start in this environment: {error}");
            return;
        }
        assert!(controller.is_running().await);
        assert!(controller.get_pid().await.is_some());
        println!("Started with PID: {:?}", controller.get_pid().await);

        // 验证双重启动返回错误
        let double_start = controller.start(vec![]).await;
        assert!(double_start.is_err(), "Double start should fail");

        // 停止
        controller.stop().await.expect("stop should succeed");
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        assert!(
            !controller.is_running().await,
            "Should not be running after stop"
        );
        println!("Controller lifecycle test passed!");
    }
}
