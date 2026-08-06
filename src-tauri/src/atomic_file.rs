use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp-{}-{sequence}", std::process::id()));
    let backup = path.with_extension(format!("bak-{}-{sequence}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&tmp)?;
    file.sync_all()?;
    // Windows does not allow renaming an open file and does not replace an
    // existing destination. Close the handle, then use a recoverable swap.
    drop(file);

    if path.exists() {
        std::fs::rename(path, &backup)?;
    }

    match std::fs::rename(&tmp, path) {
        Ok(()) => {
            if backup.exists() {
                let _ = std::fs::remove_file(&backup);
            }
            Ok(())
        }
        Err(error) => {
            if backup.exists() {
                let _ = std::fs::rename(&backup, path);
            }
            let _ = std::fs::remove_file(&tmp);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_existing_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("state.json");
        write_atomic(&path, b"first").expect("initial write");
        write_atomic(&path, b"second").expect("replacement write");
        assert_eq!(std::fs::read(&path).expect("read result"), b"second");
    }
}
