//! 密钥记录管理模块
//!
//! ## 设计说明（安全相关）
//!
//! 早期版本在本地用 ed25519-dalek 生成密钥对，并把 **base64 私钥明文** 写入
//! `data_local_dir/keys/*.json`，系统 keychain 只是被忽略错误的次要副本。这既
//! 存在私钥落盘泄露风险，又与真实 IPNS 脱节——因为 `ipns_publish` 实际使用的是
//! Kubo 自己管理的密钥（`key/gen` 在 Kubo 密钥库中另建了一把无关的密钥），本地那
//! 把私钥从不参与发布。
//!
//! 现在改为：**密钥的生成与私钥保管完全交给 Kubo**（Kubo 在自己的密钥库中管理），
//! 本模块只维护一份「标签 → IPNS 名称」的**公开**记录，用于离线展示与快速查询。
//! 本模块不再接触任何私钥，因此不存在私钥落盘 / 经 IPC 传给前端的问题。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 公开密钥记录（不含任何私钥，可安全落盘并传给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRecord {
    /// 公钥标识（IPNS 名称，等同 Kubo `key/gen` 返回的 Id）
    pub public_key: String,
    /// IPNS 名称（Kubo 返回的真实 PeerID / libp2p-key 标识，可用于 /ipns/<name>）
    pub ipns_name: String,
    /// 人类可读标签（同时作为 Kubo 侧的 key name）
    pub label: String,
}

impl KeyRecord {
    /// 由 Kubo `key/gen` / `key/list` 的 (name, id) 构造记录
    pub fn from_kubo(name: impl Into<String>, id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            public_key: id.clone(),
            ipns_name: id,
            label: name.into(),
        }
    }
}

/// 密钥记录仓库
///
/// 仅持久化公开记录（`KeyRecord`），不保存私钥。
pub struct KeyManager {
    /// 记录文件目录
    keys_dir: PathBuf,
}

impl Default for KeyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyManager {
    /// 创建密钥记录仓库（使用平台默认数据目录）
    pub fn new() -> Self {
        let keys_dir = dirs::data_local_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap())
            .join("ipfs-desktop-rust")
            .join("keys");
        Self::with_dir(keys_dir)
    }

    /// 使用指定目录创建（便于测试隔离，不污染真实用户目录）
    pub fn with_dir(keys_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&keys_dir);
        Self { keys_dir }
    }

    /// 保存 / 更新一条公开记录
    pub fn save_record(&self, record: &KeyRecord) -> Result<(), String> {
        let path = self.key_file_path(&record.label);
        let content = serde_json::to_string_pretty(record)
            .map_err(|e| format!("Failed to serialize key record: {}", e))?;
        std::fs::write(&path, content)
            .map_err(|e| format!("Failed to write key record: {}", e))?;
        tracing::info!("Saved key record: {} ({})", record.label, record.ipns_name);
        Ok(())
    }

    /// 按标签加载记录
    pub fn load_record(&self, label: &str) -> Result<KeyRecord, String> {
        let path = self.key_file_path(label);
        if !path.exists() {
            return Err(format!("Key '{}' not found", label));
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read key record: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse key record: {}", e))
    }

    /// 列出所有本地记录
    pub fn list_records(&self) -> Result<Vec<KeyRecord>, String> {
        let mut keys = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.keys_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "json") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(rec) = serde_json::from_str::<KeyRecord>(&content) {
                            keys.push(rec);
                        }
                    }
                }
            }
        }
        Ok(keys)
    }

    /// 用 Kubo 的权威密钥列表覆盖本地记录（保持一致）
    pub fn sync_from_kubo(&self, records: &[KeyRecord]) {
        // 清理已不存在于 Kubo 的本地记录
        if let Ok(existing) = self.list_records() {
            let live: std::collections::HashSet<&str> =
                records.iter().map(|r| r.label.as_str()).collect();
            for rec in existing {
                if !live.contains(rec.label.as_str()) {
                    let _ = std::fs::remove_file(self.key_file_path(&rec.label));
                }
            }
        }
        for rec in records {
            let _ = self.save_record(rec);
        }
    }

    /// 删除本地记录
    pub fn delete_record(&self, label: &str) -> Result<(), String> {
        let path = self.key_file_path(label);
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Failed to delete key record: {}", e))?;
        }
        tracing::info!("Deleted key record: {}", label);
        Ok(())
    }

    fn key_file_path(&self, label: &str) -> PathBuf {
        // 标签用于文件名，做最小化清洗防止路径穿越
        let safe: String = label
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        self.keys_dir.join(format!("{}.json", safe))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_manager() -> KeyManager {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir()
            .join(format!("ipfs-keyring-test-{}-{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&dir);
        KeyManager::with_dir(dir)
    }

    #[test]
    fn test_save_and_load_record() {
        let mgr = temp_manager();
        let rec = KeyRecord::from_kubo("test-key", "k51qzi5uqu5test");
        mgr.save_record(&rec).unwrap();

        let loaded = mgr.load_record("test-key").unwrap();
        assert_eq!(loaded.label, "test-key");
        assert_eq!(loaded.ipns_name, "k51qzi5uqu5test");
        assert_eq!(loaded.public_key, "k51qzi5uqu5test");
    }

    #[test]
    fn test_list_records() {
        let mgr = temp_manager();
        mgr.save_record(&KeyRecord::from_kubo("list-1", "k51a")).unwrap();
        mgr.save_record(&KeyRecord::from_kubo("list-2", "k51b")).unwrap();
        let keys = mgr.list_records().unwrap();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn test_delete_record() {
        let mgr = temp_manager();
        mgr.save_record(&KeyRecord::from_kubo("del", "k51c")).unwrap();
        mgr.delete_record("del").unwrap();
        assert!(mgr.load_record("del").is_err());
    }

    #[test]
    fn test_sync_from_kubo_prunes_stale() {
        let mgr = temp_manager();
        mgr.save_record(&KeyRecord::from_kubo("stale", "k51old")).unwrap();
        // Kubo 只报告 "fresh"，"stale" 应被清理
        mgr.sync_from_kubo(&[KeyRecord::from_kubo("fresh", "k51new")]);
        let keys = mgr.list_records().unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].label, "fresh");
    }

    #[test]
    fn test_label_sanitization() {
        let mgr = temp_manager();
        // 含路径分隔符的标签不应逃逸出 keys_dir
        let rec = KeyRecord::from_kubo("../evil", "k51x");
        mgr.save_record(&rec).unwrap();
        // 能按原标签读回（内部做了同样的清洗）
        assert!(mgr.load_record("../evil").is_ok());
    }
}
