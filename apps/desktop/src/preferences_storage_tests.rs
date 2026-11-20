use super::*;
use nsh_core::storage::StorageBackend;

#[test]
fn fresh_preferences_use_product_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let storage = AppStorage::open(dir.path()).unwrap();
    let entries = storage.load().unwrap();
    assert_eq!(entries.get("theme").map(String::as_str), Some("ocean"));
    assert_eq!(entries.get("language").map(String::as_str), Some("system"));
    assert_eq!(
        entries.get("appearance").map(String::as_str),
        Some("system")
    );
}

#[test]
fn writes_preserve_client_settings_and_do_not_create_json_or_default_directory() {
    let dir = tempfile::tempdir().unwrap();
    let storage = AppStorage::open(dir.path()).unwrap();
    let before: serde_json::Value = settings::load_app_section(dir.path(), "client")
        .unwrap()
        .unwrap();
    storage.write(StorageChange::Set("appearance".into(), "dark".into()));
    storage.write(StorageChange::Set(
        "window-placement".into(),
        "native-state".into(),
    ));
    storage.flush().unwrap();
    let ui: UiDocument = settings::load_app_section(dir.path(), "ui")
        .unwrap()
        .unwrap();
    assert_eq!(ui.appearance, "dark");
    assert_eq!(
        ui.runtime.get("window-placement").map(String::as_str),
        Some("native-state")
    );
    let after: serde_json::Value = settings::load_app_section(dir.path(), "client")
        .unwrap()
        .unwrap();
    assert_eq!(before, after);
    assert!(!dir.path().join("storage.json").exists());
    assert!(!dir.path().join("default").exists());
}

#[test]
fn normal_drop_drains_accepted_writes() {
    let dir = tempfile::tempdir().unwrap();
    {
        let storage = AppStorage::open(dir.path()).unwrap();
        storage.write(StorageChange::Set("language".into(), "zh-CN".into()));
    }
    let ui: UiDocument = settings::load_app_section(dir.path(), "ui")
        .unwrap()
        .unwrap();
    assert_eq!(ui.language, "zh-CN");
}
