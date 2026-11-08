use super::*;
use crate::{RemoteManager, RemoteResource, SubscriptionStore};

#[test]
fn gpui_sections_preserve_each_other_and_other_shells_keep_legacy_paths() {
    let legacy = tempfile::tempdir().unwrap();
    let remote = RemoteResource {
        name: "fixture".into(),
        url: "https://example.invalid/a.js".into(),
        ..Default::default()
    };
    RemoteManager::new(legacy.path().into())
        .save(std::slice::from_ref(&remote))
        .unwrap();
    let subs = SubscriptionStore::new(legacy.path().into());
    subs.add("fixture", "https://example.invalid/sub", true, None)
        .unwrap();
    assert!(legacy.path().join("remotes.json").exists());
    assert!(legacy.path().join("subscriptions.json").exists());
    assert_eq!(
        cache_path(legacy.path(), "remote_cache"),
        legacy.path().join("remote_cache")
    );
    let gpui = tempfile::tempdir().unwrap();
    crate::settings::ensure_app_settings(gpui.path()).unwrap();
    let manager = RemoteManager::new(gpui.path().into());
    manager.save(&[remote]).unwrap();
    let subs = SubscriptionStore::new(gpui.path().into());
    let sub = subs
        .add("fixture", "https://example.invalid/sub", true, None)
        .unwrap();
    manager.save(&manager.load().unwrap()).unwrap();
    assert_eq!(subs.load().unwrap()[0].id, sub.id);
    assert_eq!(manager.load().unwrap().len(), 1);
    assert!(!gpui.path().join("remotes.json").exists());
    assert!(!gpui.path().join("subscriptions.json").exists());
    assert_eq!(
        subs.cache_file(sub.id),
        gpui.path()
            .join(format!("cache/subscriptions/{}.json", sub.id))
    );
    std::fs::write(gpui.path().join("app.toml"), "[resources\n").unwrap();
    assert!(manager.load().is_err());
    assert!(subs.load().is_ok()); // An unrelated corrupt app section cannot destroy the standalone subscription document.
    assert!(manager.save(&[]).is_err());
}

#[cfg(feature = "mitm")]
#[test]
fn local_imports_activate_after_download_cache_is_removed() {
    let root = tempfile::tempdir().unwrap();
    crate::settings::ensure_app_settings(root.path()).unwrap();
    let manager = RemoteManager::new(root.path().into());
    let imported = crate::import::parse_import(
        "[URL Rewrite]\n^https://example.invalid/ - reject\n[MITM]\nhostname=example.invalid",
        crate::ScriptDialect::Surge,
    )
    .unwrap();
    manager.merge_imported(&imported).unwrap();
    std::fs::create_dir_all(root.path().join("cache/remotes")).unwrap();
    std::fs::remove_dir_all(root.path().join("cache")).unwrap();
    assert_eq!(manager.load_cached().unwrap().rewrites.len(), 1);
    assert_eq!(
        manager.load_cached().unwrap().hostnames,
        ["example.invalid"]
    );
    assert!(
        root.path()
            .join("resources/imports/local_imports.json")
            .is_file()
    );
}

#[test]
fn concurrent_section_writes_preserve_resource_manifests_and_appearance() {
    let root = tempfile::tempdir().unwrap();
    crate::settings::ensure_app_settings(root.path()).unwrap();
    std::thread::scope(|scope| {
        let path = root.path();
        scope.spawn(move || {
            for count in 0..20 {
                RemoteManager::new(path.into())
                    .save(&[RemoteResource {
                        name: format!("r{count}"),
                        ..Default::default()
                    }])
                    .unwrap();
            }
        });
        scope.spawn(move || {
            for count in 0..20 {
                crate::settings::persist_app_section(
                    path,
                    "appearance",
                    &serde_json::json!({"scale_percent": count}),
                )
                .unwrap();
            }
        });
        scope.spawn(move || {
            SubscriptionStore::new(path.into())
                .add("sub", "https://example.invalid", true, None)
                .unwrap();
        });
    });
    assert_eq!(load(root.path()).unwrap().remotes()[0].name, "r19");
    assert_eq!(load_subscriptions(root.path()).unwrap().len(), 1);
    assert_eq!(
        crate::settings::load_app_section::<serde_json::Value>(root.path(), "appearance")
            .unwrap()
            .unwrap()["scale_percent"],
        19
    );
}

#[test]
fn interim_app_subscriptions_cannot_be_silently_overwritten_by_business_writes() {
    let root = tempfile::tempdir().unwrap();
    crate::settings::ensure_app_settings(root.path()).unwrap();
    crate::settings::persist_app_section(
        root.path(),
        "resources",
        &serde_json::json!({"remotes": [], "subscriptions": []}),
    )
    .unwrap();
    let original = std::fs::read(root.path().join("app.toml")).unwrap();
    assert!(SubscriptionStore::new(root.path().into()).load().is_err());
    assert!(RemoteManager::new(root.path().into()).save(&[]).is_err());
    assert_eq!(
        std::fs::read(root.path().join("app.toml")).unwrap(),
        original
    );
}
