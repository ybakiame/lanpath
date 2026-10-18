use super::rules::{run_save, set_categories};
use super::views::SaveLocalOverrideInput;
use lanpath_client::local_override::{LocalOverride, LocalOverrideStore, RuleSetManager};

fn fixture(dir: &std::path::Path) -> LocalOverride {
    let ovr: LocalOverride = serde_json::from_value(serde_json::json!({
        "ruleset_metadata_version":1,"builtins_seeded":true,
        "custom_rule_sets":[{"id":"demo","name":"Demo","tag":"demo","category":"旧分类",
            "source":{"kind":"remote","url":"https://example.invalid/demo.srs","format":"binary"},
            "last_updated":100,"remote_updated_at":123,"remote_state":{"checked_at":456,"validator_url":"https://example.invalid/demo.srs","etag":"v1","last_modified":null}}]
    })).unwrap();
    LocalOverrideStore::new(dir.into()).save(&ovr).unwrap();
    ovr
}

#[test]
fn category_patch_preserves_cache_and_only_changes_requested_category() {
    let dir = tempfile::tempdir().unwrap();
    let original = fixture(dir.path());
    let manager = RuleSetManager::new(dir.path().into());
    std::fs::create_dir_all(manager.custom_rule_set_dir()).unwrap();
    let path =
        manager.custom_rule_set_file_path("demo", original.custom_rule_sets[0].file_format());
    std::fs::write(&path, b"must-stay").unwrap();
    let view = set_categories(dir.path(), &["demo".into()], "  工作  ").unwrap();
    assert_eq!(view.custom_rule_sets[0].category, "工作");
    let stored = LocalOverrideStore::new(dir.path().into()).load().unwrap();
    assert_eq!(
        stored.custom_rule_sets[0].remote_state,
        original.custom_rule_sets[0].remote_state
    );
    assert_eq!(
        stored.custom_rule_sets[0].source,
        original.custom_rule_sets[0].source
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"must-stay");
    assert!(set_categories(dir.path(), &["missing".into()], "other").is_err());
    assert!(set_categories(dir.path(), &["demo".into()], "bad\nname").is_err());
}

#[test]
fn readonly_validators_cannot_be_forged_and_source_changes_reset_them() {
    let dir = tempfile::tempdir().unwrap();
    let original = fixture(dir.path());
    let mut payload = serde_json::json!({"singbox":{"rules":[],"rule_sets":[]},"custom_rule_sets":original.custom_rule_sets});
    payload["custom_rule_sets"][0]["remote_state"]["etag"] = "forged".into();
    payload["custom_rule_sets"][0]["category"] = "新分类".into();
    payload["custom_rule_sets"][0]["last_updated"] = 999999.into();
    let input: SaveLocalOverrideInput = serde_json::from_value(payload.clone()).unwrap();
    run_save(dir.path(), input).unwrap();
    let store = LocalOverrideStore::new(dir.path().into());
    let stored = store.load().unwrap();
    assert_eq!(
        stored.custom_rule_sets[0].remote_state.etag.as_deref(),
        Some("v1")
    );
    assert_eq!(stored.custom_rule_sets[0].category, "新分类");
    assert_eq!(stored.custom_rule_sets[0].last_updated, 100);
    payload["custom_rule_sets"][0]["source"]["url"] = "https://example.invalid/changed.srs".into();
    run_save(dir.path(), serde_json::from_value(payload).unwrap()).unwrap();
    let stored = store.load().unwrap();
    assert!(stored.custom_rule_sets[0].remote_state.etag.is_none());
    assert_eq!(stored.custom_rule_sets[0].remote_updated_at, 0);
}
