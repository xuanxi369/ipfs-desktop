use crate::error::DaemonError;
use std::path::{Component, Path, PathBuf};

pub fn validate_cid(cid: &str) -> Result<&str, DaemonError> {
    let value = cid.trim();
    if value.is_empty() || value.len() > 256 {
        return Err(DaemonError::ApiError("invalid CID".into()));
    }
    value
        .parse::<cid::Cid>()
        .map_err(|_| DaemonError::ApiError("invalid CID".into()))?;
    Ok(value)
}

pub fn validate_output_path(path: &Path) -> Result<(), DaemonError> {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(DaemonError::IoError("output path must be absolute".into()));
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(DaemonError::IoError(
            "output path cannot contain '..'".into(),
        ));
    }

    let parent = path
        .parent()
        .ok_or_else(|| DaemonError::IoError("output path has no parent".into()))?;
    let existing = nearest_existing_ancestor(parent)?;
    let canonical = std::fs::canonicalize(&existing)
        .map_err(|e| DaemonError::IoError(format!("failed to resolve output directory: {e}")))?;
    reject_sensitive_root(&canonical)?;

    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            return Err(DaemonError::IoError(
                "refusing to overwrite a symbolic link".into(),
            ));
        }
        if !meta.is_file() {
            return Err(DaemonError::IoError(
                "output path is not a regular file".into(),
            ));
        }
    }
    Ok(())
}

fn nearest_existing_ancestor(path: &Path) -> Result<PathBuf, DaemonError> {
    let mut current = path;
    loop {
        if current.exists() {
            return Ok(current.to_path_buf());
        }
        current = current
            .parent()
            .ok_or_else(|| DaemonError::IoError("output path has no existing ancestor".into()))?;
    }
}

fn reject_sensitive_root(path: &Path) -> Result<(), DaemonError> {
    let sensitive = [dirs::config_dir(), dirs::data_local_dir()]
        .into_iter()
        .flatten()
        .map(|p| p.join("ipfs-desktop-rust"));
    for root in sensitive {
        if let Ok(root) = std::fs::canonicalize(root) {
            if path.starts_with(root) {
                return Err(DaemonError::IoError(
                    "refusing to export into application data".into(),
                ));
            }
        }
    }
    Ok(())
}

pub fn validate_mfs_path(path: &str) -> Result<(), DaemonError> {
    let p = path.trim();
    if p.is_empty() || p.len() > 4096 || !p.starts_with('/') || p.contains('\0') {
        return Err(DaemonError::ApiError("invalid MFS path".into()));
    }
    if p.split('/').any(|part| part == "." || part == "..") {
        return Err(DaemonError::ApiError(
            "MFS path traversal is not allowed".into(),
        ));
    }
    Ok(())
}

pub fn validate_mfs_source(path: &str) -> Result<(), DaemonError> {
    let p = path.trim();
    if (p.starts_with("/ipfs/") || p.starts_with("/ipns/"))
        && p.len() <= 4096
        && !p.contains('\0')
        && !p.split('/').any(|x| x == "." || x == "..")
    {
        return Ok(());
    }
    validate_mfs_path(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_mfs_traversal() {
        assert!(validate_mfs_path("/safe/file").is_ok());
        assert!(validate_mfs_path("relative").is_err());
        assert!(validate_mfs_path("/safe/../secret").is_err());
    }

    #[test]
    fn validates_cid_semantics() {
        assert!(validate_cid("QmYwAPJzv5CZsnA625s3Xf2nemtYgPpHdWEz79ojWnPbdG").is_ok());
        assert!(
            validate_cid("bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi").is_ok()
        );
        assert!(validate_cid("../../secret").is_err());
        assert!(validate_cid("not-a-cid").is_err());
        // Looks like a CIDv0/CIDv1, but has an invalid multihash payload.
        assert!(validate_cid("Qm11111111111111111111111111111111111111111111").is_err());
        assert!(validate_cid("bafy-not-a-real-multihash").is_err());
    }
}
