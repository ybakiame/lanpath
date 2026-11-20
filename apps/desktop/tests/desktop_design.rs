//! 产品交互回归：草稿不得在导航时静默丢失，事务与真实页面边界保持一致。
mod common;
use common::{data_root, open_subscriptions, start_app};

#[test]
fn subscription_draft_survives_cancelled_navigation_then_can_be_discarded() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);
    app.type_text("id:subscription-name", "未保存的订阅")
        .unwrap();
    app.type_text("id:subscription-url", "https://example.org/config")
        .unwrap();
    app.press("id:workspace-sheet-close").unwrap();
    app.wait_for("id:navigation-leave-dialog").unwrap();
    app.press("继续编辑").unwrap();
    assert_eq!(
        app.screen().input_text("id:subscription-name").unwrap(),
        Some("未保存的订阅")
    );
    assert!(!app.screen().matches("id:rules-target").unwrap());
    app.press("id:workspace-sheet-close").unwrap();
    app.press("放弃并离开").unwrap();
    app.wait_for_gone(
        "id:workspace-sheet-close",
        std::time::Duration::from_secs(3),
    )
    .unwrap();
    common::open_subscriptions(&mut app);
    app.wait_for("id:subscription-name").unwrap();
    assert_eq!(
        app.screen().input_text("id:subscription-name").unwrap(),
        Some("")
    );
}

#[test]
fn invalid_rule_draft_is_protected_and_successful_save_clears_the_guard() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    common::navigate(&mut app, "规则");
    app.type_text("id:rules-target", "example.org").unwrap();
    common::navigate(&mut app, "日志");
    app.wait_for("id:navigation-leave-dialog").unwrap();
    app.press("继续编辑").unwrap();
    app.press("id:rules-save").unwrap();
    app.wait_for("规则已保存，下次启动或重启核心时应用。")
        .unwrap();
    common::navigate(&mut app, "日志");
    app.wait_for("内存日志").unwrap();
    assert!(!app.screen().matches("id:navigation-leave-dialog").unwrap());
}

#[test]
fn request_draft_is_preserved_when_navigation_is_cancelled() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    common::navigate(&mut app, "请求设置");
    app.type_text("id:request-github-prefix", "https://mirror.example/")
        .unwrap();
    common::navigate(&mut app, "概览");
    app.wait_for("id:navigation-leave-dialog").unwrap();
    app.press("继续编辑").unwrap();
    assert_eq!(
        app.screen().input_text("id:request-github-prefix").unwrap(),
        Some("https://mirror.example/")
    );
    app.press("保存").unwrap();
    common::navigate(&mut app, "概览");
    app.wait_for("开始之前").unwrap();
    assert!(!app.screen().matches("id:navigation-leave-dialog").unwrap());
}

#[test]
fn backup_password_is_not_silently_lost_on_navigation() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    common::navigate(&mut app, "备份");
    app.type_text("id:backup-export-password", "example-only-password")
        .unwrap();
    common::navigate(&mut app, "概览");
    app.wait_for("id:navigation-leave-dialog").unwrap();
    app.press("继续编辑").unwrap();
    assert_eq!(
        app.screen()
            .input_text("id:backup-export-password")
            .unwrap(),
        Some("example-only-password")
    );
}

#[test]
fn fresh_data_uses_ocean() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    common::navigate(&mut app, "外观");
    app.wait_for("id:theme-ocean").unwrap();
    assert!(app.screen().selected("role:button[主题：ocean]").unwrap());
}

#[test]
fn rule_removal_requires_a_deliberate_confirmation() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    common::navigate(&mut app, "规则");
    app.type_text("id:rules-target", "example.org").unwrap();
    app.press("id:rules-save").unwrap();
    app.type_text("id:rules-search", "example.org").unwrap();
    app.press("id:rule-remove").unwrap();
    app.wait_for("id:rule-delete-dialog").unwrap();
    app.press("取消").unwrap();
    assert!(
        app.screen()
            .texts()
            .iter()
            .any(|t| t.contains("example.org"))
    );
    app.press("id:rule-remove").unwrap();
    app.press("删除规则").unwrap();
    app.wait_for("没有匹配的规则。").unwrap();
}

#[test]
fn returning_request_settings_to_the_saved_value_does_not_prompt() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    common::navigate(&mut app, "请求设置");
    app.type_text("id:request-github-prefix", "https://example.org/")
        .unwrap();
    app.clear("id:request-github-prefix").unwrap();
    common::navigate(&mut app, "概览");
    app.wait_for("开始之前").unwrap();
    assert!(!app.screen().matches("id:navigation-leave-dialog").unwrap());
}

#[test]
fn subscription_can_be_submitted_with_enter_and_refreshed_for_real() {
    let (_dir, entries) = data_root();
    let server =
        common::TestServer::new(vec![common::TestRoute::ok("/config", common::CLASH_YAML)]);
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);
    app.type_text("id:subscription-name", "键盘添加").unwrap();
    app.type_text("id:subscription-url", &server.url("/config"))
        .unwrap();
    app.submit("id:subscription-url").unwrap();
    app.wait_for("键盘添加").unwrap();
    app.wait_for_within(
        "text:/刷新完成：2 个节点/",
        std::time::Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!(server.count_path("/config"), 1);
    assert_eq!(
        app.screen().input_text("id:subscription-name").unwrap(),
        Some("")
    );
}

#[test]
fn filtering_a_large_subscription_list_targets_the_correct_record() {
    let (dir, entries) = data_root();
    common::init_root_then(&entries, dir.path(), |root| {
        let rows: Vec<_> = (0..300)
            .map(|index| lanpath_client::Subscription {
                id: uuid::Uuid::new_v4(),
                name: format!("线路-{index:03}"),
                url: format!("https://example.org/{index}"),
                enabled: true,
                userinfo: None,
                node_count: 0,
                error: None,
                user_agent: None,
                format: None,
                profile_id: None,
            })
            .collect();
        lanpath_client::SubscriptionStore::new(root.to_path_buf())
            .save(&rows)
            .unwrap();
    });
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);
    app.type_text("id:subscription-search", "线路-299").unwrap();
    app.wait_for("线路-299").unwrap();
    assert!(!app.screen().shows("线路-000"));
    app.press("role:switch[启用订阅 线路-299]").unwrap();
    let rows = lanpath_client::SubscriptionStore::new(dir.path().to_path_buf())
        .load()
        .unwrap();
    assert!(!rows.iter().find(|r| r.name == "线路-299").unwrap().enabled);
    assert!(rows.iter().find(|r| r.name == "线路-000").unwrap().enabled);
    app.clear("id:subscription-search").unwrap();
    app.wait_for("线路-000").unwrap();
    app.type_text("id:subscription-search", "线路-299").unwrap();
    assert!(!common::subscription_checked(&app, "线路-299"));
}
