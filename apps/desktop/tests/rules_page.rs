mod common;
use common::{data_root, start_app};

#[test]
fn quick_add_batch_failure_disable_and_reopen_use_real_storage() {
    let (dir, entries) = data_root();
    let mut app = start_app(entries.clone()).0;
    common::wait_ready(&mut app);
    app.press("规则").unwrap();
    app.type_text("id:rules-target", "example.com").unwrap();
    app.submit("id:rules-target").unwrap();
    app.wait_for("规则已保存，下次启动或重启核心时应用。")
        .unwrap();
    let store = lanpath_client::LocalOverrideStore::new(dir.path().to_path_buf());
    assert!(
        store
            .load()
            .unwrap()
            .singbox
            .rules
            .iter()
            .any(|r| r.target == "example.com")
    );
    app.press("批量输入").unwrap();
    app.type_text(
        "id:rules-batch",
        "domain batch.example.com -> direct\ncidr invalid -> proxy",
    )
    .unwrap();
    app.press("添加规则").unwrap();
    app.wait_for("text:/第 2 行/").unwrap();
    assert!(
        !store
            .load()
            .unwrap()
            .singbox
            .rules
            .iter()
            .any(|r| r.target == "batch.example.com")
    );
    app.clear("id:rules-batch").unwrap();
    app.type_text("id:rules-batch", "domain batch.example.com -> direct")
        .unwrap();
    app.press("添加规则").unwrap();
    app.press("text:停用 >> nth=0").unwrap();
    assert!(
        !store
            .load()
            .unwrap()
            .singbox
            .rules
            .iter()
            .find(|r| r.target == "batch.example.com")
            .unwrap()
            .enabled
    );
    drop(app);
    let mut reopened = start_app(entries).0;
    common::wait_ready(&mut reopened);
    reopened.press("规则").unwrap();
    reopened.wait_for("text:/batch.example.com/").unwrap();
}
#[test]
fn diagnostics_domain_prefills_quick_rule_and_checks_saved_order() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    app.press("诊断").unwrap();
    app.clear("id:diagnostics-target").unwrap();
    app.type_text("id:diagnostics-target", "docs.example.com")
        .unwrap();
    app.press("为此域名添加规则").unwrap();
    assert_eq!(
        app.screen().input_text("id:rules-target").unwrap(),
        Some("docs.example.com")
    );
    app.press("添加规则").unwrap();
    app.press("诊断").unwrap();
    app.clear("id:diagnostics-target").unwrap();
    app.type_text("id:diagnostics-target", "docs.example.com")
        .unwrap();
    app.press("检查本地规则").unwrap();
    app.wait_for("text:/首个可判定出口：proxy/").unwrap();
    app.clear("id:diagnostics-target").unwrap();
    app.type_text("id:diagnostics-target", "https://example.com/path")
        .unwrap();
    app.submit("id:diagnostics-target").unwrap();
    app.wait_for("text:/请输入域名/").unwrap();
    assert!(app.screen().enabled("开始诊断").unwrap());
}
#[test]
fn override_editor_exposes_only_typescript() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    app.press("覆写").unwrap();
    app.press("新建覆写模板").unwrap();
    app.wait_for("本地 TypeScript（可选）").unwrap();
    assert!(!app.screen().texts().iter().any(|t| t.contains("YAML")));
    app.type_text("id:profile-name", "TS 模板").unwrap();
    app.type_text(
        "id:profile-js",
        "function main(config: any) { return config; }",
    )
    .unwrap();
    app.press("保存").unwrap();
    app.wait_for("覆写模板已校验并保存").unwrap();
}
