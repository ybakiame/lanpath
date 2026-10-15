//! Subscription management commands.

use std::path::PathBuf;

use lanpath_client::{
    CachedSubscriptionContent, ClientConfig, Subscription, SubscriptionStore,
    fetch_subscription_with_ua,
};
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::commands::{parse_profile_id, sub_format_str};
use crate::state::AppState;

/// Subscription user info view.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SubscriptionUserInfoView {
    pub upload: Option<u64>,
    pub download: Option<u64>,
    pub total: Option<u64>,
    pub expire: Option<u64>,
}

impl SubscriptionUserInfoView {
    pub(crate) fn from_info(info: &lanpath_client::SubscriptionInfo) -> Self {
        Self {
            upload: info.upload,
            download: info.download,
            total: info.total,
            expire: info.expire,
        }
    }
}

/// Subscription view.
#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub profile_id: Option<String>,
    pub userinfo: Option<SubscriptionUserInfoView>,
    pub node_count: u64,
    pub error: Option<String>,
    pub format: Option<String>,
    pub user_agent: Option<String>,
}

impl SubscriptionView {
    pub(crate) fn from_sub(sub: &Subscription) -> Self {
        Self {
            id: sub.id.to_string(),
            name: sub.name.clone(),
            url: sub.url.clone(),
            enabled: sub.enabled,
            profile_id: sub.profile_id.map(|v| v.to_string()),
            userinfo: sub
                .userinfo
                .as_ref()
                .map(SubscriptionUserInfoView::from_info),
            node_count: sub.node_count,
            error: sub.error.clone(),
            format: sub.format.map(sub_format_str).map(str::to_string),
            user_agent: sub.user_agent.clone(),
        }
    }
}

/// Input for adding a subscription.
#[derive(Debug, Deserialize)]
pub struct AddSubscriptionInput {
    pub name: String,
    pub url: String,
    pub user_agent: Option<String>,
    #[serde(default)]
    pub profile_id: Option<String>,
}

/// Parse subscription ID string to Uuid.
pub(crate) fn parse_subscription_id(id: &str) -> Result<Uuid, String> {
    Uuid::parse_str(id).map_err(|e| format!("无效的订阅 ID: {e}"))
}

/// Parse profile reference: None / empty = unlinked; non-empty must be valid Uuid.
pub(crate) fn parse_profile_ref(profile_id: &Option<String>) -> Result<Option<Uuid>, String> {
    match profile_id.as_deref() {
        Some(s) if !s.trim().is_empty() => Ok(Some(parse_profile_id(s.trim())?)),
        _ => Ok(None),
    }
}

/// Write fetch result to local content cache (best-effort).
pub(crate) fn cache_fetch_result(
    store: &SubscriptionStore,
    id: Uuid,
    fetch: &lanpath_client::FetchResult,
) {
    let source = lanpath_client::SubscriptionSource {
        format: fetch.format,
        content: fetch.source.clone(),
    };
    if let Err(e) = store.write_source(id, &source) {
        tracing::warn!(%id, error = %e, "写入订阅原文失败");
    }
    let cached = CachedSubscriptionContent {
        format: fetch.format,
        singbox_nodes: fetch.singbox_nodes.clone(),
    };
    if let Err(e) = store.write_cached_content(id, &cached) {
        tracing::warn!(id = %id, error = %e, "写入订阅内容缓存失败");
    }
}

/// Assemble a sing-box subscription config (`outbounds` array) from fetched nodes.
pub(crate) fn sub_content_from_nodes(singbox_nodes: &[serde_json::Value]) -> serde_json::Value {
    serde_json::json!({
        "outbounds": singbox_nodes,
    })
}

/// Apply fetch result to subscription (updates userinfo / node count, clears error on success).
pub(crate) async fn apply_fetch(store: &SubscriptionStore, sub: &mut Subscription, url: &str) {
    match fetch_subscription_with_ua(url, sub.user_agent.as_deref()).await {
        Ok(result) => {
            // A slow response must not resurrect cache after URL/UA edits or deletion.
            let current = store
                .load()
                .ok()
                .and_then(|subs| subs.into_iter().find(|s| s.id == sub.id));
            if !current.is_some_and(|s| s.url == sub.url && s.user_agent == sub.user_agent) {
                return;
            }
            sub.node_count = result.singbox_nodes.len() as u64;
            sub.format = Some(result.format);
            sub.error = None;
            cache_fetch_result(store, sub.id, &result);
            sub.userinfo = result.userinfo;
        }
        Err(e) => {
            sub.error = Some(format!("拉取失败: {e}"));
        }
    }
}

/// Write updated subscription back to store.
pub(crate) fn write_subscription(
    store: &SubscriptionStore,
    sub: &Subscription,
) -> Result<(), String> {
    let mut subs = store.load().map_err(|e| format!("读取订阅失败: {e}"))?;
    if let Some(existing) = subs.iter_mut().find(|s| s.id == sub.id)
        && existing.url == sub.url
        && existing.user_agent == sub.user_agent
    {
        existing.userinfo = sub.userinfo.clone();
        existing.node_count = sub.node_count;
        existing.format = sub.format;
        existing.error = sub.error.clone();
    }
    store.save(&subs).map_err(|e| format!("保存订阅失败: {e}"))
}

/// List all subscriptions.
#[tauri::command]
pub async fn list_subscriptions(
    state: State<'_, AppState>,
) -> Result<Vec<SubscriptionView>, String> {
    let store = SubscriptionStore::new(state.data_dir.clone());
    let subs = store.load().map_err(|e| format!("读取订阅失败: {e}"))?;
    Ok(subs.iter().map(SubscriptionView::from_sub).collect())
}

/// Add subscription: validate URL -> persist -> immediately fetch once.
#[tauri::command]
pub async fn add_subscription(
    state: State<'_, AppState>,
    input: AddSubscriptionInput,
) -> Result<SubscriptionView, String> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("名称不能为空".to_string());
    }
    let url = input.url.trim().to_string();
    lanpath_client::validate_subscription_url(&url)
        .map_err(|e| format!("订阅 URL 校验失败: {e}"))?;
    let ua = input
        .user_agent
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let profile_id = parse_profile_ref(&input.profile_id)?;

    let store = SubscriptionStore::new(state.data_dir.clone());
    let mut sub = store
        .add(&name, &url, true, ua)
        .map_err(|e| format!("保存订阅失败: {e}"))?;
    store
        .set_profile_id(sub.id, profile_id)
        .map_err(|e| e.to_string())?;
    sub.profile_id = profile_id;
    apply_fetch(&store, &mut sub, &url).await;
    write_subscription(&store, &sub)?;
    Ok(SubscriptionView::from_sub(&sub))
}

/// Refresh subscription: re-fetch to update userinfo / node count.
#[tauri::command]
pub async fn refresh_subscription(
    state: State<'_, AppState>,
    id: String,
) -> Result<SubscriptionView, String> {
    let id = parse_subscription_id(&id)?;
    let store = SubscriptionStore::new(state.data_dir.clone());
    let mut subs = store.load().map_err(|e| format!("读取订阅失败: {e}"))?;
    let idx = subs
        .iter()
        .position(|s| s.id == id)
        .ok_or_else(|| "订阅不存在".to_string())?;
    let url = subs[idx].url.clone();
    apply_fetch(&store, &mut subs[idx], &url).await;
    write_subscription(&store, &subs[idx])?;
    let updated = store.load().map_err(|e| e.to_string())?;
    updated
        .iter()
        .find(|sub| sub.id == id)
        .map(SubscriptionView::from_sub)
        .ok_or_else(|| "订阅已删除".to_string())
}

/// Remove subscription; silently returns if not found.
#[tauri::command]
pub async fn remove_subscription(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let id = parse_subscription_id(&id)?;
    let store = SubscriptionStore::new(state.data_dir.clone());
    store.remove(id).map_err(|e| format!("删除订阅失败: {e}"))
}

/// Toggle subscription enabled state.
#[tauri::command]
pub async fn set_subscription_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    let id = parse_subscription_id(&id)?;
    set_subscription_enabled_impl(&state.data_dir, id, enabled)
}

/// Implementation of `set_subscription_enabled`.
pub(crate) fn set_subscription_enabled_impl(
    data_dir: &std::path::Path,
    id: Uuid,
    enabled: bool,
) -> Result<(), String> {
    let store = SubscriptionStore::new(data_dir.to_path_buf());
    store
        .set_enabled(id, enabled)
        .map_err(|e| format!("保存订阅失败: {e}"))?;
    if !enabled
        && let Ok(mut config) = ClientConfig::load(data_dir)
        && config.active_subscription_id == Some(id)
    {
        config.active_subscription_id = None;
        config.save().map_err(|e| format!("保存配置失败: {e}"))?;
    }
    Ok(())
}

/// Set active subscription (home page selection).
#[tauri::command]
pub async fn set_active_subscription(
    state: State<'_, AppState>,
    id: Option<String>,
) -> Result<(), String> {
    set_active_subscription_impl(&state.data_dir, id)
}

/// Implementation of `set_active_subscription`.
pub(crate) fn set_active_subscription_impl(
    data_dir: &std::path::Path,
    id: Option<String>,
) -> Result<(), String> {
    let mut config = match ClientConfig::load(data_dir) {
        Ok(cfg) => cfg,
        Err(_) => ClientConfig::new(
            data_dir.to_path_buf(),
            String::new(),
            String::new(),
            PathBuf::new(),
        ),
    };
    match id {
        Some(id) => {
            let id = parse_subscription_id(&id)?;
            let store = SubscriptionStore::new(data_dir.to_path_buf());
            let subs = store.load().map_err(|e| format!("读取订阅失败: {e}"))?;
            let sub = subs
                .iter()
                .find(|s| s.id == id)
                .ok_or_else(|| "所选订阅不存在".to_string())?;
            if !sub.enabled {
                return Err("所选订阅已停用，请先在订阅页启用".to_string());
            }
            config.active_subscription_id = Some(id);
        }
        None => {
            config.active_subscription_id = None;
        }
    }
    config.save().map_err(|e| format!("保存配置失败: {e}"))
}

/// Update subscription name / url / user_agent / profile link.
#[tauri::command]
pub async fn update_subscription(
    state: State<'_, AppState>,
    id: String,
    name: String,
    url: String,
    user_agent: Option<String>,
    profile_id: Option<String>,
) -> Result<SubscriptionView, String> {
    let id = parse_subscription_id(&id)?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("名称不能为空".to_string());
    }
    let url = lanpath_client::normalize_resource_url(url.trim());
    lanpath_client::validate_subscription_url(&url)
        .map_err(|e| format!("订阅 URL 校验失败: {e}"))?;
    let ua = user_agent
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let profile_id = parse_profile_ref(&profile_id)?;

    let store = SubscriptionStore::new(state.data_dir.clone());
    store
        .update(id, &name, &url, ua)
        .map_err(|e| format!("更新订阅失败: {e}"))?;
    store
        .set_profile_id(id, profile_id)
        .map_err(|e| format!("更新订阅失败: {e}"))?;
    let subs = store.load().map_err(|e| format!("读取订阅失败: {e}"))?;
    let sub = subs
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| "订阅不存在".to_string())?;
    Ok(SubscriptionView::from_sub(sub))
}

#[cfg(test)]
#[path = "subscription_tests.rs"]
mod tests;
