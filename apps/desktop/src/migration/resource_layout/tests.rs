use super::*;
use lanpath_client::{RemoteManager, RemoteResource, SubscriptionStore};

fn legacy(root: &Path) {
    settings::ensure_app_settings(root).unwrap();
    // Managers use app.toml when present. Seed old manifests explicitly.
    let remote = RemoteResource {
        name: "fixture".into(),
        url: "https://example.invalid/a.js".into(),
        ..Default::default()
    };
    std::fs::write(
        root.join("remotes.json"),
        serde_json::to_vec(&vec![remote]).unwrap(),
    )
    .unwrap();
    let old = tempfile::tempdir().unwrap();
    let subscription = SubscriptionStore::new(old.path().into())
        .add("fixture sub", "https://example.invalid/sub", false, None)
        .unwrap();
    std::fs::write(
        root.join("subscriptions.json"),
        serde_json::to_vec(&vec![subscription]).unwrap(),
    )
    .unwrap();
    for (name, bytes) in [
        ("remote_cache/fixture.json", "opaque module"),
        ("remote_cache/local_imports.json", "[]"),
        ("remote_cache/local/local.json", "irreplaceable"),
        ("remote_refresh.json", "{\"attempts\":{}}"),
        ("icons/fixture.png", "image"),
        ("subscription_cache/a.source.json", "original response"),
        ("profile_cache/a.yaml", "profile"),
        ("scripts/fixture.js", "$done({})"),
        ("scripts/user.js", "user source"),
    ] {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}
#[test]
fn migration_backs_up_then_preserves_local_imports_and_offline_downloads() {
    let root = tempfile::tempdir().unwrap();
    legacy(root.path());
    let backups = super::super::run(root.path()).unwrap();
    assert_eq!(backups.len(), 1);
    assert!(
        backups[0]
            .join("legacy/remote_cache/local/local.json")
            .is_file()
    );
    assert!(backups[0].join("converted/app.toml").is_file());
    assert_eq!(
        RemoteManager::new(root.path().into()).load().unwrap().len(),
        1
    );
    assert_eq!(
        SubscriptionStore::new(root.path().into())
            .load()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("resources/imports/local/local.json")).unwrap(),
        "irreplaceable"
    );
    assert!(
        root.path()
            .join("cache/subscriptions/a.source.json")
            .is_file()
    );
    assert!(root.path().join("cache/remotes/fixture.json").is_file());
    assert!(root.path().join("cache/remotes/refresh.json").is_file());
    assert!(root.path().join("cache/scripts/fixture.js").is_file());
    assert!(root.path().join("scripts/user.js").is_file());
    for name in LEGACY {
        assert!(!root.path().join(name).exists(), "{name}");
    }
    assert!(super::super::run(root.path()).unwrap().is_empty());
    std::fs::remove_dir_all(root.path().join("cache")).unwrap();
    assert!(
        root.path()
            .join("resources/imports/local/local.json")
            .is_file()
    );
    assert_eq!(
        RemoteManager::new(root.path().into()).load().unwrap().len(),
        1
    );
}
#[test]
fn corrupt_manifest_or_destination_conflict_never_removes_original_data() {
    for corrupt in [true, false] {
        let root = tempfile::tempdir().unwrap();
        legacy(root.path());
        if corrupt {
            std::fs::write(root.path().join("subscriptions.json"), "{broken").unwrap();
        } else {
            std::fs::create_dir_all(root.path().join("cache/icons")).unwrap();
            std::fs::write(root.path().join("cache/icons/fixture.png"), "different").unwrap();
        }
        let original = std::fs::read(root.path().join("app.toml")).unwrap();
        assert!(super::super::run(root.path()).is_err());
        assert_eq!(
            std::fs::read(root.path().join("app.toml")).unwrap(),
            original
        );
        assert!(root.path().join("remotes.json").is_file());
        assert!(root.path().join("remote_cache/local/local.json").is_file());
        // Fix and retry; previously copied destinations are byte-verified.
        std::fs::write(root.path().join("subscriptions.json"), "[]").unwrap();
        if !corrupt {
            std::fs::remove_file(root.path().join("cache/icons/fixture.png")).unwrap();
        }
        super::super::run(root.path()).unwrap();
        assert!(!super::super::is_required(root.path()).unwrap());
    }
}

#[test]
fn interim_app_subscription_section_moves_to_its_own_toml_automatically() {
    let root = tempfile::tempdir().unwrap();
    settings::ensure_app_settings(root.path()).unwrap();
    let old = tempfile::tempdir().unwrap();
    let sub = SubscriptionStore::new(old.path().into())
        .add("interim", "https://example.invalid/sub", false, None)
        .unwrap();
    settings::persist_app_section(
        root.path(),
        "resources",
        &serde_json::json!({"remotes": [], "subscriptions": [sub]}),
    )
    .unwrap();
    assert!(super::super::is_required(root.path()).unwrap());
    super::super::run(root.path()).unwrap();
    assert_eq!(
        SubscriptionStore::new(root.path().into()).load().unwrap()[0].name,
        "interim"
    );
    let section = settings::load_app_section::<serde_json::Value>(root.path(), "resources")
        .unwrap()
        .unwrap();
    assert!(section.get("subscriptions").is_none());
    assert!(root.path().join("subscriptions.toml").exists());
    assert!(!super::super::is_required(root.path()).unwrap());
}

#[test]
fn failed_two_document_publication_restores_original_app_and_removes_new_subscription_file() {
    struct Failing;
    impl super::super::SettingsMigration for Failing {
        fn id(&self) -> &'static str {
            "test-two-document-rollback"
        }
        fn is_required(&self, _: &Path) -> LanpathResult<bool> {
            Ok(true)
        }
        fn legacy_files(&self) -> &'static [&'static str] {
            &[]
        }
        fn backup_files(&self) -> &'static [&'static str] {
            &["app.toml", "subscriptions.toml"]
        }
        fn rollback_files(&self) -> &'static [&'static str] {
            &["app.toml", "subscriptions.toml"]
        }
        fn output_files(&self) -> &'static [&'static str] {
            &["app.toml", "subscriptions.toml"]
        }
        fn migrate(&self, root: &Path) -> LanpathResult<()> {
            std::fs::write(root.join("app.toml"), "[resources]\nremotes = []\n")?;
            std::fs::write(root.join("subscriptions.toml"), "subscriptions = []\n")?;
            Ok(())
        }
        fn validate(&self, _: &Path) -> LanpathResult<()> {
            Err(LanpathError::Config("fixture failure".into()))
        }
    }
    static FAILING: Failing = Failing;
    let root = tempfile::tempdir().unwrap();
    let original = b"# original\n[resources]\nremotes = []\nsubscriptions = []\n";
    std::fs::write(root.path().join("app.toml"), original).unwrap();
    assert!(super::super::run_one(root.path(), &FAILING).is_err());
    assert_eq!(
        std::fs::read(root.path().join("app.toml")).unwrap(),
        original
    );
    assert!(!root.path().join("subscriptions.toml").exists());
}
