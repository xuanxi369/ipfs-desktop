//! SQLite 缓存层
//!
//! 缓存 Pin 列表、节点状态、Repo 统计等高频查询数据，
//! 减少对 Kubo HTTP API 的穿透调用。
//!
//! 缓存策略：
//! - Pin 列表：TTL 30 秒
//! - 仪表盘数据（peers/bandwidth/bitswap）：TTL 10 秒
//! - Repo 统计：TTL 60 秒
//! - 节点 ID/版本：TTL 300 秒（基本不变）

use rusqlite::{Connection, params};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::Mutex;

/// 缓存条目 TTL（秒）
const TTL_DASHBOARD: u64 = 10;
const TTL_PINS: u64 = 30;
const TTL_REPO: u64 = 60;
const TTL_NODE_INFO: u64 = 300;

/// SQLite 缓存管理器
pub struct CacheStore {
    conn: Mutex<Connection>,
}

impl CacheStore {
    /// 打开或创建缓存数据库
    pub fn new(db_path: PathBuf) -> Result<Self, String> {
        // 确保父目录存在
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create cache dir: {}", e))?;
        }

        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open cache db: {}", e))?;

        // 创建缓存表
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cache (
                key    TEXT PRIMARY KEY,
                value  TEXT NOT NULL,
                ts     INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_cache_ts ON cache(ts);"
        ).map_err(|e| format!("Failed to create cache tables: {}", e))?;

        tracing::info!("Cache store opened at {:?}", db_path);

        Ok(Self { conn: Mutex::new(conn) })
    }

    /// 获取缓存值（若未过期）
    fn get(&self, key: &str, ttl_secs: u64) -> Option<String> {
        let conn = self.conn.lock().ok()?;
        let now = now_secs();

        let mut stmt = conn.prepare(
            "SELECT value, ts FROM cache WHERE key = ?1"
        ).ok()?;

        let result = stmt.query_row(params![key], |row| {
            let value: String = row.get(0)?;
            let ts: i64 = row.get(1)?;
            Ok((value, ts as u64))
        }).ok()?;

        if now.saturating_sub(result.1) < ttl_secs {
            Some(result.0)
        } else {
            // 过期，删除
            let _ = conn.execute("DELETE FROM cache WHERE key = ?1", params![key]);
            None
        }
    }

    /// 写入缓存值
    fn set(&self, key: &str, value: &str) {
        if let Ok(conn) = self.conn.lock() {
            let now = now_secs() as i64;
            let _ = conn.execute(
                "INSERT OR REPLACE INTO cache (key, value, ts) VALUES (?1, ?2, ?3)",
                params![key, value, now],
            );
        }
    }

    /// 清除过期条目
    pub fn prune(&self) {
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute(
                "DELETE FROM cache WHERE ts < ?1",
                params![(now_secs() - 600) as i64], // 10 分钟以上的全清
            );
        }
    }

    // ── 类型化缓存接口 ──

    /// 缓存仪表盘 JSON
    pub fn get_dashboard(&self) -> Option<String> {
        self.get("dashboard", TTL_DASHBOARD)
    }
    pub fn set_dashboard(&self, json: &str) {
        self.set("dashboard", json);
    }

    /// 缓存 Pin 列表 JSON
    pub fn get_pins(&self) -> Option<String> {
        self.get("pins", TTL_PINS)
    }
    pub fn set_pins(&self, json: &str) {
        self.set("pins", json);
    }

    /// 缓存 Repo 统计 JSON
    pub fn get_repo_stats(&self) -> Option<String> {
        self.get("repo_stats", TTL_REPO)
    }
    pub fn set_repo_stats(&self, json: &str) {
        self.set("repo_stats", json);
    }

    /// 缓存节点信息 JSON
    pub fn get_node_info(&self) -> Option<String> {
        self.get("node_info", TTL_NODE_INFO)
    }
    pub fn set_node_info(&self, json: &str) {
        self.set("node_info", json);
    }

    /// 缓存 Swarm peers JSON
    pub fn get_peers(&self) -> Option<String> {
        self.get("peers", TTL_DASHBOARD)
    }
    pub fn set_peers(&self, json: &str) {
        self.set("peers", json);
    }

    /// 缓存带宽统计 JSON
    pub fn get_bandwidth(&self) -> Option<String> {
        self.get("bandwidth", TTL_DASHBOARD)
    }
    pub fn set_bandwidth(&self, json: &str) {
        self.set("bandwidth", json);
    }

    /// 缓存 Bitswap 统计 JSON
    pub fn get_bitswap(&self) -> Option<String> {
        self.get("bitswap", TTL_DASHBOARD)
    }
    pub fn set_bitswap(&self, json: &str) {
        self.set("bitswap", json);
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> CacheStore {
        let path = std::env::temp_dir().join("ipfs-cache-test.db");
        let _ = std::fs::remove_file(&path);
        CacheStore::new(path).unwrap()
    }

    #[test]
    fn test_cache_set_get() {
        let store = temp_db();
        store.set_dashboard(r#"{"test":true}"#);
        let val = store.get_dashboard();
        assert_eq!(val, Some(r#"{"test":true}"#.to_string()));
    }

    #[test]
    fn test_cache_expiry() {
        let store = temp_db();
        // 直接写入一个"已过期"的条目（使用 get 的通用方法测试）
        store.set("expired_key", "stale");
        // 使用 TTL=0 应该立即视为过期
        let val = store.get("expired_key", 0);
        assert_eq!(val, None);
    }

    #[test]
    fn test_cache_miss() {
        let store = temp_db();
        let val = store.get_dashboard();
        assert_eq!(val, None);
    }
}
