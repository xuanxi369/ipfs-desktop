use std::path::Path;

pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    let file = std::fs::OpenOptions::new().read(true).open(&tmp)?;
    file.sync_all()?;
    std::fs::rename(&tmp, path)
}
