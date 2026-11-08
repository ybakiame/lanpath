use super::*;

#[test]
fn migration_preserves_legacy_files_and_section_writes_preserve_other_content() {
    let root = tempfile::tempdir().unwrap();
    let legacy_client = br#"{"mixed_port":17899}"#;
    let legacy_appearance = br#"{"scale_percent":117}"#;
    std::fs::write(root.path().join(LEGACY_CLIENT_FILE), legacy_client).unwrap();
    std::fs::write(root.path().join(LEGACY_APPEARANCE_FILE), legacy_appearance).unwrap();

    ensure_app_settings(root.path()).unwrap();
    assert_eq!(
        std::fs::read(root.path().join(LEGACY_CLIENT_FILE)).unwrap(),
        legacy_client
    );
    assert_eq!(
        std::fs::read(root.path().join(LEGACY_APPEARANCE_FILE)).unwrap(),
        legacy_appearance
    );
    assert_eq!(
        load_client(&app_settings_path(root.path())).unwrap()["mixed_port"],
        17899
    );

    let original = std::fs::read_to_string(app_settings_path(root.path()))
        .unwrap()
        .replace("[appearance]", "# retained comment\n[appearance]");
    std::fs::write(app_settings_path(root.path()), original).unwrap();
    persist_app_section(
        root.path(),
        "client",
        &serde_json::json!({"mixed_port": 18000}),
    )
    .unwrap();

    let text = std::fs::read_to_string(app_settings_path(root.path())).unwrap();
    assert!(text.contains("# retained comment"));
    let appearance: serde_json::Value = load_app_section(root.path(), "appearance")
        .unwrap()
        .unwrap();
    assert_eq!(appearance["scale_percent"], 117);
    assert_eq!(
        load_client(&app_settings_path(root.path())).unwrap()["mixed_port"],
        18000
    );
}

#[test]
fn resaving_same_core_does_not_create_a_previous_version() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("cores/sing-box/1.2.3/sing-box");
    persist_core_current(root.path(), Some(&path)).unwrap();
    persist_core_current(root.path(), Some(&path)).unwrap();
    let core = load_core_settings(root.path()).unwrap().unwrap();
    assert_eq!(core.current.as_ref().unwrap().version, "1.2.3");
    assert!(core.previous.is_none());
}

#[test]
fn nested_core_selection_tracks_current_and_previous_versions() {
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("cores/sing-box/1.2.3/sing-box");
    let second = root.path().join("cores/sing-box/1.2.4/sing-box");
    std::fs::write(
        root.path().join(LEGACY_CLIENT_FILE),
        serde_json::to_vec(&serde_json::json!({"core_binary": first})).unwrap(),
    )
    .unwrap();

    ensure_app_settings(root.path()).unwrap();
    let core = load_core_settings(root.path()).unwrap().unwrap();
    assert_eq!(core.current.as_ref().unwrap().version, "1.2.3");
    assert!(core.previous.is_none());

    persist_client(
        &app_settings_path(root.path()),
        &serde_json::json!({"core_binary": second}),
    )
    .unwrap();
    let persisted = load_app_section::<CoreSettings>(root.path(), "core")
        .unwrap()
        .unwrap();
    assert_eq!(
        persisted.current.as_ref().unwrap().path,
        Path::new("cores/sing-box/1.2.4/sing-box")
    );
    assert_eq!(
        persisted.previous.as_ref().unwrap().path,
        Path::new("cores/sing-box/1.2.3/sing-box")
    );
    let core = load_core_settings(root.path()).unwrap().unwrap();
    assert_eq!(core.previous.as_ref().unwrap().version, "1.2.3");
    assert_eq!(core.kind, CoreType::SingBox);
    assert_eq!(core.channel, "stable");
}

#[test]
fn publication_guard_failure_retains_authoritative_document() {
    let root = tempfile::tempdir().unwrap();
    ensure_app_settings(root.path()).unwrap();
    let original = std::fs::read(app_settings_path(root.path())).unwrap();
    let checks = std::cell::Cell::new(0);
    let result = update_app_section::<serde_json::Value>(
        root.path(),
        "resources",
        |value| {
            *value = serde_json::json!({"remotes": [], "subscriptions": []});
            Ok(())
        },
        || {
            let call = checks.get() + 1;
            checks.set(call);
            if call >= 3 {
                Err(LanpathError::Client("lease cancelled".into()))
            } else {
                Ok(())
            }
        },
    );
    assert!(result.is_err());
    assert_eq!(
        std::fs::read(app_settings_path(root.path())).unwrap(),
        original
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}
