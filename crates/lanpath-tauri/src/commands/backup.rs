//! Encrypted backup IPC. Only ciphertext crosses the file export adapter.
use crate::state::AppState;
use base64::{Engine, engine::general_purpose::STANDARD};
use lanpath_client::backup::{self, Appearance, BackupSummary};
#[cfg(not(target_os = "android"))]
use tauri::Manager;
use tauri::State;

fn unpack(content: &str, password: &str) -> Result<backup::Snapshot, String> {
    if content.len() > backup::MAX_ARCHIVE.div_ceil(3) * 4 {
        return Err("备份文件超过 16 MiB".into());
    }
    let bytes = STANDARD
        .decode(content)
        .map_err(|_| "备份文件编码无效".to_string())?;
    backup::decode(&bytes, password)
        .map_err(|_| "备份无法解密或校验：请检查密码、文件完整性和备份版本".into())
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
    let dir = state.data_dir.clone();
    // Capture and scrypt are blocking work, never performed on the IPC event loop.
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        let snapshot = backup::capture(&dir, appearance)?;
        backup::encode(&snapshot, &password)
    })
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
pub async fn backup_preview(content: String, password: String) -> Result<BackupSummary, String> {
    tauri::async_runtime::spawn_blocking(move || unpack(&content, &password).map(|s| s.summary))
        .await
        .map_err(|_| "备份校验任务未能完成".to_string())?
}

#[tauri::command]
pub async fn backup_prepare_restore(
    state: State<'_, AppState>,
    content: String,
    password: String,
    appearance: Option<Appearance>,
) -> Result<String, String> {
    let snapshot = tauri::async_runtime::spawn_blocking({
        let password = password.clone();
        move || unpack(&content, &password)
    })
    .await
    .map_err(|_| "恢复任务未能完成".to_string())??;
    let mut lock = state.client.lock().await;
    if let Some(client) = lock.as_mut() {
        client.stop().await;
    }
    *lock = None;
    let dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        backup::prepare_restore(&dir, snapshot, &password, appearance)
    })
    .await
    .map_err(|_| "恢复准备未能完成".to_string())?
    .map_err(|e| e.to_string())
}

/// Read-only startup result; frontend acknowledges after applying appearance.
#[tauri::command]
pub fn backup_restored(state: State<'_, AppState>) -> Result<Option<BackupSummary>, String> {
    if state.data_dir.join(".backup-failed").exists() {
        return Err("恢复未完成，原配置已保留。请重新选择备份并导入".into());
    }
    let file = state.data_dir.join(".backup-applied.json");
    if !file.exists() {
        return Ok(None);
    }
    serde_json::from_slice(&std::fs::read(file).map_err(|e| e.to_string())?)
        .map(Some)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn backup_acknowledge(state: State<'_, AppState>) -> Result<(), String> {
    let failure = state.data_dir.join(".backup-failed");
    if failure.exists() {
        std::fs::remove_file(failure).map_err(|e| e.to_string())?;
    }

    let file = state.data_dir.join(".backup-applied.json");
    if file.exists() {
        std::fs::remove_file(file).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn backup_restart(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if !state.data_dir.join(".backup-pending").exists() {
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
    state.data_dir.join(".backup-pending").exists()
}

#[tauri::command]
pub async fn backup_previous(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let dir = state.data_dir.join("backup-history");
    tauri::async_runtime::spawn_blocking(move || {
        if !dir.exists() {
            return Ok(None);
        }
        let mut files = std::fs::read_dir(dir)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_type().is_ok_and(|t| t.is_file())
                    && e.file_name().to_string_lossy().ends_with(".zip.age")
            })
            .collect::<Vec<_>>();
        files.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
        let Some(last) = files.last() else {
            return Ok(None);
        };
        if last.metadata().map_err(|e| e.to_string())?.len() > backup::MAX_ARCHIVE as u64 {
            return Err("回滚文件过大".into());
        }
        std::fs::read(last.path())
            .map(|b| Some(STANDARD.encode(b)))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "无法读取恢复前备份".to_string())?
}
