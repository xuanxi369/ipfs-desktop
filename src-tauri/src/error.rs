//! 统一错误类型
//!
//! 使用 thiserror 定义应用级错误，实现 Serialize 以支持
//! 结构化错误传递到前端（替代裸 String）。

use serde::Serialize;
use thiserror::Error;

/// 守护进程相关错误
#[derive(Error, Debug, Serialize)]
pub enum DaemonError {
    /// 二进制文件未找到
    #[error("IPFS binary not found. Please install Kubo or set IPFS_GO_EXEC.")]
    BinaryNotFound,

    /// 二进制验证失败
    #[error("Binary verification failed: {0}")]
    BinaryVerificationFailed(String),

    /// 进程启动失败
    #[error("Failed to start daemon process: {0}")]
    ProcessStartFailed(String),

    /// 进程已意外退出（健康监控检测到）
    #[error("Daemon process exited unexpectedly")]
    ProcessExitedUnexpectedly,

    /// 进程停止失败
    #[error("Failed to stop daemon: {0}")]
    ProcessStopFailed(String),

    /// 守护进程状态不允许该操作
    #[error("Invalid daemon state for this operation")]
    InvalidState,

    /// API 请求失败
    #[error("IPFS API error: {0}")]
    ApiError(String),

    /// API 连接失败
    #[error("Failed to connect to IPFS API at {addr}: {detail}")]
    ApiConnectionFailed { addr: String, detail: String },

    /// API 响应解析失败
    #[error("Failed to parse API response: {0}")]
    ApiParseError(String),

    /// 配置错误
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// I/O 错误
    #[error("I/O error: {0}")]
    IoError(String),
}

impl From<std::io::Error> for DaemonError {
    fn from(e: std::io::Error) -> Self {
        DaemonError::IoError(e.to_string())
    }
}

/// 允许 DaemonError 转为 String（兼容旧代码）
impl From<DaemonError> for String {
    fn from(e: DaemonError) -> Self {
        e.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = DaemonError::BinaryNotFound;
        assert!(err.to_string().contains("IPFS binary not found"));
    }

    #[test]
    fn test_error_serialization() {
        let err = DaemonError::BinaryNotFound;
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("BinaryNotFound"));
    }

    #[test]
    fn test_error_serialization_with_data() {
        let err = DaemonError::ProcessStartFailed("test error".to_string());
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("ProcessStartFailed"));
        assert!(json.contains("test error"));
    }

    #[test]
    fn test_error_api_connection() {
        let err = DaemonError::ApiConnectionFailed {
            addr: "http://127.0.0.1:5001".to_string(),
            detail: "connection refused".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("127.0.0.1"));
        assert!(json.contains("connection refused"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let daemon_err: DaemonError = io_err.into();
        assert!(matches!(daemon_err, DaemonError::IoError(_)));
    }

    #[test]
    fn test_error_into_string() {
        let err = DaemonError::InvalidState;
        let s: String = err.into();
        assert_eq!(s, "Invalid daemon state for this operation");
    }
}
