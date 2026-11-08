//! GPUI opts into unified resource storage by creating `app.toml`.
//! Other shells retain their legacy manifests and paths.
use lanpath_common::LanpathResult;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod subscriptions;
pub use subscriptions::{SUBSCRIPTIONS_FILE, load_subscriptions, persist_subscriptions};
pub(crate) use subscriptions::{SubscriptionDocument, subscriptions_text};
#[cfg(test)]
mod tests;

#[derive(Default, Serialize, Deserialize)]
pub struct ResourceSettings {
    #[serde(default)]
    remotes: Vec<crate::RemoteResource>,
}

impl ResourceSettings {
    pub fn new(remotes: Vec<crate::RemoteResource>) -> Self {
        Self { remotes }
    }
    pub fn remotes(&self) -> &[crate::RemoteResource] {
        &self.remotes
    }
    pub(crate) fn set_remotes(&mut self, remotes: Vec<crate::RemoteResource>) {
        self.remotes = remotes;
    }
}

pub fn unified(root: &Path) -> bool {
    crate::settings::app_settings_path(root).is_file()
}

/// Only reconstructible data belongs below `cache/`.
pub fn cache_path(root: &Path, legacy: &str) -> PathBuf {
    let relative = if unified(root) {
        match legacy {
            "remote_cache" => "cache/remotes",
            "remote_refresh.json" => "cache/remotes/refresh.json",
            "scripts" => "cache/scripts",
            "icons" => "cache/icons",
            "subscription_cache" => "cache/subscriptions",
            "profile_cache" => "cache/profiles",
            _ => legacy,
        }
    } else {
        legacy
    };
    root.join(relative)
}

/// Local imports contain irreplaceable source and must survive cache deletion.
pub(crate) fn imports_path(root: &Path) -> PathBuf {
    root.join(if unified(root) {
        "resources/imports"
    } else {
        "remote_cache"
    })
}

pub fn load(root: &Path) -> LanpathResult<ResourceSettings> {
    let section = crate::settings::load_app_section::<serde_json::Value>(root, "resources")?;
    if section
        .as_ref()
        .is_some_and(|section| section.get("subscriptions").is_some())
        || (section.is_none() && root.join("remotes.json").exists())
    {
        return Err(lanpath_common::LanpathError::Config(
            "旧资源清单尚未迁移，请重启 GPUI 完成自动迁移后重试".into(),
        ));
    }
    section
        .map(serde_json::from_value)
        .transpose()
        .map(|value| value.unwrap_or_default())
        .map_err(Into::into)
}
