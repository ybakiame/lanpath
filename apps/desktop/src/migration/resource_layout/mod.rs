//! GPUI-only conversion. The runner backs up inputs before any publication.
use super::SettingsMigration;
use lanpath_client::{
    RemoteKind,
    resource_storage::{self, ResourceSettings},
    settings,
};
use lanpath_common::{LanpathError, LanpathResult};
use std::path::Path;

pub(super) struct ResourceLayout;
const LEGACY: &[&str] = &[
    "remotes.json",
    "subscriptions.json",
    "remote_refresh.json",
    "remote_cache",
    "subscription_cache",
    "profile_cache",
    "icons",
];

impl SettingsMigration for ResourceLayout {
    fn id(&self) -> &'static str {
        "resource-layout-v1"
    }
    fn is_required(&self, root: &Path) -> LanpathResult<bool> {
        let resources = settings::load_app_section::<serde_json::Value>(root, "resources")?;
        Ok(LEGACY.iter().any(|name| root.join(name).exists())
            || (settings::app_settings_path(root).exists()
                && (resources
                    .as_ref()
                    .is_none_or(|section| section.get("subscriptions").is_some())
                    || !root.join(resource_storage::SUBSCRIPTIONS_FILE).exists())))
    }

    fn rollback_files(&self) -> &'static [&'static str] {
        &["app.toml", "subscriptions.toml"]
    }
    fn legacy_files(&self) -> &'static [&'static str] {
        LEGACY
    }
    fn backup_files(&self) -> &'static [&'static str] {
        &[
            "app.toml",
            "subscriptions.toml",
            "remotes.json",
            "subscriptions.json",
            "remote_refresh.json",
            "remote_cache",
            "subscription_cache",
            "profile_cache",
            "icons",
            "scripts",
        ]
    }
    fn output_files(&self) -> &'static [&'static str] {
        &["app.toml", "subscriptions.toml", "cache", "resources"]
    }
    fn migrate(&self, root: &Path) -> LanpathResult<()> {
        settings::ensure_app_settings(root)?;
        let old_section = settings::load_app_section::<serde_json::Value>(root, "resources")?;
        if old_section
            .as_ref()
            .is_some_and(|section| !section.is_object())
        {
            return Err(LanpathError::Config(
                "资源清单结构无效，自动迁移已停止，原配置保留".into(),
            ));
        }
        let remotes = match old_section.as_ref().and_then(|value| value.get("remotes")) {
            Some(value) => serde_json::from_value(value.clone())?,
            None => read_manifest(root, "remotes.json")?,
        };
        let subscriptions = if root.join(resource_storage::SUBSCRIPTIONS_FILE).exists() {
            resource_storage::load_subscriptions(root)?
        } else if let Some(value) = old_section
            .as_ref()
            .and_then(|value| value.get("subscriptions"))
        {
            serde_json::from_value(value.clone())?
        } else {
            read_manifest(root, "subscriptions.json")?
        };
        let resources = ResourceSettings::new(remotes);
        validate_manifests(&resources, &subscriptions)?;
        for name in ["subscription_cache", "profile_cache", "icons"] {
            super::copy_entry(&root.join(name), &resource_storage::cache_path(root, name))?;
        }
        let imports = root.join("resources/imports");
        let old_cache = root.join("remote_cache");
        if old_cache.exists() {
            for entry in std::fs::read_dir(&old_cache)? {
                let entry = entry?;
                let name = entry.file_name();
                let base = if matches!(
                    name.to_str(),
                    Some("local" | "local_imports.json" | "imported.json")
                ) {
                    imports.clone()
                } else {
                    resource_storage::cache_path(root, "remote_cache")
                };
                super::copy_entry(&entry.path(), &base.join(name))?;
            }
        }
        super::copy_entry(
            &root.join("remote_refresh.json"),
            &resource_storage::cache_path(root, "remote_refresh.json"),
        )?;
        // `scripts/` can also contain user-authored files referenced by path.
        // Copy known remote downloads only; preserve that durable source directory.
        for remote in resources.remotes() {
            if remote.kind == RemoteKind::Script {
                let destination =
                    lanpath_client::RemoteManager::new(root.to_owned()).script_file(&remote.name);
                if let Some(name) = destination.file_name() {
                    super::copy_entry(&root.join("scripts").join(name), &destination)?;
                }
            }
        }
        resource_storage::persist_subscriptions(root, &subscriptions)?;
        settings::persist_app_section(root, "resources", &resources)
    }
    fn validate(&self, root: &Path) -> LanpathResult<()> {
        validate_manifests(
            &resource_storage::load(root)?,
            &resource_storage::load_subscriptions(root)?,
        )?;
        // The runner byte-verifies all copies before removing legacy inputs.
        Ok(())
    }
}
fn read_manifest<T: serde::de::DeserializeOwned + Default>(
    root: &Path,
    name: &str,
) -> LanpathResult<T> {
    match std::fs::read(root.join(name)) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(error) => Err(error.into()),
    }
}
fn validate_manifests(
    resources: &ResourceSettings,
    subscriptions: &[lanpath_client::Subscription],
) -> LanpathResult<()> {
    let names: std::collections::HashSet<_> = resources.remotes().iter().map(|r| &r.name).collect();
    let ids: std::collections::HashSet<_> = subscriptions.iter().map(|s| s.id).collect();
    if names.len() != resources.remotes().len() || ids.len() != subscriptions.len() {
        return Err(LanpathError::Config(
            "资源或订阅存在重复标识，迁移已停止；原数据保留".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
