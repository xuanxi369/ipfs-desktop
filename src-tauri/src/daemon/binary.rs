use std::path::PathBuf;
use std::process::Command;

/// Kubo 二进制文件查找器
pub struct BinaryFinder;

impl BinaryFinder {
    /// 查找 Kubo 二进制文件
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
        // 1. 检查环境变量
        if let Ok(custom_path) = std::env::var("IPFS_GO_EXEC") {
            let path = PathBuf::from(custom_path);
            if path.exists() && Self::verify_binary(&path) {
                tracing::info!("Using IPFS binary from IPFS_GO_EXEC: {:?}", path);
                return Some(path);
            }
        }
        
        // 2. 检查系统 PATH
        if let Some(path) = Self::find_in_path() {
            tracing::info!("Using IPFS binary from PATH: {:?}", path);
            return Some(path);
        }
        
        // 3. 检查内置二进制（如果有）
        if let Some(path) = Self::find_bundled() {
            tracing::info!("Using bundled IPFS binary: {:?}", path);
            return Some(path);
        }
        
        tracing::error!("Could not find IPFS binary");
        None
    }
    
    /// 在系统 PATH 中查找 ipfs 命令
    fn find_in_path() -> Option<PathBuf> {
        // 尝试运行 which/where 命令
        #[cfg(unix)]
        let which_cmd = "which";
        
        #[cfg(windows)]
        let which_cmd = "where";
        
        let output = Command::new(which_cmd)
            .arg("ipfs")
            .output()
            .ok()?;
        
        if output.status.success() {
            let path_str = String::from_utf8(output.stdout).ok()?;
            let path = PathBuf::from(path_str.trim());
            
            if path.exists() && Self::verify_binary(&path) {
                return Some(path);
            }
        }
        
        None
    }
    
    /// 查找内置的 Kubo 二进制
    fn find_bundled() -> Option<PathBuf> {
        // 在应用程序目录中查找
        let app_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
        
        #[cfg(unix)]
        let binary_name = "ipfs";
        
        #[cfg(windows)]
        let binary_name = "ipfs.exe";
        
        // 检查几个可能的位置
        let candidates = vec![
            app_dir.join(binary_name),
            app_dir.join("bin").join(binary_name),
            app_dir.join("resources").join(binary_name),
        ];
        
        for candidate in candidates {
            if candidate.exists() && Self::verify_binary(&candidate) {
                return Some(candidate);
            }
        }
        
        None
    }
    
    /// 验证二进制文件是否是有效的 IPFS 可执行文件
    fn verify_binary(path: &PathBuf) -> bool {
        // 检查文件是否可执行
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
        
        // 尝试执行 version 命令验证
        match Command::new(path)
            .arg("version")
            .output()
        {
            Ok(output) => {
                let version_str = String::from_utf8_lossy(&output.stdout);
                let is_valid = output.status.success() && version_str.contains("ipfs");
                
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
    
    /// 获取 IPFS 版本信息
    pub fn get_version(binary_path: &PathBuf) -> Result<String, String> {
        let output = Command::new(binary_path)
            .arg("version")
            .output()
            .map_err(|e| format!("Failed to get version: {}", e))?;
        
        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_string();
            Ok(version)
        } else {
            Err("Failed to get version".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_find_binary() {
        // 这个测试只在有 IPFS 安装的环境中才会通过
        let result = BinaryFinder::find();
        println!("Found binary: {:?}", result);
    }
}
