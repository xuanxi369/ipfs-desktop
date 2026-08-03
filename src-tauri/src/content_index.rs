use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentRecord {
    pub cid: String,
    pub name: String,
    pub size: u64,
    pub backend: String,
    pub added_at: i64,
}
pub struct ContentIndex {
    conn: Mutex<Connection>,
}
impl ContentIndex {
    pub fn new(path: PathBuf) -> Result<Self, String> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let c = Connection::open(path).map_err(|e| e.to_string())?;
        c.execute_batch("CREATE TABLE IF NOT EXISTS content_index(cid TEXT PRIMARY KEY,name TEXT NOT NULL,size INTEGER NOT NULL,backend TEXT NOT NULL,added_at INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS idx_content_added ON content_index(added_at DESC);").map_err(|e|e.to_string())?;
        Ok(Self {
            conn: Mutex::new(c),
        })
    }
    pub fn upsert(&self, r: &ContentRecord) -> Result<(), String> {
        let c = self
            .conn
            .lock()
            .map_err(|_| "content index lock poisoned".to_string())?;
        c.execute("INSERT OR REPLACE INTO content_index(cid,name,size,backend,added_at) VALUES (?1,?2,?3,?4,?5)",params![r.cid,r.name,r.size as i64,r.backend,r.added_at]).map_err(|e|e.to_string()).map(|_|())
    }
    pub fn list(&self) -> Result<Vec<ContentRecord>, String> {
        let c = self
            .conn
            .lock()
            .map_err(|_| "content index lock poisoned".to_string())?;
        let mut s = c
            .prepare(
                "SELECT cid,name,size,backend,added_at FROM content_index ORDER BY added_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = s
            .query_map([], |r| {
                Ok(ContentRecord {
                    cid: r.get(0)?,
                    name: r.get(1)?,
                    size: r.get::<_, i64>(2)? as u64,
                    backend: r.get(3)?,
                    added_at: r.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
    pub fn remove(&self, cid: &str) -> Result<(), String> {
        let c = self
            .conn
            .lock()
            .map_err(|_| "content index lock poisoned".to_string())?;
        c.execute("DELETE FROM content_index WHERE cid=?1", params![cid])
            .map_err(|e| e.to_string())
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn temp_index() -> ContentIndex {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "content-index-test-{}-{}.db",
            std::process::id(),
            n
        ));
        let _ = std::fs::remove_file(&path);
        ContentIndex::new(path).expect("failed to create test content index")
    }

    #[test]
    fn test_upsert_and_list() {
        let index = temp_index();

        let record = ContentRecord {
            cid: "QmTest123".to_string(),
            name: "test.txt".to_string(),
            size: 1024,
            backend: "kubo".to_string(),
            added_at: 1234567890,
        };

        index.upsert(&record).expect("upsert failed");

        let list = index.list().expect("list failed");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].cid, "QmTest123");
        assert_eq!(list[0].name, "test.txt");
        assert_eq!(list[0].size, 1024);
    }

    #[test]
    fn test_remove() {
        let index = temp_index();

        let record = ContentRecord {
            cid: "QmToRemove".to_string(),
            name: "remove.txt".to_string(),
            size: 512,
            backend: "iroh".to_string(),
            added_at: 9876543210,
        };

        index.upsert(&record).expect("upsert failed");
        assert_eq!(index.list().expect("list failed").len(), 1);

        index.remove("QmToRemove").expect("remove failed");
        assert_eq!(index.list().expect("list failed").len(), 0);
    }

    #[test]
    fn test_upsert_replaces_existing() {
        let index = temp_index();

        let record1 = ContentRecord {
            cid: "QmSame".to_string(),
            name: "old.txt".to_string(),
            size: 100,
            backend: "kubo".to_string(),
            added_at: 1000,
        };

        let record2 = ContentRecord {
            cid: "QmSame".to_string(),
            name: "new.txt".to_string(),
            size: 200,
            backend: "iroh".to_string(),
            added_at: 2000,
        };

        index.upsert(&record1).expect("upsert 1 failed");
        index.upsert(&record2).expect("upsert 2 failed");

        let list = index.list().expect("list failed");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "new.txt");
        assert_eq!(list[0].size, 200);
        assert_eq!(list[0].backend, "iroh");
    }

    #[test]
    fn test_list_ordered_by_added_at_desc() {
        let index = temp_index();

        let records = vec![
            ContentRecord {
                cid: "Qm1".to_string(),
                name: "first.txt".to_string(),
                size: 100,
                backend: "kubo".to_string(),
                added_at: 1000,
            },
            ContentRecord {
                cid: "Qm2".to_string(),
                name: "second.txt".to_string(),
                size: 200,
                backend: "kubo".to_string(),
                added_at: 2000,
            },
            ContentRecord {
                cid: "Qm3".to_string(),
                name: "third.txt".to_string(),
                size: 300,
                backend: "kubo".to_string(),
                added_at: 1500,
            },
        ];

        for record in &records {
            index.upsert(record).expect("upsert failed");
        }

        let list = index.list().expect("list failed");
        assert_eq!(list.len(), 3);
        // 应该按 added_at 降序排列
        assert_eq!(list[0].cid, "Qm2"); // 2000
        assert_eq!(list[1].cid, "Qm3"); // 1500
        assert_eq!(list[2].cid, "Qm1"); // 1000
    }
}
