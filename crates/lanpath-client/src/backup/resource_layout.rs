//! Portable backups carry logical manifests; target-local app sections survive restore.
use super::{LanpathResult, invalid};
use crate::resource_storage::{self, ResourceSettings};
use std::{collections::BTreeMap, path::Path};
type Files = BTreeMap<String, Vec<u8>>;

pub(super) fn capture_manifests(dir: &Path, files: &mut Files) -> LanpathResult<()> {
    if resource_storage::unified(dir) {
        let settings = resource_storage::load(dir)?;
        files.insert(
            "remotes.json".into(),
            serde_json::to_vec(settings.remotes())?,
        );
        files.insert(
            "subscriptions.json".into(),
            serde_json::to_vec(&resource_storage::load_subscriptions(dir)?)?,
        );
        files.remove(".cache/remotes/refresh.json"); // runtime age is not portable configuration
    }
    Ok(())
}

pub(super) fn restore(dir: &Path, files: &mut Files, document: String) -> LanpathResult<String> {
    let resources = ResourceSettings::new(read(files, "remotes.json")?);
    let subscriptions = read(files, "subscriptions.json")?;
    files.insert(
        "subscriptions.toml".into(),
        resource_storage::subscriptions_text(subscriptions)?.into_bytes(),
    );
    let document = crate::settings::merge_app_section_text(&document, "resources", &resources)?;
    files.remove("remotes.json");
    files.remove("subscriptions.json");
    remap(files, |name| {
        for legacy in ["remote_cache", "subscription_cache", "profile_cache"] {
            if let Some(suffix) = name.strip_prefix(&format!("{legacy}/")) {
                if legacy == "remote_cache" && is_local(suffix) {
                    return format!("resources/imports/{suffix}");
                }
                let path = resource_storage::cache_path(dir, legacy);
                let prefix = path
                    .strip_prefix(dir)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                return format!("{prefix}/{suffix}");
            }
        }
        name.to_owned()
    })?;
    Ok(document)
}

pub(super) fn restore_legacy(files: &mut Files) -> LanpathResult<()> {
    remap(files, |name| {
        for (current, legacy) in [
            (".cache/remotes/", "remote_cache/"),
            (".cache/subscriptions/", "subscription_cache/"),
            (".cache/profiles/", "profile_cache/"),
            (".cache/scripts/", "scripts/"),
            ("resources/imports/", "remote_cache/"),
        ] {
            if let Some(suffix) = name.strip_prefix(current) {
                return format!("{legacy}{suffix}");
            }
        }
        name.to_owned()
    })
}
fn is_local(suffix: &str) -> bool {
    suffix.starts_with("local/") || matches!(suffix, "local_imports.json" | "imported.json")
}
fn read<T: serde::de::DeserializeOwned + Default>(files: &Files, name: &str) -> LanpathResult<T> {
    files
        .get(name)
        .map(|bytes| serde_json::from_slice(bytes).map_err(Into::into))
        .unwrap_or_else(|| Ok(T::default()))
}
fn remap(files: &mut Files, rename: impl Fn(&str) -> String) -> LanpathResult<()> {
    let mut mapped = Files::new();
    for (name, bytes) in files.iter() {
        let name = rename(name);
        if let Some(previous) = mapped.insert(name, bytes.clone())
            && previous != *bytes
        {
            return Err(invalid("备份的新旧资源路径存在不同内容，恢复已停止"));
        }
    }
    *files = mapped;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{ClientConfig, RemoteManager, RemoteResource, SubscriptionStore};
    fn configured() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        crate::settings::ensure_app_settings(root.path()).unwrap();
        ClientConfig::new(root.path().into(), "", "", Default::default())
            .save()
            .unwrap();
        root
    }
    #[test]
    fn gpui_backup_roundtrips_manifests_imports_and_cache_with_local_appearance() {
        let source = configured();
        let manager = RemoteManager::new(source.path().into());
        manager
            .save(&[RemoteResource {
                name: "module".into(),
                ..Default::default()
            }])
            .unwrap();
        let sub = SubscriptionStore::new(source.path().into())
            .add("fixture", "https://example.invalid", true, None)
            .unwrap();
        for (name, bytes) in [
            (".cache/remotes/a.json", "remote"),
            (".cache/remotes/refresh.json", "runtime"),
            ("resources/imports/local/local.json", "local source"),
            (".cache/icons/icon.png", "icon"),
        ] {
            let path = source.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        let snapshot = crate::backup::capture(source.path(), None).unwrap();
        assert_eq!(snapshot.summary.version, 3);
        assert_eq!(snapshot.summary.subscriptions, 1);
        assert!(!snapshot.files.contains_key(".cache/remotes/refresh.json"));
        assert!(!snapshot.files.contains_key(".cache/icons/icon.png"));
        let encrypted = crate::backup::encode(&snapshot, "roundtrip-password").unwrap();
        let snapshot = crate::backup::decode(&encrypted, "roundtrip-password").unwrap();
        let target = configured();
        crate::settings::persist_app_section(
            target.path(),
            "appearance",
            &serde_json::json!({"theme": "dark"}),
        )
        .unwrap();
        crate::backup::prepare_restore(target.path(), snapshot, "roundtrip-password", None)
            .unwrap();
        crate::backup::validate_pending(target.path()).unwrap();
        crate::backup::apply_pending(target.path()).unwrap();
        assert_eq!(
            SubscriptionStore::new(target.path().into()).load().unwrap()[0].id,
            sub.id
        );
        assert_eq!(
            RemoteManager::new(target.path().into()).load().unwrap()[0].name,
            "module"
        );
        assert!(
            target
                .path()
                .join("resources/imports/local/local.json")
                .is_file()
        );
        assert!(target.path().join(".cache/remotes/a.json").is_file());
        assert!(!target.path().join("remotes.json").exists());
        assert_eq!(
            crate::settings::load_app_section::<serde_json::Value>(target.path(), "appearance")
                .unwrap()
                .unwrap()["theme"],
            "dark"
        );
    }
    #[test]
    fn legacy_backup_maps_caches_into_gpui_without_copying_legacy_manifests() {
        let source = tempfile::tempdir().unwrap();
        ClientConfig::new(source.path().into(), "", "", Default::default())
            .save()
            .unwrap();
        SubscriptionStore::new(source.path().into())
            .add("old", "https://example.invalid", true, None)
            .unwrap();
        for name in [
            "remote_cache/local_imports.json",
            "remote_cache/local/id.json",
            "subscription_cache/a.json",
        ] {
            let path = source.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "[]").unwrap();
        }
        let mut snapshot = crate::backup::capture(source.path(), None).unwrap();
        let target = configured();
        super::super::snapshot::relocate(&mut snapshot, target.path()).unwrap();
        assert!(
            snapshot
                .files
                .contains_key("resources/imports/local/id.json")
        );
        assert!(snapshot.files.contains_key(".cache/subscriptions/a.json"));
        assert!(!snapshot.files.contains_key("subscriptions.json"));
        super::super::snapshot::validate(&mut snapshot).unwrap();
        assert_eq!(snapshot.summary.subscriptions, 1);
    }
}
