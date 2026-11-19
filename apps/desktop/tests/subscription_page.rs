//! Headless 测试（`nsh_testing::Headless`，NSH 0.5.1）：订阅页真实列表与持久化。
//! 每个场景使用独立临时数据根（见 `common::data_root`）；刷新用例经
//! `common::TestServer` 本地真实 HTTP 服务器联网，不依赖互联网。

mod common;

use std::time::Duration;

use common::{TestRoute, TestServer, data_root, open_subscriptions, start_app, texts};

const URL_ERROR: &str =
    "新增订阅失败:validation error: Subscription URL must start with http:// or https://";
const NAME_ERROR: &str = "新增订阅失败:validation error: 订阅名称不能为空";

/// 新数据根上订阅列表为空。
#[test]
fn fresh_data_root_shows_an_empty_subscription_list() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);
    let seen = texts(&app);
    assert!(seen.contains(&"共 0 个订阅"), "{seen:?}");
}

/// 新增订阅：名称 + URL 提交后列表显示名称/启用状态/节点数，表单清空。
#[test]
fn adding_a_subscription_shows_it_and_clears_the_form() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);
    app.type_text("id:subscription-name", "东京专线").unwrap();
    app.type_text("id:subscription-url", "https://example.com/sub.yaml")
        .unwrap();
    app.press("新增订阅").unwrap();
    let seen = texts(&app);
    for expected in ["共 1 个订阅", "东京专线", "已启用", "未获取节点"] {
        assert!(seen.contains(&expected), "missing {expected}: {seen:?}");
    }
    assert_eq!(
        app.screen().input_text("id:subscription-name").unwrap(),
        Some("")
    );
    assert_eq!(
        app.screen().input_text("id:subscription-url").unwrap(),
        Some("")
    );
}

/// 同一数据根上重开应用，订阅从真实文件持久化恢复。
#[test]
fn saved_subscriptions_restore_across_relaunches() {
    let (_dir, entries) = data_root();
    let mut first = start_app(entries.clone()).0;
    open_subscriptions(&mut first);
    first.type_text("id:subscription-name", "持久订阅").unwrap();
    first
        .type_text("id:subscription-url", "https://example.com/p.yaml")
        .unwrap();
    first.press("新增订阅").unwrap();
    assert!(texts(&first).contains(&"持久订阅"));
    drop(first);

    let mut second = start_app(entries).0;
    open_subscriptions(&mut second);
    second.wait_for("持久订阅").unwrap();
    let seen = texts(&second);
    for expected in ["持久订阅", "共 1 个订阅", "已启用"] {
        assert!(seen.contains(&expected), "missing {expected}: {seen:?}");
    }
}

/// 无效输入被拒绝并明确显示错误，已有数据不被覆盖。
#[test]
fn invalid_input_is_rejected_without_overwriting_existing_data() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);
    app.type_text("id:subscription-name", "保留订阅").unwrap();
    app.type_text("id:subscription-url", "https://example.com/keep.yaml")
        .unwrap();
    app.press("新增订阅").unwrap();
    assert!(texts(&app).contains(&"共 1 个订阅"));

    // 非法 URL：错误显示，列表不变，表单保留输入。
    app.clear("id:subscription-name").unwrap();
    app.clear("id:subscription-url").unwrap();
    app.type_text("id:subscription-name", "坏订阅").unwrap();
    app.type_text("id:subscription-url", "ftp://example.com/x.yaml")
        .unwrap();
    app.press("新增订阅").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&URL_ERROR), "{seen:?}");
    assert!(seen.contains(&"共 1 个订阅"), "{seen:?}");
    assert!(seen.contains(&"保留订阅"), "{seen:?}");
    assert!(!seen.contains(&"坏订阅"), "{seen:?}");

    // 空名称：同样拒绝且已有数据不动。
    app.clear("id:subscription-name").unwrap();
    app.clear("id:subscription-url").unwrap();
    app.type_text("id:subscription-url", "https://example.com/no-name.yaml")
        .unwrap();
    assert_eq!(
        app.screen().input_text("id:subscription-name").unwrap(),
        Some("")
    );
    app.press("新增订阅").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&NAME_ERROR), "{seen:?}");
    assert!(seen.contains(&"共 1 个订阅"), "{seen:?}");
}

/// 启用开关来回切换并即时反映在列表状态文本上。
#[test]
fn toggling_a_subscription_off_and_back_on() {
    let (dir, entries) = data_root();
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);
    app.type_text("id:subscription-name", "切换订阅").unwrap();
    app.type_text("id:subscription-url", "https://example.com/t.yaml")
        .unwrap();
    app.press("新增订阅").unwrap();
    assert!(texts(&app).contains(&"已启用"));

    let enabled = format!("{} >> role:switch", enabled_selector(&dir, "切换订阅"));
    app.press(&enabled).unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"已停用"), "{seen:?}");
    assert!(!seen.contains(&"已启用"), "{seen:?}");

    app.press(&enabled).unwrap();
    assert!(texts(&app).contains(&"已启用"));
}

fn enabled_selector(dir: &tempfile::TempDir, name: &str) -> String {
    let subscriptions = lanpath_client::SubscriptionStore::new(dir.path().into())
        .load()
        .unwrap();
    let id = subscriptions
        .iter()
        .find(|subscription| subscription.name == name)
        .unwrap()
        .id;
    format!("id:subscription-enabled-{id}")
}

/// 删除需经确认对话框，删除结果在同一数据根上持久化。
#[test]
fn deleting_a_subscription_needs_confirmation_and_persists() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries.clone()).0;
    open_subscriptions(&mut app);
    app.type_text("id:subscription-name", "待删订阅").unwrap();
    app.type_text("id:subscription-url", "https://example.com/d.yaml")
        .unwrap();
    app.press("新增订阅").unwrap();
    assert!(texts(&app).contains(&"共 1 个订阅"));

    app.press("id:subscription-row-待删订阅 >> text:更多")
        .unwrap();
    app.press("id:subscription-delete").unwrap();
    assert!(texts(&app).contains(&"删除订阅“待删订阅”？"));
    app.press("确认删除").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"尚无订阅。"), "{seen:?}");
    assert!(seen.contains(&"共 0 个订阅"), "{seen:?}");
    drop(app);

    let mut reopened = start_app(entries).0;
    open_subscriptions(&mut reopened);
    reopened.wait_for("尚无订阅。").unwrap();
}

/// 固定 YAML 测试订阅（两个 SS 节点，共享解析器可识别为 ClashYaml）。
const YAML: &str = "proxies:\n  - name: 节点甲\n    type: ss\n    server: 192.0.2.1\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n  - name: 节点乙\n    type: ss\n    server: 192.0.2.2\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n";

fn add_subscription(app: &mut nsh_testing::Headless, name: &str, url: &str) {
    app.type_text("id:subscription-name", name).unwrap();
    app.type_text("id:subscription-url", url).unwrap();
    app.press("新增订阅").unwrap();
}

/// 新增指向本地服务器的订阅后自动后台刷新：busy 提示、结果反馈与节点数
/// 都真实上屏；请求带已保存 UA。
#[test]
fn adding_auto_refreshes_in_the_background_against_the_saved_url() {
    let server = TestServer::new(vec![TestRoute::delayed(
        "/sub.yaml",
        YAML,
        Duration::from_millis(300),
    )]);
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);

    app.type_text("id:subscription-name", "自动刷新").unwrap();
    app.type_text("id:subscription-url", &server.url("/sub.yaml"))
        .unwrap();
    app.type_text("id:subscription-user-agent", " lanpath-headless/1.0 ")
        .unwrap();
    app.press("新增订阅").unwrap();
    assert!(texts(&app).contains(&"正在刷新订阅…"), "{:?}", texts(&app));
    app.wait_for_within("text:/刷新完成：2 个节点/", Duration::from_secs(10))
        .unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"2 个节点 · 格式 ClashYaml"), "{seen:?}");
    let recorded = server.recorded();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].path, "/sub.yaml");
    assert_eq!(
        recorded[0].user_agent.as_deref(),
        Some("lanpath-headless/1.0")
    );
}

/// 同一订阅的重复刷新被去重：在途时再次按“刷新”只产生一次真实请求。
#[test]
fn repeat_refresh_is_locked_while_in_flight() {
    let server = TestServer::new(vec![TestRoute::delayed(
        "/slow.yaml",
        YAML,
        Duration::from_millis(500),
    )]);
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);

    add_subscription(&mut app, "锁定验证", &server.url("/slow.yaml"));
    app.wait_for("正在刷新订阅…").unwrap();
    app.press("id:subscription-row-锁定验证 >> text:刷新")
        .unwrap();
    app.wait_for_within("text:/刷新完成：2 个节点/", Duration::from_secs(10))
        .unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"共 1 个订阅"), "{seen:?}");
    assert_eq!(server.count_path("/slow.yaml"), 1);
}

/// 离页后后台刷新仍由业务层持有：新页面恢复真实 busy，不重复发请求；
/// 旧 worker 完成后新版面通过共享版本自动重读节点，而不是停在旧列表。
#[test]
fn refresh_survives_leaving_the_page_and_is_seen_by_the_next_page() {
    let server = TestServer::new(vec![TestRoute::delayed(
        "/left.yaml",
        YAML,
        Duration::from_millis(1200),
    )]);
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);

    add_subscription(&mut app, "离页刷新", &server.url("/left.yaml"));
    app.wait_for("正在刷新订阅…").unwrap();

    // 旧页面销毁只取消 UI 轮询，业务刷新继续。
    app.press("role:button[外观]").unwrap();
    app.press("role:button[订阅]").unwrap();
    app.wait_for("离页刷新").unwrap();
    app.wait_for("正在刷新订阅…").unwrap();

    // 新页面看到共享在途：再次点刷新不产生第二个网络请求。
    app.press("id:subscription-row-离页刷新 >> text:刷新")
        .unwrap();
    assert_eq!(server.count_path("/left.yaml"), 1);

    // 旧 worker 完成：版本变化让新页面自动重读真实节点并清除 busy。
    app.wait_for_within("2 个节点 · 格式 ClashYaml", Duration::from_secs(10))
        .unwrap();
    let seen = texts(&app);
    assert!(!seen.contains(&"正在刷新订阅…"), "{seen:?}");
    assert_eq!(server.count_path("/left.yaml"), 1);
}

/// 完成发生在离页期间：回到订阅页时初始 busy 必须与实际共享活动对齐，
/// 不能永久显示“正在刷新”。轮询真实落盘直到完成后再重进，窗口可控。
#[test]
fn refresh_completed_before_returning_does_not_leave_the_page_busy() {
    let server = TestServer::new(vec![TestRoute::delayed(
        "/before-return.yaml",
        YAML,
        Duration::from_millis(300),
    )]);
    let (dir, entries) = data_root();
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);

    add_subscription(&mut app, "返回前完成", &server.url("/before-return.yaml"));
    app.wait_for("正在刷新订阅…").unwrap();
    app.press("role:button[外观]").unwrap();

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let done = lanpath_client::SubscriptionStore::new(dir.path().into())
            .load()
            .unwrap()
            .iter()
            .any(|subscription| subscription.name == "返回前完成" && subscription.node_count == 2);
        if done {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    app.press("role:button[订阅]").unwrap();
    app.wait_for("2 个节点 · 格式 ClashYaml").unwrap();
    let seen = texts(&app);
    assert!(!seen.contains(&"正在刷新订阅…"), "{seen:?}");
    assert_eq!(server.count_path("/before-return.yaml"), 1);
}

/// 手动刷新使用已保存的 URL/UA：编辑框里未保存的草稿不会被写回或发出。
#[test]
fn manual_refresh_uses_saved_url_and_keeps_unsaved_draft_unsent() {
    let server = TestServer::new(vec![
        TestRoute::ok("/saved.yaml", YAML),
        TestRoute::ok("/draft.yaml", YAML),
    ]);
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);
    add_subscription(&mut app, "草稿保护", &server.url("/saved.yaml"));
    app.wait_for_within("text:/刷新完成：2 个节点/", Duration::from_secs(10))
        .unwrap();

    // 打开编辑器输入另一个地址但不保存，取消并确认放弃。
    app.press("id:subscription-row-草稿保护 >> text:编辑")
        .unwrap();
    app.clear("id:edit-url").unwrap();
    app.type_text("id:edit-url", &server.url("/draft.yaml"))
        .unwrap();
    app.press("取消").unwrap();
    app.wait_for("放弃未保存的修改？").unwrap();
    app.press("放弃修改").unwrap();

    // 手动刷新仍打向已保存地址。
    app.press("id:subscription-row-草稿保护 >> text:刷新")
        .unwrap();
    app.wait_for_within("text:/刷新完成：2 个节点/", Duration::from_secs(10))
        .unwrap();
    assert_eq!(server.count_path("/saved.yaml"), 2);
    assert_eq!(server.count_path("/draft.yaml"), 0);
}

/// 编辑保存：改名保留上次缓存；列表更新并在同一数据根上持久化。
#[test]
fn editing_a_subscription_saves_and_persists_without_refetching() {
    let server = TestServer::new(vec![TestRoute::ok("/keep.yaml", YAML)]);
    let (_dir, entries) = data_root();
    let mut app = start_app(entries.clone()).0;
    open_subscriptions(&mut app);
    add_subscription(&mut app, "改名前", &server.url("/keep.yaml"));
    app.wait_for_within("text:/刷新完成：2 个节点/", Duration::from_secs(10))
        .unwrap();

    app.press("id:subscription-row-改名前 >> text:编辑")
        .unwrap();
    assert_eq!(
        app.screen().input_text("id:edit-name").unwrap(),
        Some("改名前")
    );
    app.clear("id:edit-name").unwrap();
    app.type_text("id:edit-name", "改名后").unwrap();
    app.press("保存修改").unwrap();
    app.wait_for("已保存修改").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"改名后"), "{seen:?}");
    assert!(!seen.contains(&"改名前"), "{seen:?}");
    assert!(seen.contains(&"2 个节点 · 格式 ClashYaml"), "{seen:?}");
    drop(app);

    let mut reopened = start_app(entries).0;
    open_subscriptions(&mut reopened);
    reopened.wait_for("改名后").unwrap();
    let seen = texts(&reopened);
    assert!(seen.contains(&"2 个节点 · 格式 ClashYaml"), "{seen:?}");
    // 编辑未触发联网。
    assert_eq!(server.recorded().len(), 1);
}

/// 编辑改 URL：保存后旧缓存失效，提示重新刷新，原文明确“请先刷新”。
#[test]
fn editing_the_url_invalidates_cache_and_source() {
    let server = TestServer::new(vec![TestRoute::ok("/a.yaml", YAML)]);
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);
    add_subscription(&mut app, "换址", &server.url("/a.yaml"));
    app.wait_for_within("text:/刷新完成：2 个节点/", Duration::from_secs(10))
        .unwrap();

    app.press("id:subscription-row-换址 >> text:编辑").unwrap();
    app.clear("id:edit-url").unwrap();
    app.type_text("id:edit-url", &server.url("/b.yaml"))
        .unwrap();
    app.press("保存修改").unwrap();
    app.wait_for("已保存修改；URL 已变更，请重新刷新获取新内容")
        .unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"未获取节点"), "{seen:?}");

    app.press("id:subscription-row-换址 >> text:原文").unwrap();
    app.wait_for("“换址”还没有已保存原文，请先刷新").unwrap();
}

/// 取消保护：有草稿时取消需确认；确认放弃后列表保持原样。
#[test]
fn cancelling_an_edit_with_changes_asks_first() {
    let server = TestServer::new(vec![TestRoute::ok("/edit.yaml", YAML)]);
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);
    add_subscription(&mut app, "保护对象", &server.url("/edit.yaml"));
    app.wait_for_within("text:/刷新完成：2 个节点/", Duration::from_secs(10))
        .unwrap();

    app.press("id:subscription-row-保护对象 >> text:编辑")
        .unwrap();
    app.clear("id:edit-name").unwrap();
    app.type_text("id:edit-name", "不应保存").unwrap();
    app.press("取消").unwrap();
    app.wait_for("放弃未保存的修改？").unwrap();
    // 继续编辑：回到表单，草稿还在。
    app.press("继续编辑").unwrap();
    assert_eq!(
        app.screen().input_text("id:edit-name").unwrap(),
        Some("不应保存")
    );
    // 再次取消并确认放弃：列表仍是原名。
    app.press("取消").unwrap();
    app.wait_for("放弃未保存的修改？").unwrap();
    app.press("放弃修改").unwrap();
    app.wait_for("共 1 个订阅").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"保护对象"), "{seen:?}");
    assert!(!seen.contains(&"不应保存"), "{seen:?}");
}

/// 刷新在途时打开编辑器：列表重载不清空未保存草稿；刷新完成后可正常取消。
#[test]
fn edit_draft_survives_while_a_refresh_is_in_flight() {
    let server = TestServer::new(vec![TestRoute::delayed(
        "/draft.yaml",
        YAML,
        Duration::from_millis(500),
    )]);
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);
    add_subscription(&mut app, "在途草稿", &server.url("/draft.yaml"));

    app.press("id:subscription-row-在途草稿 >> text:编辑")
        .unwrap();
    app.clear("id:edit-name").unwrap();
    app.type_text("id:edit-name", "草稿留存").unwrap();
    // 刷新完成、列表重载后，草稿仍在编辑框里。
    app.wait_for_within("text:/刷新完成：2 个节点/", Duration::from_secs(10))
        .unwrap();
    assert_eq!(
        app.screen().input_text("id:edit-name").unwrap(),
        Some("草稿留存")
    );
    app.press("取消").unwrap();
    app.wait_for("放弃未保存的修改？").unwrap();
    app.press("放弃修改").unwrap();
}

/// 原文弹窗展示最后一次成功刷新的真实内容；关闭后回到列表。
#[test]
fn source_dialog_shows_the_last_successful_content_read_only() {
    let server = TestServer::new(vec![TestRoute::ok("/src.yaml", YAML)]);
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);
    add_subscription(&mut app, "原文订阅", &server.url("/src.yaml"));
    app.wait_for_within("text:/刷新完成：2 个节点/", Duration::from_secs(10))
        .unwrap();

    app.press("id:subscription-row-原文订阅 >> text:原文")
        .unwrap();
    app.wait_for("订阅原文（只读）：原文订阅").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"格式：ClashYaml"), "{seen:?}");
    assert!(seen.iter().any(|text| text.contains("节点甲")), "{seen:?}");
    app.press("关闭").unwrap();
    app.wait_for("共 1 个订阅").unwrap();
}

/// 刷新失败：真实错误上屏，旧列表/缓存不被伪报成功，原文入口明确先刷新。
#[test]
fn refresh_failure_shows_the_real_error_without_fake_success() {
    let server = TestServer::new(vec![TestRoute {
        path: "/bad.yaml".to_owned(),
        status: 500,
        body: Vec::new(),
        delay: Duration::ZERO,
    }]);
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);
    add_subscription(&mut app, "失败订阅", &server.url("/bad.yaml"));
    app.wait_for_within("text:/HTTP 500/", Duration::from_secs(10))
        .unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"未获取节点"), "{seen:?}");
    assert!(seen.contains(&"共 1 个订阅"), "{seen:?}");

    app.press("id:subscription-row-失败订阅 >> text:原文")
        .unwrap();
    app.wait_for("“失败订阅”还没有已保存原文，请先刷新")
        .unwrap();
}

/// 当前生效订阅：设置后显示标记并持久化；停用/删除生效项自动清除。
#[test]
fn active_subscription_sets_persists_and_clears_on_disable_or_remove() {
    let (dir, entries) = data_root();
    let mut app = start_app(entries.clone()).0;
    open_subscriptions(&mut app);
    add_subscription(&mut app, "甲", "https://example.com/a.yaml");
    add_subscription(&mut app, "乙", "https://example.com/b.yaml");

    app.press("id:subscription-row-甲 >> text:设为生效")
        .unwrap();
    app.wait_for("已设为当前生效订阅").unwrap();
    assert!(texts(&app).contains(&"当前生效"));
    // 停用的订阅不能设为生效。
    let enabled = format!("{} >> role:switch", enabled_selector(&dir, "甲"));
    app.press(&enabled).unwrap();
    app.wait_for("已停用").unwrap();
    let seen = texts(&app);
    assert!(!seen.contains(&"当前生效"), "{seen:?}");
    app.press("id:subscription-row-甲 >> text:设为生效")
        .unwrap();
    app.wait_for("text:/所选订阅已停用，请先启用/").unwrap();

    // 设为生效后重开仍在；删除生效项后清除。
    app.press(&enabled).unwrap();
    app.press("id:subscription-row-甲 >> text:设为生效")
        .unwrap();
    app.wait_for("当前生效").unwrap();
    drop(app);
    let mut reopened = start_app(entries.clone()).0;
    open_subscriptions(&mut reopened);
    reopened.wait_for("当前生效").unwrap();

    reopened
        .press("id:subscription-row-甲 >> text:更多")
        .unwrap();
    reopened.press("id:subscription-delete").unwrap();
    reopened.wait_for("删除订阅“甲”？").unwrap();
    reopened.press("确认删除").unwrap();
    let seen = texts(&reopened);
    assert!(!seen.contains(&"当前生效"), "{seen:?}");
    assert!(seen.contains(&"共 1 个订阅"), "{seen:?}");
    drop(reopened);

    // 清除也持久化。
    let mut third = start_app(entries).0;
    open_subscriptions(&mut third);
    third.wait_for("共 1 个订阅").unwrap();
    assert!(!texts(&third).contains(&"当前生效"), "{:?}", texts(&third));
}

/// 全局单飞：两个订阅同时点刷新，只有一个请求在途；busy 最终归零，
/// 完成后第二个订阅仍可刷新（不是永久禁用）。
#[test]
fn refresh_is_single_in_flight_globally_and_recovers_afterwards() {
    let server = TestServer::new(vec![
        TestRoute::delayed("/a.yaml", YAML, Duration::from_millis(600)),
        TestRoute::delayed("/b.yaml", YAML, Duration::from_millis(300)),
    ]);
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);
    add_subscription(&mut app, "甲", &server.url("/a.yaml"));
    app.wait_for_within("text:/刷新完成：2 个节点/", Duration::from_secs(10))
        .unwrap();
    add_subscription(&mut app, "乙", &server.url("/b.yaml"));
    app.wait_for_within("text:/刷新完成：2 个节点/", Duration::from_secs(10))
        .unwrap();
    assert_eq!(server.count_path("/a.yaml"), 1);
    assert_eq!(server.count_path("/b.yaml"), 1);

    // 甲刷新在途时点击乙刷新：被单飞拒绝，不产生第二个请求。
    app.press("id:subscription-row-甲 >> text:刷新").unwrap();
    app.wait_for("正在刷新订阅…").unwrap();
    app.press("id:subscription-row-乙 >> text:刷新").unwrap();
    app.wait_for_within("text:/刷新完成：2 个节点/", Duration::from_secs(10))
        .unwrap();
    assert_eq!(server.count_path("/a.yaml"), 2);
    assert_eq!(server.count_path("/b.yaml"), 1, "单飞拒绝第二个在途刷新");
    let seen = texts(&app);
    assert!(!seen.contains(&"正在刷新订阅…"), "{seen:?}");

    // 完成后第二个订阅可以刷新。按“刷新”会清掉旧完成提示，因此等到
    // 提示重新出现即本轮完成（等待过程持续驱动引擎）。
    app.press("id:subscription-row-乙 >> text:刷新").unwrap();
    app.wait_for("正在刷新订阅…").unwrap();
    app.wait_for_within("text:/刷新完成：2 个节点/", Duration::from_secs(10))
        .unwrap();
    assert_eq!(server.count_path("/b.yaml"), 2);
    let seen = texts(&app);
    assert!(!seen.contains(&"正在刷新订阅…"), "{seen:?}");
}

/// 启用开关的真实可访问性：role=switch、可访问名称、checked 状态如实
/// 暴露；键盘 Enter/Space 激活切换（不只鼠标/test_id）。Windows UIA 复验
/// 的 headless 对应。
#[test]
fn enabled_switch_has_role_label_checked_and_keyboard_activation() {
    let (dir, entries) = data_root();
    let mut app = common::start_app_desktop(entries).0;
    open_subscriptions(&mut app);
    add_subscription(&mut app, "开关订阅", "https://example.com/sw.yaml");
    let enabled = format!("{} >> role:switch", enabled_selector(&dir, "开关订阅"));
    // 真实 Switch 语义：角色 + 可访问名称 + checked 状态。
    assert!(app.screen().matches(&enabled).unwrap(), "role:switch 可达");
    assert!(
        app.screen()
            .matches("role:switch[启用订阅 开关订阅]")
            .unwrap(),
        "可访问名称随订阅名"
    );
    use nsh_core::Checked;
    assert_eq!(
        app.screen().accessibility_state(&enabled).unwrap().checked,
        Some(Checked::True),
        "初始 checked=true（已启用）"
    );

    // 键盘：桌面端指针按压不夺取键盘焦点；先经输入框取得确定焦点
    // （名称输入框），再 Tab 4 站（URL → User-Agent → 新增按钮 → 行内
    // 开关），Enter 与 Space 各自激活切换一次。
    app.type_text("id:subscription-name", "draft").unwrap();
    for _ in 0..4 {
        app.press_key("tab").unwrap();
    }
    app.press_key("enter").unwrap();
    app.wait_for("已停用").unwrap();
    app.press_key("space").unwrap();
    app.wait_for("已启用").unwrap();
    app.press_key("enter").unwrap();
    app.wait_for("已停用").unwrap();
    assert_eq!(
        app.screen().accessibility_state(&enabled).unwrap().checked,
        Some(Checked::False),
        "checked 状态随切换更新"
    );
}

/// 英文界面回归：订阅页主要动作、状态与启用开关可访问名称随语言切换，
/// 不出现本阶段新增的中文硬编码（订阅名等业务数据保持原样）。
#[test]
fn subscription_actions_and_switch_label_follow_english() {
    let (_dir, mut entries) = data_root();
    entries.insert("language".to_owned(), "en".to_owned());
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);
    app.type_text("id:subscription-name", "Lang sub").unwrap();
    app.type_text("id:subscription-url", "https://example.com/en.yaml")
        .unwrap();
    app.press("Add subscription").unwrap();

    let seen = texts(&app);
    for expected in [
        "Subscriptions",
        "Enabled",
        "Refresh",
        "Edit",
        "Source",
        "Set active",
        "More",
    ] {
        assert!(seen.contains(&expected), "missing {expected}: {seen:?}");
    }
    // 本阶段 UI 文案不得残留中文硬编码。
    for unexpected in ["刷新", "编辑", "设为生效", "更多", "已启用", "新增订阅"] {
        assert!(
            !seen.contains(&unexpected),
            "英文界面出现中文硬编码 {unexpected}: {seen:?}"
        );
    }
    // 启用开关可访问名称随语言切换。
    assert!(
        app.screen()
            .matches("role:switch[Enable subscription Lang sub]")
            .unwrap(),
        "英文可访问名称"
    );
}
