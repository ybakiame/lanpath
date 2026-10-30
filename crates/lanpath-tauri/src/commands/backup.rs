//! Encrypted backup IPC. Only ciphertext crosses the file export adapter.
use crate::state::AppState;
use base64::{Engine, engine::general_purpose::STANDARD};
use lanpath_client::backup::{self, Appearance, BackupSummary};
use lanpath_common::LanpathError;
#[cfg(not(target_os = "android"))]
use tauri::Manager;
use tauri::State;

fn unpack(content: &str) -> Result<Vec<u8>, String> {
    if content.len() > backup::MAX_ARCHIVE.div_ceil(3) * 4 {
        return Err("备份文件超过 16 MiB".into());
    }
    STANDARD
        .decode(content)
        .map_err(|_| "备份文件编码无效".into())
}

// These read/acknowledgement commands historically expose the underlying message.
fn read_error(error: LanpathError) -> String {
    match error {
        LanpathError::Client(message) => message,
        LanpathError::Io(error) => error.to_string(),
        LanpathError::Serialization(error) => error.to_string(),
        error => error.to_string(),
    }
}

#[tauri::command]
pub fn backup_generate_password() -> String {
    backup::generate_password()
}

#[tauri::command]
pub async fn backup_export(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    password: String,
    appearance: Option<Appearance>,
) -> Result<String, String> {
    let service = std::sync::Arc::clone(&state.backup);
    // Capture and scrypt are blocking work, never performed on the IPC event loop.
    let bytes = tauri::async_runtime::spawn_blocking(move || service.export(&password, appearance))
        .await
        .map_err(|_| "备份任务未能完成".to_string())?
        .map_err(|e| e.to_string())?;
    let filename = format!("Lanpath-backup-{}.zip.age", uuid::Uuid::new_v4());
    #[cfg(target_os = "android")]
    {
        let _ = app;
        let handle = crate::core_bridge::vpn_plugin_handle().ok_or("文件导出服务未就绪")?;
        handle
            .run_mobile_plugin_async::<String>(
                "exportBackup",
                serde_json::json!({
                    "filename": filename, "content": STANDARD.encode(bytes),
                }),
            )
            .await
            .map_err(|_| "无法导出备份到下载目录".into())
    }
    #[cfg(not(target_os = "android"))]
    {
        let target = app
            .path()
            .download_dir()
            .map_err(|_| "无法读取下载目录")?
            .join("Lanpath");
        std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        let path = target.join(filename);
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        Ok(path.to_string_lossy().into_owned())
    }
}

#[tauri::command]
pub async fn backup_preview(
    state: State<'_, AppState>,
    content: String,
    password: String,
) -> Result<backup::BackupPreview, String> {
    let service = std::sync::Arc::clone(&state.backup);
    let work_service = std::sync::Arc::clone(&service);
    let preview = tauri::async_runtime::spawn_blocking(move || {
        let bytes = unpack(&content)?;
        work_service
            .inspect(&bytes, password)
            .map_err(|_| "备份无法解密或校验：请检查密码、文件完整性和备份版本".to_string())
    })
    .await
    .map_err(|_| "备份校验任务未能完成".to_string())??;
    let id = preview.preview_id.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
        service.discard(&id);
    });
    Ok(preview)
}

#[tauri::command]
pub async fn backup_prepare_restore(
    state: State<'_, AppState>,
    preview_id: String,
    appearance: Option<Appearance>,
) -> Result<String, String> {
    let prepared = state
        .backup
        .take_preview(&preview_id)
        .map_err(|e| e.to_string())?;
    let mut lock = state.client.lock().await;
    if let Some(client) = lock.as_mut() {
        client
            .stop()
            .await
            .map_err(|e| format!("系统代理或核心未能安全停止；未恢复备份: {e}"))?;
    }
    *lock = None;
    let dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || prepared.prepare(&dir, appearance))
        .await
        .map_err(|_| "恢复准备未能完成".to_string())?
        .map_err(|e| e.to_string())
}

/// Read-only startup result; frontend acknowledges after applying appearance.
#[tauri::command]
pub fn backup_restored(state: State<'_, AppState>) -> Result<Option<BackupSummary>, String> {
    state.backup.restored().map_err(read_error)
}

#[tauri::command]
pub fn backup_acknowledge(state: State<'_, AppState>) -> Result<(), String> {
    state.backup.acknowledge().map_err(read_error)
}

#[tauri::command]
pub async fn backup_restart(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if !state.backup.is_pending() {
        return Err("没有待恢复的配置".into());
    }
    #[cfg(target_os = "android")]
    {
        let _ = app;
        let handle = crate::core_bridge::vpn_plugin_handle().ok_or("关闭服务未就绪")?;
        handle
            .run_mobile_plugin_async::<()>("closeForRestore", serde_json::json!({}))
            .await
            .map_err(|_| "无法关闭应用，请在系统应用设置中停止后重新打开".into())
    }
    #[cfg(not(target_os = "android"))]
    {
        app.request_restart();
        Ok(())
    }
}

#[tauri::command]
pub fn backup_pending(state: State<'_, AppState>) -> bool {
    state.backup.is_pending()
}

#[tauri::command]
pub async fn backup_previous(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let service = std::sync::Arc::clone(&state.backup);
    tauri::async_runtime::spawn_blocking(move || {
        service
            .previous()
            .map(|bytes| bytes.map(|bytes| STANDARD.encode(bytes)))
            .map_err(read_error)
    })
    .await
    .map_err(|_| "无法读取恢复前备份".to_string())?
}
