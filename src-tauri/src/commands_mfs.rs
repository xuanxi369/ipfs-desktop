use crate::error::DaemonError;
use crate::path_security::{validate_mfs_path, validate_mfs_source};
use crate::state::AppState;
use tauri::State;

async fn client(state: &AppState) -> Result<crate::daemon::IpfsApiClient, DaemonError> {
    state
        .get_api_client()
        .await
        .ok_or(DaemonError::InvalidState)
}

#[tauri::command]
pub async fn mfs_ls(
    state: State<'_, AppState>,
    path: String,
) -> Result<crate::daemon::MfsLsResult, DaemonError> {
    validate_mfs_path(&path)?;
    client(&state).await?.files_ls(&path).await
}
#[tauri::command]
pub async fn mfs_stat(
    state: State<'_, AppState>,
    path: String,
) -> Result<crate::daemon::MfsStatResult, DaemonError> {
    validate_mfs_path(&path)?;
    client(&state).await?.files_stat(&path).await
}
#[tauri::command]
pub async fn mfs_mkdir(
    state: State<'_, AppState>,
    path: String,
    parents: bool,
) -> Result<(), DaemonError> {
    validate_mfs_path(&path)?;
    client(&state).await?.files_mkdir(&path, parents).await
}
#[tauri::command]
pub async fn mfs_rm(
    state: State<'_, AppState>,
    path: String,
    recursive: bool,
) -> Result<(), DaemonError> {
    validate_mfs_path(&path)?;
    if path.trim() == "/" {
        return Err(DaemonError::ApiError("refusing to remove MFS root".into()));
    }
    client(&state).await?.files_rm(&path, recursive).await
}
#[tauri::command]
pub async fn mfs_cp(
    state: State<'_, AppState>,
    source: String,
    dest: String,
) -> Result<(), DaemonError> {
    validate_mfs_source(&source)?;
    validate_mfs_path(&dest)?;
    client(&state).await?.files_cp(&source, &dest).await
}
#[tauri::command]
pub async fn mfs_mv(
    state: State<'_, AppState>,
    source: String,
    dest: String,
) -> Result<(), DaemonError> {
    validate_mfs_path(&source)?;
    validate_mfs_path(&dest)?;
    if source.trim() == "/" {
        return Err(DaemonError::ApiError("refusing to move MFS root".into()));
    }
    client(&state).await?.files_mv(&source, &dest).await
}
#[tauri::command]
pub async fn mfs_read(state: State<'_, AppState>, path: String) -> Result<Vec<u8>, DaemonError> {
    validate_mfs_path(&path)?;
    client(&state).await?.files_read(&path).await
}
#[tauri::command]
pub async fn mfs_write(
    state: State<'_, AppState>,
    path: String,
    content: Vec<u8>,
    create: bool,
    truncate: bool,
) -> Result<(), DaemonError> {
    validate_mfs_path(&path)?;
    client(&state)
        .await?
        .files_write(&path, content, create, truncate)
        .await
}
