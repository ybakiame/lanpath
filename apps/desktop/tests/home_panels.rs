//! 首页面板、编辑器和弹窗几何：从用户实际入口验证，不绕开原生覆盖层。
mod common;
use common::{data_root, open_profiles, open_subscriptions, start_app_desktop};
use nsh_core::{Points, Size};

#[test]
fn subscription_is_a_home_sheet_and_has_no_sidebar_page() {
    let (_dir, entries) = data_root();
    let mut app = start_app_desktop(entries).0;
    common::wait_ready(&mut app);
    assert!(
        !app.screen()
            .matches("id:app-sidebar >> id:nav-subscriptions")
            .unwrap()
    );
    assert!(
        !app.screen()
            .matches("id:app-sidebar >> id:nav-profiles")
            .unwrap()
    );
    open_subscriptions(&mut app);
    assert!(app.screen().matches("role:dialog[订阅]").unwrap());
    assert!(app.screen().shows("开始之前"), "面板保留首页上下文");
    app.press_key("escape").unwrap();
    app.wait_for_gone(
        "id:workspace-sheet-close",
        std::time::Duration::from_secs(3),
    )
    .unwrap();
    assert!(app.screen().shows("开始之前"));
}

#[test]
fn new_script_uses_editor_template_without_remote_url_and_retains_draft_on_cancel() {
    let (dir, entries) = data_root();
    let mut app = start_app_desktop(entries).0;
    open_profiles(&mut app);
    app.press("id:profiles-create").unwrap();
    app.wait_for("id:profile-js").unwrap();
    assert!(!app.screen().matches("id:profile-js-url").unwrap());
    assert_eq!(
        app.screen().input_text("id:profile-js").unwrap(),
        Some("function main(config) {\n  return config;\n}\n")
    );
    app.type_text("id:profile-name", "最小脚本").unwrap();
    app.press("取消").unwrap();
    app.wait_for("有未保存修改，要丢弃吗？").unwrap();
    app.press("继续编辑").unwrap();
    assert_eq!(
        app.screen().input_text("id:profile-name").unwrap(),
        Some("最小脚本")
    );
    app.press("保存").unwrap();
    app.wait_for("覆写模板已校验并保存").unwrap();
    let scripts = lanpath_client::ProfileStoreV2::new(dir.path().to_path_buf())
        .load()
        .unwrap();
    assert_eq!(scripts.len(), 1);
    assert!(scripts[0].js_url.is_none());
    assert!(scripts[0].js_override.contains("return config"));
}

#[test]
fn script_dialog_fits_the_window_and_the_editor_remains_usable_at_minimum_size() {
    let (_dir, entries) = data_root();
    let mut app = start_app_desktop(entries).0;
    app.set_viewport(Size::new(Points(960.0), Points(640.0)))
        .unwrap();
    open_profiles(&mut app);
    app.press("id:profiles-create").unwrap();
    app.wait_for("id:profile-js").unwrap();
    let nodes = app.screen().layout_nodes();
    let panel = nodes
        .iter()
        .find(|n| n.test_id.as_deref() == Some("form-dialog-panel"))
        .unwrap();
    let right = panel.bounds.origin.x.0 + panel.bounds.size.width.0;
    let bottom = panel.bounds.origin.y.0 + panel.bounds.size.height.0;
    assert!(panel.bounds.origin.x.0 >= 24.0 && right <= 936.0);
    assert!(panel.bounds.origin.y.0 >= 24.0 && bottom <= 616.0);
    let editor = nodes
        .iter()
        .find(|n| n.test_id.as_deref() == Some("profile-js"))
        .unwrap();
    assert!(
        editor.bounds.size.height.0 >= 180.0,
        "代码编辑区不能退化为单行输入"
    );
    common::replace_script(&mut app, "config.");
    app.press("id:profile-completions").unwrap();
    app.wait_for("outbounds").unwrap();
    app.press_key("escape").unwrap();
    assert!(
        app.screen().matches("id:profile-js").unwrap(),
        "Escape 先关闭补全，保留编辑 Dialog"
    );
}
