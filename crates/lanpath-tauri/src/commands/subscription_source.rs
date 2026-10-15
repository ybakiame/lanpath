//! Original response inspection/export and dashboard override association.

use lanpath_client::{ProfileStoreV2, SubscriptionSource, SubscriptionStore};
use serde::Serialize;
#[cfg(not(target_os = "android"))]
use tauri::Manager;
use tauri::State;

use super::{parse_profile_ref, parse_subscription_id};
use crate::state::AppState;

#[derive(Serialize)]
pub struct SubscriptionSourceView {
    content: String,
    extension: &'static str,
}

fn read_source(state: &AppState, id: &str) -> Result<SubscriptionSource, String> {
    let id = parse_subscription_id(id)?;
    let store = SubscriptionStore::new(state.data_dir.clone());
    if !store
        .load()
        .map_err(|e| e.to_string())?
        .iter()
        .any(|s| s.id == id)
    {
        return Err("订阅不存在".into());
    }
    store
        .load_source(id)
        .map_err(|e| format!("读取订阅原文失败: {e}"))?
        .ok_or_else(|| "尚未保存订阅原文，请先刷新订阅".into())
}

#[tauri::command]
pub fn subscription_source(
    state: State<'_, AppState>,
    id: String,
) -> Result<SubscriptionSourceView, String> {
    let source = read_source(&state, &id)?;
    Ok(SubscriptionSourceView {
        extension: source.extension(),
        content: source.content,
    })
}

/// Export the cached successful response, never a fresh request or synthesized config.
#[tauri::command]
pub async fn export_subscription_source(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<String, String> {
    let source = read_source(&state, &id)?;
    let filename = format!(
        "Lanpath-subscription-{}-{}.{}",
        parse_subscription_id(&id)?,
        uuid::Uuid::new_v4(),
        source.extension()
    );
    #[cfg(target_os = "android")]
    {
        let _ = &app;
        let handle = crate::core_bridge::vpn_plugin_handle()
            .ok_or_else(|| "VPN 插件未初始化，请重启应用后重试".to_string())?;
        handle
            .run_mobile_plugin_async::<String>(
                "exportSubscription",
                serde_json::json!({
                    "filename": filename, "content": source.content,
                }),
            )
            .await
            .map_err(crate::core_bridge::plugin_error)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let dir = app
            .path()
            .download_dir()
            .map_err(|e| e.to_string())?
            .join("Lanpath");
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建下载目录失败: {e}"))?;
        let path = dir.join(filename);
        std::fs::write(&path, source.content.as_bytes())
            .map_err(|e| format!("导出订阅失败: {e}"))?;
        Ok(path.to_string_lossy().into_owned())
    }
}

#[tauri::command]
pub fn set_subscription_profile(
    state: State<'_, AppState>,
    id: String,
    profile_id: Option<String>,
) -> Result<(), String> {
    let id = parse_subscription_id(&id)?;
    let profile_id = parse_profile_ref(&profile_id)?;
    if let Some(profile_id) = profile_id {
        let profiles = ProfileStoreV2::new(state.data_dir.clone())
            .load()
            .map_err(|e| e.to_string())?;
        if !profiles.iter().any(|p| p.id == profile_id) {
            return Err("覆写模板不存在，请重新选择".into());
        }
    }
    SubscriptionStore::new(state.data_dir.clone())
        .set_profile_id(id, profile_id)
        .map_err(|e| e.to_string())
}
