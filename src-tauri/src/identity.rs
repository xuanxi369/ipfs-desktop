//! 节点身份 — Phase D1（可信个人节点的第一块）
//!
//! 「可信节点」的第一步是**可读、稳定、可验证的身份**：
//! 把一个人类可读标签（`My Node`）绑定到节点的密码学身份
//! （Kubo 的 PeerID / iroh 的 EndpointId——后者本身就是自证公钥）。
//!
//! 设计（节点无关）：
//! - 本模块只持久化**人类可读部分**（标签 + 创建时间）；
//! - 真正的 node_id 由各后端 `node_info()` 实时提供，不在此冗余存储；
//! - 因此本模块与 Kubo/iroh 无耦合，纯粹是「身份的 UX 记录」。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 持久化的节点身份记录（人类可读部分）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeIdentity {
    /// 人类可读标签，如 "Charles's Node"
    pub label: String,
    /// 首次创建时间（Unix 秒）
    pub created_at: u64,
}

impl Default for NodeIdentity {
    fn default() -> Self {
        Self {
            label: "My Node".to_string(),
            created_at: now_secs(),
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 身份记录仓库（单文件 JSON 持久化）
pub struct IdentityStore {
    path: PathBuf,
}

impl IdentityStore {
    pub fn new(path: PathBuf) -> Self {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self { path }
    }

    /// 载入身份；文件不存在则创建默认身份并落盘（保证 created_at 稳定）
    pub fn load(&self) -> NodeIdentity {
        if let Ok(s) = std::fs::read_to_string(&self.path) {
            if let Ok(id) = serde_json::from_str::<NodeIdentity>(&s) {
                return id;
            }
        }
        let id = NodeIdentity::default();
        self.persist(&id);
        id
    }

    /// 更新标签（保持 created_at 不变）
    pub fn set_label(&self, label: &str) -> Result<NodeIdentity, String> {
        let label = label.trim();
        if label.is_empty() {
            return Err("node label cannot be empty".to_string());
        }
        let mut id = self.load();
        id.label = label.to_string();
        self.persist(&id);
        Ok(id)
    }

    fn persist(&self, id: &NodeIdentity) {
        match serde_json::to_string_pretty(id) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&self.path, json) {
                    tracing::warn!("failed to persist node identity: {e}");
                }
            }
            Err(e) => tracing::warn!("failed to serialize node identity: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn temp_store() -> IdentityStore {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "ipfs-identity-test-{}-{}.json",
            std::process::id(),
            n
        ));
        let _ = std::fs::remove_file(&path);
        IdentityStore::new(path)
    }

    #[test]
    fn test_default_and_persist() {
        let s = temp_store();
        let id = s.load();
        assert_eq!(id.label, "My Node");
        assert!(id.created_at > 0);
        // 再次载入应从磁盘读回同一记录（created_at 稳定）
        let id2 = s.load();
        assert_eq!(id.created_at, id2.created_at);
    }

    #[test]
    fn test_set_label_persists_and_keeps_created_at() {
        let s = temp_store();
        let orig = s.load();
        let updated = s.set_label("Charles's Node").unwrap();
        assert_eq!(updated.label, "Charles's Node");
        assert_eq!(
            updated.created_at, orig.created_at,
            "created_at must be stable"
        );
        // 新实例从磁盘恢复标签
        let reloaded = s.load();
        assert_eq!(reloaded.label, "Charles's Node");
    }

    #[test]
    fn test_empty_label_rejected() {
        let s = temp_store();
        assert!(s.set_label("   ").is_err());
    }
}
