//! 离线队列 — Phase 3
//!
//! 当 Kubo 守护进程不可用时（断网、进程崩溃、API 不可达），
//! 将写操作序列化到本地 SQLite 队列中。
//! 恢复连接后按 FIFO 顺序自动重放。
//!
//! 支持的离线操作：
//! - add_file (CID + 本地路径)
//! - pin_add (CID)
//! - pin_rm (CID)
//! - ipns_publish (CID + key_name + lifetime)

use rusqlite::{Connection, params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

// ════════════════════════════════════════════════════════════════
// 离线操作类型
// ════════════════════════════════════════════════════════════════

/// 可排队的离线操作
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", content = "payload")]
pub enum OfflineOperation {
    /// 添加文件（CID 预留 + 本地路径 — 实际添加发生在联网后）
    AddFile {
        file_path: String,
        queued_at: u64,
    },
    /// Pin 添加
    PinAdd {
        cid: String,
        queued_at: u64,
    },
    /// Pin 移除
    PinRm {
        cid: String,
        queued_at: u64,
    },
    /// IPNS 发布
    IpnsPublish {
        cid: String,
        key_name: String,
        lifetime: String,
        queued_at: u64,
    },
}

/// 队列条目（存储在 SQLite）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEntry {
    pub id: i64,
    pub operation: OfflineOperation,
    pub created_at: u64,
    pub retry_count: u32,
    pub last_error: Option<String>,
}

// ════════════════════════════════════════════════════════════════
// 离线队列管理器
// ════════════════════════════════════════════════════════════════

pub struct OfflineQueue {
    conn: Mutex<Connection>,
}

impl OfflineQueue {
    /// 打开或创建离线队列数据库
    pub fn new(db_path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create offline queue dir: {}", e))?;
        }

        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open offline queue db: {}", e))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS offline_queue (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                op_json    TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                retry_count INTEGER DEFAULT 0,
                last_error TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_queue_created ON offline_queue(created_at);"
        ).map_err(|e| format!("Failed to create offline queue tables: {}", e))?;

        tracing::info!("Offline queue opened at {:?} ({} pending)",
            db_path,
            conn.query_row("SELECT COUNT(*) FROM offline_queue", [], |r| r.get::<_, i64>(0))
                .unwrap_or(0)
        );

        Ok(Self { conn: Mutex::new(conn) })
    }

    /// 将操作加入队列
    pub fn enqueue(&self, op: OfflineOperation) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let json = serde_json::to_string(&op)
            .map_err(|e| format!("Serialize error: {}", e))?;
        let now = now_secs() as i64;

        conn.execute(
            "INSERT INTO offline_queue (op_json, created_at) VALUES (?1, ?2)",
            params![json, now],
        ).map_err(|e| format!("Insert error: {}", e))?;

        let id = conn.last_insert_rowid();
        tracing::info!("OfflineQueue: enqueued operation id={}", id);
        Ok(id)
    }

    /// 获取下一个待处理的条目（FIFO）
    pub fn dequeue(&self) -> Result<Option<QueueEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;

        let mut stmt = conn.prepare(
            "SELECT id, op_json, created_at, retry_count, last_error
             FROM offline_queue ORDER BY created_at ASC LIMIT 1"
        ).map_err(|e| e.to_string())?;

        let result: Option<QueueEntry> = stmt.query_row([], |row| {
            let id: i64 = row.get(0)?;
            let json: String = row.get(1)?;
            let created_at: i64 = row.get(2)?;
            let retry_count: u32 = row.get(3)?;
            let last_error: Option<String> = row.get(4)?;

            let operation: OfflineOperation = serde_json::from_str(&json)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                    0, rusqlite::types::Type::Text,
                    Box::new(e)
                ))?;

            Ok(QueueEntry { id, operation, created_at: created_at as u64, retry_count, last_error })
        }).optional()
          .map_err(|e| e.to_string())?;

        Ok(result)
    }

    /// 标记条目为已完成（从队列移除）
    pub fn complete(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM offline_queue WHERE id = ?1", params![id])
            .map_err(|e| format!("Delete error: {}", e))?;
        tracing::info!("OfflineQueue: completed id={}", id);
        Ok(())
    }

    /// 增加重试计数并记录错误
    pub fn record_failure(&self, id: i64, error: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE offline_queue SET retry_count = retry_count + 1, last_error = ?1 WHERE id = ?2",
            params![error, id],
        ).map_err(|e| format!("Update error: {}", e))?;
        tracing::warn!("OfflineQueue: id={} failed (retry {}): {}", id,
            conn.query_row("SELECT retry_count FROM offline_queue WHERE id = ?1", params![id],
                |r| r.get::<_, u32>(0)).unwrap_or(0),
            error);
        Ok(())
    }

    /// 丢弃超过最大重试次数的条目
    pub fn purge_stale(&self, max_retries: u32) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let deleted = conn.execute(
            "DELETE FROM offline_queue WHERE retry_count >= ?1",
            params![max_retries],
        ).map_err(|e| e.to_string())?;
        if deleted > 0 {
            tracing::warn!("OfflineQueue: purged {} stale entries", deleted);
        }
        Ok(deleted)
    }

    /// 获取队列长度
    pub fn len(&self) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.query_row("SELECT COUNT(*) FROM offline_queue", [], |r| r.get::<_, i64>(0))
            .map(|c| c as usize)
            .map_err(|e| e.to_string())
    }

    /// 队列是否为空
    pub fn is_empty(&self) -> Result<bool, String> {
        self.len().map(|l| l == 0)
    }

    /// 获取所有待处理条目（用于前端展示）
    pub fn list_all(&self) -> Result<Vec<QueueEntry>, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;

        let mut stmt = conn.prepare(
            "SELECT id, op_json, created_at, retry_count, last_error
             FROM offline_queue ORDER BY created_at ASC"
        ).map_err(|e| e.to_string())?;

        let iter = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let json: String = row.get(1)?;
            let created_at: i64 = row.get(2)?;
            let retry_count: u32 = row.get(3)?;
            let last_error: Option<String> = row.get(4)?;

            let operation: OfflineOperation = serde_json::from_str(&json)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                    0, rusqlite::types::Type::Text,
                    Box::new(e)
                ))?;

            Ok(QueueEntry { id, operation, created_at: created_at as u64, retry_count, last_error })
        }).map_err(|e| e.to_string())?;

        let mut entries = Vec::new();
        for entry in iter {
            entries.push(entry.map_err(|e| e.to_string())?);
        }
        Ok(entries)
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ════════════════════════════════════════════════════════════════
// 重放引擎
// ════════════════════════════════════════════════════════════════

/// 离线队列重放引擎
///
/// 在检测到 Kubo API 恢复后，按 FIFO 顺序重放所有离线操作。
/// 最大重试 3 次，超过则丢弃。
pub struct ReplayEngine {
    queue: Arc<OfflineQueue>,
    max_retries: u32,
}

impl ReplayEngine {
    pub fn new(queue: Arc<OfflineQueue>) -> Self {
        Self { queue, max_retries: 3 }
    }

    /// 对单个离线操作执行重放
    ///
    /// 需要调用方提供 IpfsApiClient 来实际执行操作。
    pub async fn replay_one(
        &self,
        entry: &QueueEntry,
        api: &crate::daemon::IpfsApiClient,
    ) -> Result<(), String> {
        match &entry.operation {
            OfflineOperation::AddFile { file_path, .. } => {
                let path = std::path::PathBuf::from(file_path);
                if !path.exists() {
                    return Err(format!("File no longer exists: {}", file_path));
                }
                api.add_file(&path).await.map_err(|e| e.to_string())?;
                tracing::info!("Replay: added file {}", file_path);
            }
            OfflineOperation::PinAdd { cid, .. } => {
                api.pin_add(cid).await.map_err(|e| e.to_string())?;
                tracing::info!("Replay: pinned {}", cid);
            }
            OfflineOperation::PinRm { cid, .. } => {
                api.pin_rm(cid).await.map_err(|e| e.to_string())?;
                tracing::info!("Replay: unpinned {}", cid);
            }
            OfflineOperation::IpnsPublish { cid, key_name, lifetime, .. } => {
                api.name_publish(cid, key_name, lifetime).await.map_err(|e| e.to_string())?;
                tracing::info!("Replay: published IPNS {} -> {}", cid, key_name);
            }
        }
        Ok(())
    }

    /// 重放所有待处理条目
    ///
    /// 返回 (成功数, 失败数)。每个条目最多重试 max_retries 次，超过则丢弃。
    pub async fn replay_all(&self, api: &crate::daemon::IpfsApiClient) -> (usize, usize) {
        let mut success = 0usize;
        let mut failed = 0usize;

        loop {
            let entry = match self.queue.dequeue() {
                Ok(Some(e)) => e,
                Ok(None) => break,
                Err(e) => {
                    tracing::error!("Dequeue error: {}", e);
                    break;
                }
            };

            // 超过最大重试次数 → 丢弃
            if entry.retry_count >= self.max_retries {
                tracing::warn!(
                    "Entry id={} exceeded max retries ({}), discarding",
                    entry.id, self.max_retries
                );
                let _ = self.queue.complete(entry.id);
                failed += 1;
                continue;
            }

            match self.replay_one(&entry, api).await {
                Ok(()) => {
                    let _ = self.queue.complete(entry.id);
                    success += 1;
                }
                Err(e) => {
                    let _ = self.queue.record_failure(entry.id, &e);
                    failed += 1;
                    // record_failure 后条目仍在队列中（retry_count+1），
                    // 下次循环会重新 dequeue 并检查 retry_count
                }
            }
        }

        // 清理剩余的过期条目（以防万一）
        if let Err(e) = self.queue.purge_stale(self.max_retries) {
            tracing::error!("Failed to purge stale entries: {}", e);
        }

        tracing::info!("Replay complete: {} succeeded, {} failed", success, failed);
        (success, failed)
    }
}

use std::sync::Arc;

#[cfg(test)]
mod tests {
    use super::*;

    fn test_queue() -> OfflineQueue {
        // 每个测试用独立文件，避免并行执行时共享同一 DB 互相干扰
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir()
            .join(format!("ipfs-offline-queue-test-{}-{}.db", std::process::id(), n));
        let _ = std::fs::remove_file(&path);
        OfflineQueue::new(path).unwrap()
    }

    #[test]
    fn test_enqueue_dequeue() {
        let queue = test_queue();
        let op = OfflineOperation::PinAdd {
            cid: "QmTest".to_string(),
            queued_at: now_secs(),
        };
        let id = queue.enqueue(op).unwrap();
        assert!(id > 0);

        let entry = queue.dequeue().unwrap().unwrap();
        assert_eq!(entry.id, id);
        assert!(matches!(entry.operation, OfflineOperation::PinAdd { .. }));

        queue.complete(id).unwrap();
        assert!(queue.is_empty().unwrap());
    }

    #[test]
    fn test_list_all() {
        let queue = test_queue();
        queue.enqueue(OfflineOperation::PinAdd {
            cid: "QmA".to_string(), queued_at: now_secs(),
        }).unwrap();
        queue.enqueue(OfflineOperation::PinRm {
            cid: "QmB".to_string(), queued_at: now_secs(),
        }).unwrap();

        let all = queue.list_all().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_retry_and_purge() {
        let queue = test_queue();
        let id = queue.enqueue(OfflineOperation::PinAdd {
            cid: "QmTest".to_string(), queued_at: now_secs(),
        }).unwrap();

        // 模拟多次失败
        queue.record_failure(id, "test error 1").unwrap();
        queue.record_failure(id, "test error 2").unwrap();
        queue.record_failure(id, "test error 3").unwrap();

        // 清理超过 2 次重试的
        let purged = queue.purge_stale(2).unwrap();
        assert_eq!(purged, 1);
        assert!(queue.is_empty().unwrap());
    }

    #[test]
    fn test_queue_len() {
        let queue = test_queue();
        assert_eq!(queue.len().unwrap(), 0);

        queue.enqueue(OfflineOperation::PinAdd {
            cid: "QmTest".to_string(), queued_at: now_secs(),
        }).unwrap();
        assert_eq!(queue.len().unwrap(), 1);
    }
}
