//! Layout changes preserve real editing state and accessible selection.
mod common;
use common::{data_root, start_app_desktop as start_app};
use nsh_core::{Points, Size};

#[test]
fn launch_manifest_is_used_for_the_native_window_minimum() {
    let root = lanpath_desktop_nsh::root();
    let window = root.main_window_ref().expect("manifest window options");
    assert_eq!(
        window.attributes.min_size,
        Some(Some(Size::new(Points(960.0), Points(640.0))))
    );
}
#[test]
fn core_cards_have_visible_bounds_and_select_the_intended_version() {
    let (dir, entries) = data_root();
    common::init_root_then(&entries, dir.path(), |root| {
        let active = common::seed_core(root, lanpath_common::CoreType::SingBox, "1.2.3", "1.2.3");
        common::save_core_config(root, &active);
        common::seed_core(root, lanpath_common::CoreType::SingBox, "1.2.4", "1.2.4");
    });
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    common::navigate(&mut app, "核心");
    app.wait_for("id:core-row-sing-box-1.2.4").unwrap();
    let nodes = app.screen().layout_nodes();
    let bounds = |version| {
        nodes
            .iter()
            .find(|n| n.test_id.as_deref() == Some(format!("core-row-sing-box-{version}").as_str()))
            .unwrap()
            .bounds
    };
    let a = bounds("1.2.3");
    let b = bounds("1.2.4");
    assert!(a.size.height.0 >= 80.0 && b.size.height.0 >= 80.0);
    assert!((a.origin.y.0 - b.origin.y.0).abs() < 1.0);
    app.press("id:core-use-sing-box-1.2.4").unwrap();
    app.wait_for("sing-box · v1.2.4").unwrap();
}
#[test]
fn subscription_sheet_is_half_width_and_cards_reflow_from_actual_space() {
    let (dir, entries) = data_root();
    let mut first = start_app(entries.clone()).0;
    common::wait_ready(&mut first);
    drop(first);
    let store = lanpath_client::SubscriptionStore::new(dir.path().into());
    store
        .add("卡片甲", "http://127.0.0.1:9/a", true, None)
        .unwrap();
    store
        .add("卡片乙", "http://127.0.0.1:9/b", false, None)
        .unwrap();
    let mut app = start_app(entries).0;
    common::open_subscriptions(&mut app);
    let nodes = app.screen().layout_nodes();
    let bounds = |id: &str| {
        nodes
            .iter()
            .find(|n| n.test_id.as_deref() == Some(id))
            .unwrap()
            .bounds
    };
    assert!((bounds("workspace-sheet-header").size.width.0 - 440.0).abs() < 2.0);
    let a = bounds("subscription-row-卡片甲");
    let b = bounds("subscription-row-卡片乙");
    assert!((a.origin.y.0 - b.origin.y.0).abs() < 1.0);
    assert!(b.origin.x.0 > a.origin.x.0 + a.size.width.0);
    assert!(!app.screen().shows("已启用"));
    assert!(common::subscription_checked(&app, "卡片甲"));
    assert!(!common::subscription_checked(&app, "卡片乙"));
    app.set_environment("large text", |env| env.font_scale = 1.5)
        .unwrap();
    let nodes = app.screen().layout_nodes();
    let a = nodes
        .iter()
        .find(|n| n.test_id.as_deref() == Some("subscription-row-卡片甲"))
        .unwrap()
        .bounds;
    let b = nodes
        .iter()
        .find(|n| n.test_id.as_deref() == Some("subscription-row-卡片乙"))
        .unwrap()
        .bounds;
    assert!(b.origin.y.0 > a.origin.y.0);
}
#[test]
fn sidebar_hides_and_returns_without_losing_the_current_draft() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    common::navigate(&mut app, "请求设置");
    app.type_text("id:request-github-prefix", "https://draft.example/")
        .unwrap();
    app.press("id:sidebar-toggle").unwrap();
    assert!(!app.screen().matches("id:app-sidebar").unwrap());
    assert!(app.screen().shows("请求设置"));
    app.press("id:sidebar-toggle").unwrap();
    assert!(app.screen().matches("id:app-sidebar").unwrap());
    assert_eq!(
        app.screen().input_text("id:request-github-prefix").unwrap(),
        Some("https://draft.example/")
    );
}
#[test]
fn appearance_is_compact_and_has_no_explanatory_footer() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    common::navigate(&mut app, "外观");
    for removed in [
        "窗口材质与不透明度",
        "当前使用不透明窗口，可在上方切换主题与明暗模式",
        "已选择：ocean",
    ] {
        assert!(!app.screen().shows(removed));
    }
    let nodes = app.screen().layout_nodes();
    let a = nodes
        .iter()
        .find(|n| n.test_id.as_deref() == Some("mode-group"))
        .unwrap()
        .bounds;
    assert!(a.size.height.0 <= 40.0);
    let parent = "role:button[主题：ocean]";
    assert_eq!(
        app.screen()
            .view_style(parent)
            .unwrap()
            .unwrap()
            .background_color,
        None
    );
}
