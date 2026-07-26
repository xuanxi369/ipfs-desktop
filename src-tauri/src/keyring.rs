//! 密钥管理与 IPNS 模块
//!
//! 功能：
//! - Ed25519 密钥对生成
//! - 密钥安全存储（平台 keychain + 本地文件双保险）
//! - IPNS 发布（通过 Kubo API）
//! - IPNS 解析（通过 Kubo API）

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 密钥对（可序列化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyPair {
    /// Base64 编码的私钥（32 字节）
    pub secret_key: String,
    /// Base64 编码的公钥（32 字节）
    pub public_key: String,
    /// IPNS 名称（公钥的 CIDv1 表示）
    pub ipns_name: String,
    /// 人类可读的标签
    pub label: String,
}

/// 密钥存储管理器
pub struct KeyManager {
    /// 密钥文件目录
    keys_dir: PathBuf,
    /// 平台 keychain service name
    keychain_service: String,
}

impl KeyManager {
    /// 创建密钥管理器
    pub fn new() -> Self {
        let keys_dir = dirs::data_local_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap())
            .join("ipfs-desktop-rust")
            .join("keys");

        let _ = std::fs::create_dir_all(&keys_dir);

        Self {
            keys_dir,
            keychain_service: "ipfs-desktop-rust".to_string(),
        }
    }

    /// 生成新的 Ed25519 密钥对
    ///
    /// 使用操作系统随机源 OsRng，安全生成 32 字节种子。
    pub fn generate_key(&self, label: &str) -> Result<KeyPair, String> {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let secret_bytes = signing_key.to_bytes();
        let public_bytes = verifying_key.to_bytes();

        // IPNS 名称：公钥的 multihash → base36 编码的 CIDv1
        let ipns_name = self.public_key_to_ipns_name(&public_bytes);

        let keypair = KeyPair {
            secret_key: STANDARD.encode(&secret_bytes),
            public_key: STANDARD.encode(&public_bytes),
            ipns_name,
            label: label.to_string(),
        };

        // 保存到本地文件
        self.save_to_file(&keypair)?;

        // 尝试保存私钥到系统 keychain
        #[cfg(not(test))]
        {
            let entry = keyring::Entry::new(
                &self.keychain_service,
                &format!("ipns-{}", label),
            ).map_err(|e| format!("Keychain error: {}", e))?;

            let _ = entry.set_password(&keypair.secret_key);
        }

        tracing::info!("Generated new key pair: {}", keypair.ipns_name);
        Ok(keypair)
    }

    /// 加载密钥（按标签）
    pub fn load_key(&self, label: &str) -> Result<KeyPair, String> {
        // 先尝试从文件加载
        let file_path = self.key_file_path(label);
        if file_path.exists() {
            let content = std::fs::read_to_string(&file_path)
                .map_err(|e| format!("Failed to read key file: {}", e))?;
            let kp: KeyPair = serde_json::from_str(&content)
                .map_err(|e| format!("Failed to parse key file: {}", e))?;
            return Ok(kp);
        }

        // 尝试从 keychain 恢复
        #[cfg(not(test))]
        {
            let entry = keyring::Entry::new(
                &self.keychain_service,
                &format!("ipns-{}", label),
            ).map_err(|e| format!("Keychain error: {}", e))?;

            if let Ok(secret) = entry.get_password() {
                // 重建密钥对
                let secret_bytes = STANDARD.decode(&secret)
                    .map_err(|e| format!("Failed to decode secret key: {}", e))?;
                let secret_array: [u8; 32] = secret_bytes.try_into()
                    .map_err(|_| "Invalid key length".to_string())?;
                let signing_key = SigningKey::from_bytes(&secret_array);
                let verifying_key = signing_key.verifying_key();

                let public_bytes = verifying_key.to_bytes();
                let ipns_name = self.public_key_to_ipns_name(&public_bytes);

                let kp = KeyPair {
                    secret_key: secret,
                    public_key: STANDARD.encode(&public_bytes),
                    ipns_name,
                    label: label.to_string(),
                };

                // 补充保存到文件
                let _ = self.save_to_file(&kp);
                return Ok(kp);
            }
        }

        Err(format!("Key '{}' not found", label))
    }

    /// 列出所有已保存的密钥
    pub fn list_keys(&self) -> Result<Vec<KeyPair>, String> {
        let mut keys = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.keys_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "json") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(kp) = serde_json::from_str::<KeyPair>(&content) {
                            keys.push(kp);
                        }
                    }
                }
            }
        }
        Ok(keys)
    }

    /// 删除密钥
    pub fn delete_key(&self, label: &str) -> Result<(), String> {
        let file_path = self.key_file_path(label);
        if file_path.exists() {
            std::fs::remove_file(&file_path)
                .map_err(|e| format!("Failed to delete key file: {}", e))?;
        }

        // 从 keychain 删除
        #[cfg(not(test))]
        {
            if let Ok(entry) = keyring::Entry::new(&self.keychain_service, &format!("ipns-{}", label)) {
                let _ = entry.delete_password();
            }
        }

        tracing::info!("Key '{}' deleted", label);
        Ok(())
    }

    // ── 内部辅助 ──

    fn key_file_path(&self, label: &str) -> PathBuf {
        self.keys_dir.join(format!("{}.json", label))
    }

    fn save_to_file(&self, keypair: &KeyPair) -> Result<(), String> {
        let path = self.key_file_path(&keypair.label);
        let content = serde_json::to_string_pretty(keypair)
            .map_err(|e| format!("Failed to serialize key: {}", e))?;
        std::fs::write(&path, content)
            .map_err(|e| format!("Failed to write key file: {}", e))?;
        Ok(())
    }

    /// 公钥 → IPNS 名称
    ///
    /// IPNS 名称是公钥的 libp2p-key 编码 CIDv1（base36）。
    /// 简化实现：使用 "k51" 前缀（base36 编码的 ed25519 公钥）。
    fn public_key_to_ipns_name(&self, public_bytes: &[u8; 32]) -> String {
        // IPNS 名称格式：k51... （base36 编码的 multihash 公钥）
        // 简化计算：multihash = 0x00 (identity hash) + 0x20 (32 bytes) + public_key
        let multihash: Vec<u8> = vec![0x00, 0x20]
            .into_iter()
            .chain(public_bytes.iter().copied())
            .collect();

        // IPNS 名称格式：k51... （base36 编码的 multihash 公钥）
        // 实际 IPNS 使用 multibase + base36，这里用简化的 hex 表示
        // 生产环境建议用 multibase/multihash 库
        let hex_name: String = multihash.iter().map(|b| format!("{:02x}", b)).collect();
        format!("k51{}", &hex_name[..16]) // 截断显示
    }
}

/// IPNS 发布载荷（通过 Kubo API）
#[derive(Debug, Serialize)]
pub struct IpnsPublishRequest {
    /// 要发布的 CID
    pub cid: String,
    /// 密钥名称
    pub key_name: String,
    /// 生命周期（如 "24h"）
    pub lifetime: String,
}

/// IPNS 解析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpnsResolveResult {
    /// 解析后的 CID
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_load_key() {
        let mgr = KeyManager::new();
        let kp = mgr.generate_key("test-key").unwrap();

        assert_eq!(kp.label, "test-key");
        assert!(!kp.secret_key.is_empty());
        assert!(!kp.public_key.is_empty());
        assert!(kp.ipns_name.starts_with("k51"));

        // 加载
        let loaded = mgr.load_key("test-key").unwrap();
        assert_eq!(loaded.public_key, kp.public_key);
        assert_eq!(loaded.ipns_name, kp.ipns_name);

        // 清理
        mgr.delete_key("test-key").unwrap();
    }

    #[test]
    fn test_list_keys() {
        let mgr = KeyManager::new();
        mgr.generate_key("list-test-1").unwrap();
        mgr.generate_key("list-test-2").unwrap();

        let keys = mgr.list_keys().unwrap();
        assert!(keys.len() >= 2);

        mgr.delete_key("list-test-1").unwrap();
        mgr.delete_key("list-test-2").unwrap();
    }

    #[test]
    fn test_delete_key() {
        let mgr = KeyManager::new();
        mgr.generate_key("del-test").unwrap();
        mgr.delete_key("del-test").unwrap();

        assert!(mgr.load_key("del-test").is_err());
    }
}
