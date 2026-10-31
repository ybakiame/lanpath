use super::*;

#[test]
fn absent_preferences_are_display_defaults_without_creating_a_file() {
    let root = tempfile::tempdir().unwrap();
    let store = AppearanceStore::new(root.path());
    let preferences = store.load().unwrap();
    assert_eq!(preferences, AppearancePreferences::default());
    assert!(!root.path().join("appearance.json").exists());
}

#[test]
fn bounded_custom_scale_and_black_preference_round_trip_exactly() {
    let root = tempfile::tempdir().unwrap();
    let store = AppearanceStore::new(root.path());
    for percent in [75, 117, 150] {
        let preferences = AppearancePreferences::default()
            .with_theme(ThemePreference::Light)
            .with_accent(AccentPreference::Rose)
            .with_pure_black(true)
            .with_scale_percent(percent);
        store.save(preferences).unwrap();
        assert_eq!(store.load().unwrap(), preferences);
    }
}

#[test]
fn invalid_scale_save_leaves_valid_file_and_live_preferences_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let store = AppearanceStore::new(root.path());
    let previous = AppearancePreferences::default().with_scale_percent(117);
    store.save(previous).unwrap();
    let path = root.path().join("app.toml");
    let bytes = std::fs::read(&path).unwrap();
    let mut state = AppearanceState {
        preferences: previous,
        store: Some(store.clone()),
        load_error: None,
        restore_warning: None,
    };
    for percent in [0, 74, 151, u16::MAX] {
        assert!(state.save(previous.with_scale_percent(percent)).is_err());
        assert_eq!(state.preferences(), previous);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(store.load().unwrap(), previous);
    }
}

#[test]
fn malformed_preferences_are_never_silently_replaced_by_save() {
    let root = tempfile::tempdir().unwrap();
    let store = AppearanceStore::new(root.path());
    let path = root.path().join("appearance.json");
    let valid = serde_json::to_value(AppearancePreferences::default()).unwrap();
    let mut malformed = vec![b"{".to_vec(), b"null".to_vec(), b"{}".to_vec()];
    for (field, value) in [
        ("theme", serde_json::json!("automatic")),
        ("accent", serde_json::json!("red")),
        ("pure_black", serde_json::json!("true")),
        ("scale_percent", serde_json::json!(74)),
        ("scale_percent", serde_json::json!(151)),
        ("scale_percent", serde_json::json!(-1)),
        ("scale_percent", serde_json::json!(99.5)),
        ("unknown", serde_json::json!(true)),
    ] {
        let mut document = valid.clone();
        document[field] = value;
        malformed.push(serde_json::to_vec(&document).unwrap());
    }
    for bytes in malformed {
        std::fs::write(&path, &bytes).unwrap();
        assert!(store.load().is_err());
        assert!(store.save(AppearancePreferences::default()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn reload_failure_retains_last_applied_preferences_until_explicit_repair() {
    let root = tempfile::tempdir().unwrap();
    let store = AppearanceStore::new(root.path());
    let preferences = AppearancePreferences::default()
        .with_theme(ThemePreference::Dark)
        .with_pure_black(true)
        .with_scale_percent(125);
    store.save(preferences).unwrap();
    let mut state = AppearanceState {
        preferences,
        store: Some(store),
        load_error: None,
        restore_warning: None,
    };
    let path = root.path().join("app.toml");
    std::fs::write(&path, "broken").unwrap();
    assert!(state.reload().is_err());
    assert_eq!(state.preferences(), preferences);
    assert!(state.load_error().is_some());
    assert!(
        state
            .save(preferences.with_accent(AccentPreference::Teal))
            .is_err()
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "broken");
    let repaired =
        lanpath_client::settings::merge_app_section_text("", "appearance", &preferences).unwrap();
    std::fs::write(&path, repaired).unwrap();
    state.reload().unwrap();
    assert!(state.load_error().is_none());
    let updated = preferences.with_accent(AccentPreference::Teal);
    state.save(updated).unwrap();
    assert_eq!(state.preferences(), updated);
}

#[test]
fn preferences_reject_missing_root_and_nonfile_destination() {
    let root = tempfile::tempdir().unwrap();
    let missing = AppearanceStore::new(&root.path().join("missing"));
    assert!(missing.load().is_err());
    assert!(missing.save(AppearancePreferences::default()).is_err());
    let store = AppearanceStore::new(root.path());
    std::fs::create_dir(root.path().join("appearance.json")).unwrap();
    assert!(store.load().is_err());
    assert!(store.save(AppearancePreferences::default()).is_err());
    assert!(root.path().join("appearance.json").is_dir());
}

#[cfg(unix)]
#[test]
fn preference_file_cannot_link_outside_the_experiment_root() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let external = outside.path().join("appearance.json");
    let bytes = serde_json::to_vec(&AppearancePreferences::default()).unwrap();
    std::fs::write(&external, &bytes).unwrap();
    symlink(&external, root.path().join("appearance.json")).unwrap();
    let store = AppearanceStore::new(root.path());
    assert!(store.load().is_err());
    assert!(store.save(AppearancePreferences::default()).is_err());
    assert_eq!(std::fs::read(external).unwrap(), bytes);
}

#[test]
fn system_resolves_each_os_appearance_and_explicit_modes_ignore_it() {
    for (appearance, os_dark) in [
        (WindowAppearance::Light, false),
        (WindowAppearance::VibrantLight, false),
        (WindowAppearance::Dark, true),
        (WindowAppearance::VibrantDark, true),
    ] {
        let system = AppearancePreferences::default().with_pure_black(true);
        assert_eq!(system.is_dark(appearance), os_dark);
        assert_eq!(system.effective_pure_black(appearance), os_dark);
        let light = system.with_theme(ThemePreference::Light);
        assert!(!light.is_dark(appearance));
        assert!(!light.effective_pure_black(appearance));
        assert!(light.pure_black());
        let dark = light.with_theme(ThemePreference::Dark);
        assert!(dark.is_dark(appearance));
        assert!(dark.effective_pure_black(appearance));
        assert!(!dark.with_pure_black(false).effective_pure_black(appearance));
        assert_eq!(
            system
                .with_theme(ThemePreference::Light)
                .with_theme(ThemePreference::System),
            system
        );
    }
}
