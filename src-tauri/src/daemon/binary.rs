use crate::daemon::kubo_hashes::KuboHashes;
use crate::error::DaemonError;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::Command;

/// Kubo 二进制文件查找器
pub struct BinaryFinder;

impl BinaryFinder {
    /// 查找 Kubo 二进制文件（不进行哈希校验）
    ///
    /// 查找顺序：
    /// 1. 环境变量 IPFS_GO_EXEC
    /// 2. 系统 PATH 中的 ipfs
    /// 3. 内置的 kubo 二进制（如果有）
    ///
    /// # Returns
    ///
    /// 返回找到的二进制文件路径，如果找不到则返回 None
    pub fn find() -> Option<PathBuf> {
        Self::find_with_expected_hash(None)
    }

    /// 查找 Kubo 二进制文件并进行哈希校验
    ///
    /// # Arguments
    ///
    /// * `expected_hash` - 期望的 SHA-256 哈希值（64位十六进制字符串）
    ///
    /// # Returns
    ///
    /// 如果找到且哈希匹配则返回路径，否则返回 None
    pub fn find_with_expected_hash(expected_hash: Option<String>) -> Option<PathBuf> {
        // 1. 检查环境变量
        if let Ok(custom_path) = std::env::var("IPFS_GO_EXEC") {
            let path = PathBuf::from(custom_path);
            if path.exists() && Self::verify_binary_with_hash(&path, expected_hash.as_deref()) {
                tracing::info!("Using IPFS binary from IPFS_GO_EXEC: {:?}", path);
                return Some(path);
            }
        }

        // 2. 检查系统 PATH
        if let Some(path) = Self::find_in_path(expected_hash.as_deref()) {
            tracing::info!("Using IPFS binary from PATH: {:?}", path);
            return Some(path);
        }

        // 3. 检查内置二进制（如果有）
        if let Some(path) = Self::find_bundled(expected_hash.as_deref()) {
            tracing::info!("Using bundled IPFS binary: {:?}", path);
            return Some(path);
        }

        tracing::error!("Could not find IPFS binary");
        None
    }

    /// 在系统 PATH 中查找 ipfs 命令
    fn find_in_path(expected_hash: Option<&str>) -> Option<PathBuf> {
        #[cfg(unix)]
        let which_cmd = "which";

        #[cfg(windows)]
        let which_cmd = "where";

        let output = Command::new(which_cmd).arg("ipfs").output().ok()?;

        if output.status.success() {
            let path_str = String::from_utf8(output.stdout).ok()?;
            let path = PathBuf::from(path_str.trim());

            if path.exists() && Self::verify_binary_with_hash(&path, expected_hash) {
                return Some(path);
            }
        }

        None
    }

    /// 查找内置的 Kubo 二进制
    fn find_bundled(expected_hash: Option<&str>) -> Option<PathBuf> {
        let app_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();

        #[cfg(unix)]
        let binary_name = "ipfs";

        #[cfg(windows)]
        let binary_name = "ipfs.exe";

        let candidates = vec![
            app_dir.join(binary_name),
            app_dir.join("bin").join(binary_name),
            app_dir.join("resources").join(binary_name),
        ];

        candidates.into_iter().find(|candidate| {
            candidate.exists() && Self::verify_binary_with_hash(candidate, expected_hash)
        })
    }

    /// 验证二进制文件是否是有效的 IPFS 可执行文件
    ///
    /// 执行三层验证：
    /// 1. 文件权限检查（Unix 系统）
    /// 2. SHA-256 哈希校验（如果提供了 expected_hash）
    /// 3. 行为验证（能否执行 `ipfs version` 命令）
    ///
    /// # Arguments
    ///
    /// * `path` - 二进制文件路径
    /// * `expected_hash` - 期望的 SHA-256 哈希值（可选）
    ///
    /// # Returns
    ///
    /// 如果二进制文件有效则返回 true
    fn verify_binary_with_hash(path: &PathBuf, expected_hash: Option<&str>) -> bool {
        // 1. 权限检查（Unix）
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(path) {
                let permissions = metadata.permissions();
                if permissions.mode() & 0o111 == 0 {
                    tracing::warn!("Binary {:?} is not executable", path);
                    return false;
                }
            }
        }

        // 2. 哈希校验（强制模式）
        if let Some(expected) = expected_hash {
            match Self::calculate_hash(path) {
                Ok(actual) => {
                    if !actual.eq_ignore_ascii_case(expected) {
                        tracing::error!(
                            "Kubo binary hash mismatch at {:?}: expected {}, got {}",
                            path,
                            expected,
                            actual
                        );
                        return false;
                    }
                    tracing::info!("Binary hash verified: {}", actual);
                }
                Err(e) => {
                    tracing::warn!("Failed to calculate binary hash: {}", e);
                    return false;
                }
            }
        }

        // 3. 行为验证（执行 version 命令）
        match Command::new(path).arg("version").output() {
            Ok(output) => {
                let version_str = String::from_utf8_lossy(&output.stdout);
                // 收紧匹配：Kubo 的 `ipfs version` 输出形如 "ipfs version 0.x.y"
                let is_valid =
                    output.status.success() && version_str.to_lowercase().contains("ipfs version");

                if is_valid {
                    tracing::info!("Verified IPFS binary: {}", version_str.trim());
                } else {
                    tracing::warn!("Binary {:?} failed verification", path);
                }

                is_valid
            }
            Err(e) => {
                tracing::warn!("Failed to verify binary {:?}: {}", path, e);
                false
            }
        }
    }

    /// 计算文件的 SHA-256 哈希值
    pub fn calculate_hash(path: &PathBuf) -> Result<String, std::io::Error> {
        let bytes = std::fs::read(path)?;
        let hash = Sha256::digest(bytes);
        Ok(format!("{:x}", hash))
    }

    /// 验证二进制文件是否匹配已知版本的哈希
    ///
    /// 这是一个可选的安全增强功能：
    /// - 获取二进制的版本信息
    /// - 查询已知版本的官方哈希值
    /// - 如果匹配则增强信任，不匹配则记录警告但仍允许使用
    pub fn verify_against_known_hashes(path: &PathBuf) -> Result<bool, String> {
        // 获取版本信息
        let version_output = Command::new(path)
            .arg("version")
            .output()
            .map_err(|e| format!("Failed to get version: {}", e))?;

        let version_str = String::from_utf8_lossy(&version_output.stdout).to_string();

        // 提取版本号
        let version = match KuboHashes::extract_version(&version_str) {
            Some(v) => v,
            None => {
                tracing::warn!("Could not extract version from: {}", version_str.trim());
                return Ok(false);
            }
        };

        // 查询已知哈希
        let db = KuboHashes::get();
        let platform = KuboHashes::get_current_platform();

        let expected_hash = match db.get_hash_for_version(&version, &platform) {
            Some(hash) => hash,
            None => {
                tracing::info!(
                    "No known hash for version {} on platform {}",
                    version,
                    platform
                );
                return Ok(false);
            }
        };

        // 计算实际哈希
        let actual_hash =
            Self::calculate_hash(path).map_err(|e| format!("Failed to calculate hash: {}", e))?;

        // 比较哈希
        let matches = actual_hash.eq_ignore_ascii_case(expected_hash);

        if matches {
            tracing::info!(
                "✓ Binary verified against known hash for Kubo {} ({})",
                version,
                platform
            );
        } else {
            tracing::warn!(
                "⚠ Binary hash does not match known hash for Kubo {} ({})",
                version,
                platform
            );
            tracing::warn!("  Expected: {}", expected_hash);
            tracing::warn!("  Actual:   {}", actual_hash);
            tracing::warn!("  This may indicate a modified or unofficial binary");
        }

        Ok(matches)
    }

    /// 获取 IPFS 版本信息
    pub fn get_version(binary_path: &PathBuf) -> Result<String, DaemonError> {
        let output = Command::new(binary_path)
            .arg("version")
            .output()
            .map_err(|e| DaemonError::BinaryVerificationFailed(e.to_string()))?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(version)
        } else {
            Err(DaemonError::BinaryVerificationFailed(
                "version command returned non-zero exit code".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_binary_does_not_panic() {
        // 无论是否找到二进制，都不应该 panic
        let result = BinaryFinder::find();
        println!("Found binary: {:?}", result);
    }

    #[test]
    fn test_find_binary_returns_some_if_installed() {
        let result = BinaryFinder::find();
        if let Some(path) = result {
            assert!(path.exists(), "Binary path should exist");
        }
    }

    #[test]
    fn test_calculate_hash() {
        if let Some(path) = BinaryFinder::find() {
            let hash = BinaryFinder::calculate_hash(&path);
            assert!(hash.is_ok(), "Should calculate hash successfully");
            let h = hash.unwrap();
            assert_eq!(h.len(), 64, "SHA-256 should be 64 hex chars");
            println!("Binary hash: {}", h);
        }
    }

    #[test]
    fn test_verify_against_known_hashes() {
        if let Some(path) = BinaryFinder::find() {
            let result = BinaryFinder::verify_against_known_hashes(&path);
            // 无论匹配与否都不应该失败
            assert!(
                result.is_ok(),
                "Verification should not fail: {:?}",
                result.err()
            );
            println!("Known hash verification result: {:?}", result.unwrap());
        }
    }

    #[test]
    fn test_get_version_on_found_binary() {
        if let Some(path) = BinaryFinder::find() {
            let version = BinaryFinder::get_version(&path);
            assert!(
                version.is_ok(),
                "get_version should succeed: {:?}",
                version.err()
            );
            let v = version.unwrap();
            assert!(!v.is_empty(), "Version string should not be empty");
            println!("IPFS version: {}", v);
        }
    }

    #[test]
    fn test_find_with_wrong_hash() {
        // 提供一个错误的哈希值，应该找不到二进制
        let fake_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        let result = BinaryFinder::find_with_expected_hash(Some(fake_hash.to_string()));
        assert!(result.is_none(), "Should not find binary with wrong hash");
    }
}
