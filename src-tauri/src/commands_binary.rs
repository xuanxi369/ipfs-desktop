use crate::error::DaemonError;
use crate::state::AppState;
use serde::Serialize;
use tauri::State;

#[derive(Debug, Clone, Serialize)]
pub struct BinaryVerificationInfo {
    pub path: String,
    pub version: String,
    pub sha256: String,
    pub matches_known_hash: bool,
    pub platform: String,
}

#[tauri::command]
pub async fn get_binary_verification_info(
    _state: State<'_, AppState>,
) -> Result<BinaryVerificationInfo, DaemonError> {
    let binary_path = crate::daemon::BinaryFinder::find().ok_or(DaemonError::BinaryNotFound)?;
    let version = crate::daemon::BinaryFinder::get_version(&binary_path)?;
    let sha256 = crate::daemon::BinaryFinder::calculate_hash(&binary_path)
        .map_err(|e| DaemonError::BinaryVerificationFailed(e.to_string()))?;
    let matches_known_hash =
        crate::daemon::BinaryFinder::verify_against_known_hashes(&binary_path).unwrap_or(false);

    Ok(BinaryVerificationInfo {
        path: binary_path.to_string_lossy().to_string(),
        version,
        sha256,
        matches_known_hash,
        platform: crate::daemon::KuboHashes::get_current_platform(),
    })
}

#[tauri::command]
pub async fn set_binary_hash(
    state: State<'_, AppState>,
    hash: Option<String>,
) -> Result<(), DaemonError> {
    if let Some(ref value) = hash {
        if value.len() != 64 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(DaemonError::ConfigError(
                "Invalid hash format: must be 64 hexadecimal characters".to_string(),
            ));
        }
    }

    let mut config = state.get_config().await;
    config.kubo_binary_sha256 = hash;
    config.save().map_err(DaemonError::ConfigError)?;
    state.update_config(config).await;
    Ok(())
}
