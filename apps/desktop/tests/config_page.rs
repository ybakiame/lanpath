//! Overview preview: raw source without overrides, composed configuration with a bound script.
mod common;
use common::{TestRoute, TestServer, data_root, start_app_desktop as start_app};
use lanpath_client::{
    CachedSubscriptionContent, RequestProxyMode, SubscriptionSource, SubscriptionStore,
};
use std::{path::Path, time::Duration};

const YAML: &str = "# 原始注释不可丢失\nproxies:\n  - name: 节点甲\n    type: ss\n    server: 192.0.2.1\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n";
fn seed(root: &Path, url: &str, source: bool, script: bool) -> uuid::Uuid {
    let store = SubscriptionStore::new(root.to_path_buf());
    let sub = store.add("预览订阅", url, true, None).unwrap();
    if source {
        let parsed = lanpath_client::parse_subscription_body(YAML, None).unwrap();
        store
            .write_source(
                sub.id,
                &SubscriptionSource {
                    format: parsed.format,
                    content: YAML.into(),
                },
            )
            .unwrap();
        store
            .write_cached_content(
                sub.id,
                &CachedSubscriptionContent {
                    format: parsed.format,
                    singbox_nodes: parsed.singbox_nodes,
                },
            )
            .unwrap();
    }
    if script {
        let id = uuid::Uuid::new_v4();
        lanpath_client::ProfileStoreV2::new(root.to_path_buf())
            .save(&[lanpath_client::Profile {
                id,
                name: "预览脚本".into(),
                yaml_override: String::new(),
                yaml_url: None,
                js_url: None,
                js_override:
                    "function main(config) { config.log = { level: 'debug' }; return config; }"
                        .into(),
            }])
            .unwrap();
        store.set_profile_id(sub.id, Some(id)).unwrap();
    }
    let mut config = lanpath_client::ClientConfig::load_for_requests(root).unwrap();
    config.active_subscription_id = Some(sub.id);
    config.request_proxy_mode = RequestProxyMode::Core;
    config.save().unwrap();
    sub.id
}
fn initialized() -> (
    tempfile::TempDir,
    std::collections::BTreeMap<String, String>,
) {
    let (dir, entries) = data_root();
    let mut app = start_app(entries.clone()).0;
    common::wait_ready(&mut app);
    drop(app);
    (dir, entries)
}
fn preview(app: &mut nsh_testing::Headless) {
    common::wait_ready(app);
    app.press("id:nav-config").unwrap();
    app.wait_for("id:config-preview").unwrap();
}
fn content(app: &nsh_testing::Headless) -> String {
    app.screen()
        .input_text("id:config-preview")
        .unwrap()
        .unwrap_or_default()
        .into()
}
#[test]
fn preview_is_disabled_without_selection_and_absent_from_sidebar() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    assert!(!app.screen().enabled("id:nav-config").unwrap());
    assert!(app.press("id:nav-config").is_err());
    assert!(
        !app.screen()
            .matches("id:app-sidebar >> id:nav-config")
            .unwrap()
    );
}
#[test]
fn no_script_shows_exact_saved_source_without_network_or_composition() {
    let server = TestServer::new(vec![TestRoute::ok("/sub", YAML)]);
    let (dir, entries) = initialized();
    seed(dir.path(), &server.url("/sub"), true, false);
    let mut app = start_app(entries).0;
    preview(&mut app);
    app.wait_for("订阅原文").unwrap();
    assert_eq!(content(&app), YAML);
    assert!(server.recorded().is_empty());
    assert!(!content(&app).contains("listen_port"));
    common::close_workspace(&mut app);
    assert!(app.screen().shows("开始之前"));
}
#[test]
fn bound_script_generates_final_json_and_labels_its_scope() {
    let (dir, entries) = initialized();
    seed(dir.path(), "http://127.0.0.1:9/unreachable", true, true);
    let mut app = start_app(entries).0;
    preview(&mut app);
    app.wait_for_within("覆写后的最终配置", Duration::from_secs(10))
        .unwrap();
    let document: serde_json::Value = serde_json::from_str(&content(&app)).unwrap();
    assert_eq!(document["log"]["level"], "debug");
    assert!(content(&app).contains("节点甲"));
    assert!(content(&app).contains("listen_port"));
    assert!(
        app.screen()
            .shows("按当前订阅与已保存设置合成预览。预览不代表正在运行的配置，也不替代核心校验。")
    );
}
#[test]
fn missing_saved_source_is_an_actionable_error_and_does_not_fetch() {
    let server = TestServer::new(vec![TestRoute::ok("/sub", YAML)]);
    let (dir, entries) = initialized();
    seed(dir.path(), &server.url("/sub"), false, false);
    let mut app = start_app(entries).0;
    preview(&mut app);
    app.wait_for("text:/所选订阅尚无已保存原文，请先刷新订阅/")
        .unwrap();
    assert!(server.recorded().is_empty());
    assert!(content(&app).is_empty());
}
#[test]
fn reload_does_not_show_a_disabled_subscription_as_current() {
    let (dir, entries) = initialized();
    let id = seed(dir.path(), "http://127.0.0.1:9/unreachable", true, false);
    let mut app = start_app(entries).0;
    preview(&mut app);
    app.wait_for("订阅原文").unwrap();
    assert_eq!(content(&app), YAML);
    let store = SubscriptionStore::new(dir.path().to_path_buf());
    let mut items = store.load().unwrap();
    items.iter_mut().find(|s| s.id == id).unwrap().enabled = false;
    store.save(&items).unwrap();
    app.press("id:config-compose").unwrap();
    app.wait_for("text:/请先选择一个启用的订阅/").unwrap();
    assert!(content(&app).is_empty());
}
