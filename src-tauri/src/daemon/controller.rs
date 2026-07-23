use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::types::DaemonStatus;

/// IPFS 守护进程控制器
/// 
/// 负责启动、停止和监控 Kubo 守护进程
#[derive(Clone)]
pub struct DaemonController {
    /// IPFS 二进制文件路径
    binary_path: Arc<PathBuf>,
    /// IPFS 仓库路径
    repo_path: Arc<PathBuf>,
    /// 守护进程子进程
    process: Arc<RwLock<Option<Child>>>,
    /// 守护进程状态
    status: Arc<RwLock<DaemonStatus>>,
}

impl DaemonController {
    /// 创建新的守护进程控制器
    pub fn new(binary_path: PathBuf, repo_path: PathBuf) -> Self {
        Self {
            binary_path: Arc::new(binary_path),
            repo_path: Arc::new(repo_path),
            process: Arc::new(RwLock::new(None)),
            status: Arc::new(RwLock::new(DaemonStatus::Stopped)),
        }
    }
    
    /// 启动守护进程
    /// 
    /// # Arguments
    /// 
    /// * `flags` - 启动参数（例如：["--migrate=true", "--enable-gc=true"]）
    /// 
    /// # Returns
    /// 
    /// 成功返回 Ok(())，失败返回错误信息
    pub async fn start(&self, flags: Vec<String>) -> Result<(), String> {
        // 检查当前状态
        let current_status = self.status.read().await;
        if !matches!(*current_status, DaemonStatus::Stopped | DaemonStatus::Failed { .. }) {
            return Err("Daemon is not in stopped state".to_string());
        }
        drop(current_status);
        
        // 更新状态为启动中
        *self.status.write().await = DaemonStatus::Starting;
        tracing::info!("Starting IPFS daemon...");
        tracing::info!("Binary: {:?}", self.binary_path);
        tracing::info!("Repo: {:?}", self.repo_path);
        tracing::info!("Flags: {:?}", flags);
        
        // 设置环境变量
        let mut cmd = Command::new(&self.binary_path);
        cmd.env("IPFS_PATH", &self.repo_path);
        
        // 添加 daemon 命令
        cmd.arg("daemon");
        
        // 添加启动参数
        for flag in flags {
            cmd.arg(flag);
        }
        
        // 配置标准输入输出
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        
        // 启动进程
        match cmd.spawn() {
            Ok(child) => {
                tracing::info!("IPFS daemon process started with PID: {:?}", child.id());
                
                // 保存进程句柄
                *self.process.write().await = Some(child);
                
                // 等待一小段时间，检查进程是否成功启动
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                
                // 检查进程是否还在运行
                if self.is_running().await {
                    *self.status.write().await = DaemonStatus::Running;
                    tracing::info!("IPFS daemon started successfully");
                    Ok(())
                } else {
                    let error = "Daemon process exited unexpectedly".to_string();
                    *self.status.write().await = DaemonStatus::Failed { 
                        error: error.clone() 
                    };
                    Err(error)
                }
            }
            Err(e) => {
                let error = format!("Failed to start daemon: {}", e);
                tracing::error!("{}", error);
                *self.status.write().await = DaemonStatus::Failed { error: error.clone() };
                Err(error)
            }
        }
    }
    
    /// 停止守护进程
    pub async fn stop(&self) -> Result<(), String> {
        // 检查当前状态
        let current_status = self.status.read().await;
        if matches!(*current_status, DaemonStatus::Stopped) {
            return Ok(());
        }
        drop(current_status);
        
        // 更新状态为停止中
        *self.status.write().await = DaemonStatus::Stopping;
        tracing::info!("Stopping IPFS daemon...");
        
        // 获取进程句柄
        let mut process_guard = self.process.write().await;
        
        if let Some(mut child) = process_guard.take() {
            // 尝试优雅地终止进程
            #[cfg(unix)]
            {
                // 在 Unix 系统上发送 SIGTERM
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                
                if let Some(pid) = child.id() {
                    let pid = Pid::from_raw(pid as i32);
                    match kill(pid, Signal::SIGTERM) {
                        Ok(_) => {
                            tracing::info!("Sent SIGTERM to daemon process");
                            
                            // 等待进程退出（最多 5 秒）
                            for _ in 0..10 {
                                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                match child.try_wait() {
                                    Ok(Some(status)) => {
                                        tracing::info!("Daemon process exited with status: {:?}", status);
                                        *self.status.write().await = DaemonStatus::Stopped;
                                        return Ok(());
                                    }
                                    Ok(None) => {
                                        // 进程还在运行
                                        continue;
                                    }
                                    Err(e) => {
                                        tracing::warn!("Error checking process status: {}", e);
                                        break;
                                    }
                                }
                            }
                            
                            // 如果超时，强制杀死进程
                            tracing::warn!("Daemon did not stop gracefully, sending SIGKILL");
                            let _ = kill(pid, Signal::SIGKILL);
                        }
                        Err(e) => {
                            tracing::error!("Failed to send SIGTERM: {}", e);
                        }
                    }
                }
            }
            
            #[cfg(windows)]
            {
                // 在 Windows 上直接 kill
                match child.kill() {
                    Ok(_) => {
                        tracing::info!("Killed daemon process");
                    }
                    Err(e) => {
                        tracing::error!("Failed to kill daemon: {}", e);
                    }
                }
            }
            
            // 等待进程完全退出
            match child.wait() {
                Ok(status) => {
                    tracing::info!("Daemon process exited: {:?}", status);
                }
                Err(e) => {
                    tracing::warn!("Error waiting for process: {}", e);
                }
            }
        }
        
        *self.status.write().await = DaemonStatus::Stopped;
        tracing::info!("IPFS daemon stopped");
        Ok(())
    }
    
    /// 重启守护进程
    pub async fn restart(&self, flags: Vec<String>) -> Result<(), String> {
        tracing::info!("Restarting IPFS daemon...");
        
        // 先停止
        self.stop().await?;
        
        // 等待一小段时间
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        
        // 再启动
        self.start(flags).await
    }
    
    /// 检查守护进程是否正在运行
    pub async fn is_running(&self) -> bool {
        let process_guard = self.process.read().await;
        
        if let Some(child) = process_guard.as_ref() {
            // 检查进程是否还存在
            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                
                if let Some(pid) = child.id() {
                    let pid = Pid::from_raw(pid as i32);
                    // 发送信号 0 来检查进程是否存在
                    return kill(pid, Signal::SIGHUP).is_ok() || 
                           kill(pid, None).is_ok();
                }
            }
            
            #[cfg(windows)]
            {
                // Windows 上通过 try_wait 检查
                // 注意：这需要可变引用，所以这里的检查不完美
                return child.id().is_some();
            }
        }
        
        false
    }
    
    /// 获取当前状态
    pub async fn get_status(&self) -> DaemonStatus {
        self.status.read().await.clone()
    }
    
    /// 设置状态
    pub async fn set_status(&self, status: DaemonStatus) {
        *self.status.write().await = status;
    }
    
    /// 获取进程 ID
    pub async fn get_pid(&self) -> Option<u32> {
        let process_guard = self.process.read().await;
        process_guard.as_ref().and_then(|child| child.id())
    }
}

impl Drop for DaemonController {
    fn drop(&mut self) {
        // 确保在控制器销毁时停止守护进程
        tracing::info!("DaemonController dropped, ensuring daemon is stopped");
        
        // 注意：这里不能使用 async，所以使用阻塞的方式
        if let Some(mut child) = self.process.blocking_write().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_controller_lifecycle() {
        // 这个测试需要实际的 IPFS 二进制
        // 在 CI 环境中可能会跳过
    }
}
