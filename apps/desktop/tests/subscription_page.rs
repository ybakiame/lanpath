//! Headless 测试（`nsh_testing::Headless`，NSH 0.5.0）：订阅页真实列表与持久化。
//! 每个场景使用独立临时数据根（见 `common::data_root`）。

mod common;

use common::{data_root, start_app, texts};

const PARSED: &str =
    "解析成功：2 个节点\n格式：ClashYaml\n警告：0 条\n\n东京测试节点\n新加坡测试节点";
const URL_ERROR: &str =
    "新增订阅失败:validation error: Subscription URL must start with http:// or https://";
const NAME_ERROR: &str = "新增订阅失败:validation error: 订阅名称不能为空";

/// 共享解析验证：固定测试订阅经共享 lanpath-client 解析成功。
#[test]
fn pressing_parse_verifies_the_shared_parser() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    app.wait_for("尚未解析共享订阅。").unwrap();
    app.press("解析共享订阅").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&PARSED), "{seen:?}");
}

/// 新数据根上订阅列表为空。
#[test]
fn fresh_data_root_shows_an_empty_subscription_list() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    app.wait_for("尚无订阅。").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"共 0 个订阅"), "{seen:?}");
}

/// 新增订阅：名称 + URL 提交后列表显示名称/启用状态/节点数，表单清空。
#[test]
fn adding_a_subscription_shows_it_and_clears_the_form() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    app.wait_for("尚无订阅。").unwrap();
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
    first.wait_for("尚无订阅。").unwrap();
    first.type_text("id:subscription-name", "持久订阅").unwrap();
    first
        .type_text("id:subscription-url", "https://example.com/p.yaml")
        .unwrap();
    first.press("新增订阅").unwrap();
    assert!(texts(&first).contains(&"持久订阅"));
    drop(first);

    let mut second = start_app(entries).0;
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
    app.wait_for("尚无订阅。").unwrap();
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
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    app.wait_for("尚无订阅。").unwrap();
    app.type_text("id:subscription-name", "切换订阅").unwrap();
    app.type_text("id:subscription-url", "https://example.com/t.yaml")
        .unwrap();
    app.press("新增订阅").unwrap();
    assert!(texts(&app).contains(&"已启用"));

    app.press("id:subscription-enabled-切换订阅").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"已停用"), "{seen:?}");
    assert!(!seen.contains(&"已启用"), "{seen:?}");

    app.press("id:subscription-enabled-切换订阅").unwrap();
    assert!(texts(&app).contains(&"已启用"));
}

/// 删除需经确认对话框，删除结果在同一数据根上持久化。
#[test]
fn deleting_a_subscription_needs_confirmation_and_persists() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries.clone()).0;
    app.wait_for("尚无订阅。").unwrap();
    app.type_text("id:subscription-name", "待删订阅").unwrap();
    app.type_text("id:subscription-url", "https://example.com/d.yaml")
        .unwrap();
    app.press("新增订阅").unwrap();
    assert!(texts(&app).contains(&"共 1 个订阅"));

    app.press("id:subscription-row-待删订阅 >> text:删除")
        .unwrap();
    assert!(texts(&app).contains(&"删除订阅“待删订阅”？"));
    app.press("确认删除").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"尚无订阅。"), "{seen:?}");
    assert!(seen.contains(&"共 0 个订阅"), "{seen:?}");
    drop(app);

    let mut reopened = start_app(entries).0;
    reopened.wait_for("尚无订阅。").unwrap();
}
