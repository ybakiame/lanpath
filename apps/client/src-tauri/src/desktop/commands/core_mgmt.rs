//! Thin IPC adapter for shared, framework-free core management.

use std::path::Path;

use lanpath_client::core_management::{CoreManagementService, ManagedCore, RemoteChannel};
use tauri::State;

use crate::desktop::state::AppState;

#[tauri::command]
pub async fn list_cores(state: State<'_, AppState>) -> Result<Vec<ManagedCore>, String> {
    CoreManagementService::new(state.data_dir.clone())
        .list_local()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn list_remote_core_channels(
    state: State<'_, AppState>,
) -> Result<Vec<RemoteChannel>, String> {
    CoreManagementService::new(state.data_dir.clone())
        .list_remote_channels()
        .await
        .map_err(|error| format!("拉取远端版本失败: {error}"))
}

#[tauri::command]
pub async fn list_downloaded_versions(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    CoreManagementService::new(state.data_dir.clone())
        .list_downloaded_versions()
        .map_err(|error| error.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn download_core(
    state: State<'_, AppState>,
    version: String,
) -> Result<ManagedCore, String> {
    CoreManagementService::new(state.data_dir.clone())
        .download(&version)
        .await
        .map_err(|error| format!("下载核心失败: {error}"))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn set_active_core(state: State<'_, AppState>, path: String) -> Result<(), String> {
    CoreManagementService::new(state.data_dir.clone())
        .select(Path::new(&path))
        .map_err(|error| error.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn delete_core(state: State<'_, AppState>, path: String) -> Result<(), String> {
    CoreManagementService::new(state.data_dir.clone())
        .delete(Path::new(&path))
        .map_err(|error| format!("删除核心失败: {error}"))
}
