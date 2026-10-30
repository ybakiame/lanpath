//! Remote resource commands: script / snippet subscription management.

use lanpath_client::{RemoteManager, RemoteResource, RemoteResourceService};
use lanpath_script::ScriptDialect;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::desktop::commands::{remote_kind_str, script_dialect_str};
use crate::desktop::state::AppState;

/// External view of a remote resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteResourceView {
    pub name: String,
    pub url: String,
    /// Resource kind: `Script` / `Snippet`.
    pub kind: String,
    /// Script dialect: `Surge` / `Loon`.
    pub dialect: String,
    /// Optional description (`None` = not configured).
    pub description: Option<String>,
    pub update_interval_secs: u64,
    pub enabled: bool,
    /// User-configured argument values `(key, value)`.
    pub argument_values: Vec<(String, String)>,
    /// Icon URL (optional; pre-filled from sniff result).
    pub icon: Option<String>,
    /// Module argument declarations (`#!arguments=` / Loon `[Argument]`).
    #[serde(default)]
    pub arguments: Vec<ArgSpecView>,
}

impl RemoteResourceView {
    pub(crate) fn from_remote(remote: &RemoteResource) -> Self {
        Self {
            name: remote.name.clone(),
            url: remote.url.clone(),
            kind: serde_json::to_value(remote.kind)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default(),
            dialect: serde_json::to_value(remote.dialect)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default(),
            description: remote.description.clone(),
            update_interval_secs: remote.update_interval_secs,
            enabled: remote.enabled,
            argument_values: remote.argument_values.clone(),
            icon: remote.icon.clone(),
            arguments: remote.arguments.iter().map(ArgSpecView::from_arg).collect(),
        }
    }

    pub(crate) fn into_remote(self) -> Result<RemoteResource, String> {
        let arguments: Vec<lanpath_client::ArgSpec> = self
            .arguments
            .into_iter()
            .map(ArgSpecView::into_arg)
            .collect();
        let value = serde_json::json!({
            "name": self.name,
            "url": self.url,
            "kind": self.kind,
            "dialect": self.dialect,
            "description": self.description,
            "update_interval_secs": self.update_interval_secs,
            "enabled": self.enabled,
            "argument_values": self.argument_values,
            "icon": self.icon,
            "arguments": arguments,
        });
        serde_json::from_value::<RemoteResource>(value).map_err(|e| e.to_string())
    }
}

/// Fetch report view for `fetch_remotes`.
#[derive(Debug, Clone, Serialize)]
pub struct FetchReportView {
    pub fetched: usize,
    pub scripts: usize,
    pub rewrites: usize,
    pub tasks: usize,
    pub hostnames: usize,
    pub warnings: Vec<String>,
}

/// Module argument declaration view.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArgSpecView {
    pub key: String,
    pub default_value: String,
    pub description: Option<String>,
    /// Control type: `Input` / `Select`.
    pub kind: String,
    /// Options for `Select` control.
    pub options: Vec<String>,
    /// Parameter group tag (null when ungrouped).
    pub tag: Option<String>,
}

impl ArgSpecView {
    pub(crate) fn from_arg(arg: &lanpath_client::ArgSpec) -> Self {
        Self {
            key: arg.key.clone(),
            default_value: arg.default_value.clone(),
            description: arg.description.clone(),
            kind: match arg.kind {
                lanpath_client::ArgKind::Select => "Select".to_string(),
                lanpath_client::ArgKind::Input => "Input".to_string(),
            },
            options: arg.options.clone(),
            tag: arg.tag.clone(),
        }
    }

    pub(crate) fn into_arg(self) -> lanpath_client::ArgSpec {
        lanpath_client::ArgSpec {
            key: self.key,
            default_value: self.default_value,
            description: self.description,
            kind: match self.kind.as_str() {
                "Select" => lanpath_client::ArgKind::Select,
                _ => lanpath_client::ArgKind::Input,
            },
            options: self.options,
            tag: self.tag,
        }
    }
}

/// Config header `#!key=value` metadata view.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigMetaView {
    pub name: Option<String>,
    pub desc: Option<String>,
    pub author: Option<String>,
    pub icon: Option<String>,
    pub date: Option<String>,
    pub category: Option<String>,
    pub open_url: Option<String>,
    pub arguments: Vec<ArgSpecView>,
}

impl ConfigMetaView {
    pub(crate) fn from_meta(meta: &lanpath_client::ConfigMeta) -> Self {
        Self {
            name: meta.name.clone(),
            desc: meta.desc.clone(),
            author: meta.author.clone(),
            icon: meta.icon.clone(),
            date: meta.date.clone(),
            category: meta.category.clone(),
            open_url: meta.open_url.clone(),
            arguments: meta
                .arguments
                .iter()
                .map(|arg| ArgSpecView {
                    key: arg.key.clone(),
                    default_value: arg.default_value.clone(),
                    description: arg.description.clone(),
                    kind: match arg.kind {
                        lanpath_client::ArgKind::Select => "Select".to_string(),
                        lanpath_client::ArgKind::Input => "Input".to_string(),
                    },
                    options: arg.options.clone(),
                    tag: arg.tag.clone(),
                })
                .collect(),
        }
    }
}

/// Config import summary view.
#[derive(Debug, Clone, Serialize)]
pub struct ImportSummaryView {
    pub rewrites: usize,
    pub scripts: usize,
    pub tasks: usize,
    pub hostnames: usize,
    pub warnings: Vec<String>,
    pub meta: ConfigMetaView,
}

/// Detect remote result view.
#[derive(Debug, Clone, Serialize, Default)]
pub struct DetectRemoteView {
    pub kind: Option<String>,
    pub dialect: Option<String>,
    pub meta: Option<ConfigMetaView>,
}

/// List all remote resources (`remotes.json`).
#[tauri::command]
pub async fn list_remotes(state: State<'_, AppState>) -> Result<Vec<RemoteResourceView>, String> {
    let remotes = RemoteResourceService::new(state.data_dir.clone())
        .load()
        .map_err(|e| format!("读取远程资源失败: {e}"))?;
    Ok(remotes
        .iter()
        .map(RemoteResourceView::from_remote)
        .collect())
}

/// Add a remote resource; errors on duplicate name.
#[tauri::command]
pub async fn add_remote(
    state: State<'_, AppState>,
    remote: RemoteResourceView,
) -> Result<(), String> {
    let remote = remote.into_remote()?;
    RemoteResourceService::new(state.data_dir.clone())
        .add(remote)
        .await
        .map_err(|e| format!("保存远程资源失败: {e}"))?;
    Ok(())
}

/// Remove a remote resource; errors if not found.
#[tauri::command]
pub async fn remove_remote(state: State<'_, AppState>, name: String) -> Result<(), String> {
    RemoteResourceService::new(state.data_dir.clone())
        .remove(&name)
        .await
        .map_err(|e| format!("保存远程资源失败: {e}"))
}

/// Update a remote resource by name (full replacement).
#[tauri::command]
pub async fn update_remote(
    state: State<'_, AppState>,
    resource: RemoteResourceView,
) -> Result<(), String> {
    let name = resource.name.clone();
    let remote = resource.into_remote()?;
    RemoteResourceService::new(state.data_dir.clone())
        .update(&name, remote)
        .await
        .map_err(|e| format!("更新远程资源失败: {e}"))?;
    Ok(())
}

/// Read cached local icon for a remote resource, returning `data:{mime};base64,...`.
#[tauri::command]
pub async fn get_remote_icon(
    state: State<'_, AppState>,
    name: String,
) -> Result<Option<String>, String> {
    use base64::Engine as _;
    let manager = RemoteManager::new(state.data_dir.clone());
    let Some(path) = manager.icon_file(&name) else {
        return Ok(None);
    };
    let bytes = std::fs::read(&path).map_err(|e| format!("读取图标缓存失败: {e}"))?;
    let mime = match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(Some(format!("data:{mime};base64,{encoded}")))
}

/// Sniff a remote resource URL: suffix-based type/dialect detection;
/// for Snippet with accessible URL, fetch and parse config header metadata.
#[tauri::command]
pub async fn detect_remote(
    state: State<'_, AppState>,
    url: String,
) -> Result<DetectRemoteView, String> {
    let detected = RemoteResourceService::new(state.data_dir.clone())
        .detect(&url)
        .await
        .map_err(|e| format!("嗅探远程资源失败: {e}"))?;
    Ok(DetectRemoteView {
        kind: detected.kind.map(|kind| remote_kind_str(kind).to_string()),
        dialect: detected
            .dialect
            .map(|dialect| script_dialect_str(dialect).to_string()),
        meta: detected.meta.as_ref().map(ConfigMetaView::from_meta),
    })
}

/// Fetch all enabled remote resources (no system proxy, 30s timeout).
#[tauri::command]
pub async fn fetch_remotes(state: State<'_, AppState>) -> Result<FetchReportView, String> {
    let mut report = RemoteResourceService::new(state.data_dir.clone())
        .fetch_all()
        .await
        .map_err(|e| format!("读取远程资源失败: {e}"))?;
    reload_running_scheduler(&state, &mut report.warnings).await;
    Ok(FetchReportView {
        fetched: report.fetched,
        scripts: report.scripts,
        rewrites: report.rewrites,
        tasks: report.tasks,
        hostnames: report.hostnames,
        warnings: report.warnings,
    })
}

/// Test GitHub proxy connectivity via real fetch pipeline.
#[tauri::command]
pub async fn test_github_proxy(state: State<'_, AppState>) -> Result<String, String> {
    let config = lanpath_client::ClientConfig::load_for_requests(&state.data_dir)
        .map_err(|error| error.to_string())?;
    let elapsed_ms = lanpath_client::test_github_connectivity(&config)
        .await
        .map_err(|error| error.to_string())?;
    Ok(format!("OK（{elapsed_ms} ms）"))
}

/// Import Surge / Loon config snippet: parse -> fetch script sources -> merge into local cache.
#[tauri::command]
pub async fn import_config(
    state: State<'_, AppState>,
    content: String,
    dialect: String,
) -> Result<ImportSummaryView, String> {
    let script_dialect = match dialect.as_str() {
        "surge" => ScriptDialect::Surge,
        "loon" | "quantumultx" => ScriptDialect::Loon,
        other => {
            return Err(format!("未知方言 '{other}'（可选: surge / loon）"));
        }
    };
    let mut summary = RemoteResourceService::new(state.data_dir.clone())
        .import_content(&content, script_dialect)
        .await
        .map_err(|e| format!("导入配置失败: {e}"))?;
    reload_running_scheduler(&state, &mut summary.warnings).await;
    Ok(ImportSummaryView {
        rewrites: summary.rewrites,
        scripts: summary.scripts,
        tasks: summary.tasks,
        hostnames: summary.hostnames,
        warnings: summary.warnings,
        meta: ConfigMetaView::from_meta(&summary.meta),
    })
}

async fn reload_running_scheduler(state: &State<'_, AppState>, warnings: &mut Vec<String>) {
    let mut client = state.client.lock().await;
    if let Some(client) = client.as_mut() {
        if let Err(error) = client.reload_scheduler_from_cache().await {
            warnings.push(format!("任务调度器重新载入失败: {error}"));
        }
    }
}
