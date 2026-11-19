//! Headless 画廊测试（`nsh_testing::Headless`，NSH 0.5.1）：仅开发验收
//! 的组件画廊（`--ui-gallery` 同一根构建链）——分区完整性、交互状态、
//! 确认对话框、reduced motion 渲染与主题预览。

mod common;

use common::{start_gallery, texts};

/// 画廊覆盖全部验收分区（按钮/输入/选择/反馈/浮层/数据/主题/动效）。
#[test]
fn gallery_shows_every_section() {
    let (_dir, entries) = common::data_root();
    let mut app = start_gallery(entries).0;
    app.wait_for("组件画廊").unwrap();
    let seen = texts(&app);
    for expected in [
        "按钮",
        "输入与字段",
        "选择控件",
        "反馈",
        "浮层",
        "数据展示",
        "主题",
        "动效",
    ] {
        assert!(
            seen.iter().any(|text| text.contains(expected)),
            "missing {expected}: {seen:?}"
        );
    }
    // 13 个主题预览块。
    for theme in [
        "amethyst", "indigo", "ocean", "forest", "sand", "rose", "graphite", "cyan", "orange",
        "teal", "crimson", "gold", "slate",
    ] {
        assert!(seen.contains(&theme), "missing preview {theme}");
    }
}

/// 按钮状态真实可交互：切换“加载中”复选框后，loading 按钮在禁用/
/// 可用之间真实变化，spinner 也随信号出现/消失而不是只画首帧。
#[test]
fn gallery_button_loading_state_reacts() {
    let (_dir, entries) = common::data_root();
    let mut app = start_gallery(entries).0;
    app.wait_for("组件画廊").unwrap();
    // 初始：加载中开关开启（信号默认 true）——加载按钮禁用且显示指示器。
    assert!(!app.screen().enabled("role:button[加载中]").unwrap());
    assert!(
        app.screen()
            .matches("id:loading-button >> id:button-loading-indicator")
            .unwrap()
    );
    assert!(
        !app.screen()
            .matches("id:prefix-loading-button >> text:◆")
            .unwrap()
    );
    app.press("role:checkbox[加载中]").unwrap();
    assert!(app.screen().enabled("role:button[加载中]").unwrap());
    assert!(
        !app.screen()
            .matches("id:loading-button >> id:button-loading-indicator")
            .unwrap()
    );
    assert!(
        app.screen()
            .matches("id:prefix-loading-button >> text:◆")
            .unwrap(),
        "loading 结束后 prefix 必须恢复"
    );
    app.press("role:checkbox[加载中]").unwrap();
    assert!(!app.screen().enabled("role:button[加载中]").unwrap());
    assert!(
        app.screen()
            .matches("id:loading-button >> id:button-loading-indicator")
            .unwrap()
    );
    assert!(
        !app.screen()
            .matches("id:prefix-loading-button >> text:◆")
            .unwrap()
    );
    // 第二次 loading 结束后 prefix 仍由工厂重建，不是一次性元素。
    app.press("role:checkbox[加载中]").unwrap();
    assert!(
        app.screen()
            .matches("id:prefix-loading-button >> text:◆")
            .unwrap()
    );
}

/// 复选框有可见勾选标记与 checked 状态，不只依赖颜色。
#[test]
fn gallery_checkbox_shows_checked_state_and_mark() {
    let (_dir, entries) = common::data_root();
    let mut app = start_gallery(entries).0;
    app.wait_for("组件画廊").unwrap();
    let checkbox = "role:checkbox[加载中]";
    assert_eq!(
        app.screen().accessibility_state(checkbox).unwrap().checked,
        Some(nsh_core::Checked::True)
    );
    assert!(
        app.screen()
            .matches("role:checkbox[加载中] >> text:✓")
            .unwrap()
    );
    app.press(checkbox).unwrap();
    assert_eq!(
        app.screen().accessibility_state(checkbox).unwrap().checked,
        Some(nsh_core::Checked::False)
    );
    assert!(
        !app.screen()
            .matches("role:checkbox[加载中] >> text:✓")
            .unwrap()
    );
}

/// 画廊确认对话框：打开 → 内容呈现 → 确认回调执行（状态文本出现）。
#[test]
fn gallery_confirm_dialog_confirms() {
    let (_dir, entries) = common::data_root();
    let mut app = start_gallery(entries).0;
    app.wait_for("组件画廊").unwrap();
    app.press("role:button[打开对话框]").unwrap();
    app.wait_for("确认操作").unwrap();
    app.press("role:button[确认]").unwrap();
    assert!(texts(&app).contains(&"确认"));
    assert!(!app.screen().shows("确认操作"));
}

/// 画廊确认对话框：取消按钮真实关闭（内容消失，可再次打开）。
#[test]
fn gallery_confirm_dialog_cancel_closes() {
    let (_dir, entries) = common::data_root();
    let mut app = start_gallery(entries).0;
    app.wait_for("组件画廊").unwrap();
    app.press("role:button[打开对话框]").unwrap();
    app.wait_for("确认操作").unwrap();
    app.press("role:button[取消]").unwrap();
    assert!(!app.screen().shows("确认操作"));
    // 关闭后可再次打开（open 状态被真实复位）。
    app.press("role:button[打开对话框]").unwrap();
    app.wait_for("确认操作").unwrap();
}

/// 画廊确认对话框：Escape 经 NSH dialog 原生 on_cancel 关闭。
#[test]
fn gallery_confirm_dialog_escape_closes() {
    let (_dir, entries) = common::data_root();
    let mut app = start_gallery(entries).0;
    app.wait_for("组件画廊").unwrap();
    app.press("role:button[打开对话框]").unwrap();
    app.wait_for("确认操作").unwrap();
    app.press_key("escape").unwrap();
    assert!(!app.screen().shows("确认操作"));
}

/// 画廊主题卡片真实可点击：点击应用并持久化，勾选响应式移动。
#[test]
fn gallery_theme_card_applies_and_persists() {
    let (_dir, entries) = common::data_root();
    let (mut app, storage) = start_gallery(entries);
    app.wait_for("组件画廊").unwrap();
    assert!(
        app.screen()
            .matches("id:gallery-theme-amethyst >> text:✓")
            .unwrap()
    );
    assert!(
        app.screen()
            .selected("role:button[主题：amethyst]")
            .unwrap()
    );
    app.press("id:gallery-theme-ocean >> text:ocean").unwrap();
    assert_eq!(storage.get("theme"), Some("ocean".to_owned()));
    assert!(app.screen().selected("role:button[主题：ocean]").unwrap());
    assert!(
        app.screen()
            .matches("id:gallery-theme-ocean >> text:✓")
            .unwrap()
    );
    assert!(
        !app.screen()
            .matches("id:gallery-theme-amethyst >> text:✓")
            .unwrap()
    );
}

/// 画廊列表行选择：按下第三行后该行选中态切换。
#[test]
fn gallery_list_item_selects() {
    let (_dir, entries) = common::data_root();
    let mut app = start_gallery(entries).0;
    app.wait_for("组件画廊").unwrap();
    assert!(
        app.screen()
            .selected("id:list-item-1 >> role:button")
            .unwrap()
    );
    app.press("id:list-item-2 >> text:列表行 3").unwrap();
    assert!(
        app.screen()
            .selected("id:list-item-2 >> role:button")
            .unwrap()
    );
    assert!(
        !app.screen()
            .selected("id:list-item-1 >> role:button")
            .unwrap()
    );
}

/// reduced motion（引擎级）下画廊立即可读：动效分区显示减少动效说明，
/// 全部内容照常渲染。
#[test]
fn gallery_with_reduced_motion_stays_readable() {
    let (_dir, entries) = common::data_root();
    let storage = common::MemoryStorage::new(entries);
    let mut config = nsh_engine::EngineConfig {
        viewport: nsh_core::Size::new(nsh_core::Points(1100.0), nsh_core::Points(800.0)),
        storage: Some(storage.clone()),
        ..nsh_engine::EngineConfig::new(lanpath_desktop_nsh::gallery_root())
    };
    config.environment.reduce_motion = true;
    let mut app = nsh_testing::Headless::start(config).expect("start the headless gallery");
    app.wait_for("已按减少动效渲染（立即可读）").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"组件画廊"), "{seen:?}");
    assert!(seen.contains(&"已按减少动效渲染（立即可读）"), "{seen:?}");
    // reduced 下循环动效不启动：按下后状态仍可观察，但不显示持续旋转，
    // 内容与控件保持可读可用。
    app.press("role:button[动效演示（进入 / 按下 / 循环）]")
        .unwrap();
    assert!(!app.screen().shows("旋转中（循环播放）"));
    assert!(app.screen().shows("已按减少动效渲染（立即可读）"));
    assert!(
        app.screen()
            .enabled("role:button[动效演示（进入 / 按下 / 循环）]")
            .unwrap()
    );
}

/// 仅应用内“减少动效”偏好（引擎系统 flag 未开）也必须约束循环演示，
/// 不能只靠环境级 reduced motion。
#[test]
fn gallery_app_reduced_motion_preference_skips_looping_demo() {
    let (_dir, mut entries) = common::data_root();
    entries.insert("reduce_motion".to_owned(), "reduce".to_owned());
    let mut app = start_gallery(entries).0;
    app.wait_for("已按减少动效渲染（立即可读）").unwrap();
    app.press("role:button[动效演示（进入 / 按下 / 循环）]")
        .unwrap();
    assert!(!app.screen().shows("旋转中（循环播放）"));
    assert!(app.screen().shows("已按减少动效渲染（立即可读）"));
}

/// 画廊语言跟随存储偏好：预置 en 时分区标题为英文。
#[test]
fn gallery_language_follows_stored_preference() {
    let (_dir, mut entries) = common::data_root();
    entries.insert("language".to_owned(), "en".to_owned());
    let mut app = start_gallery(entries).0;
    app.wait_for("Component Gallery").unwrap();
    let seen = texts(&app);
    assert!(seen.iter().any(|text| text.contains("Buttons")), "{seen:?}");
    assert!(
        seen.iter().any(|text| text.contains("MotionTokens")),
        "{seen:?}"
    );
    assert!(
        seen.contains(&"English"),
        "select must show its value: {seen:?}"
    );
    assert!(
        seen.contains(&"Use intent-stable keys; test IDs are never translated"),
        "field hint must use the active locale: {seen:?}"
    );
    app.press("role:combobox").unwrap();
    app.press("text:Simplified Chinese").unwrap();
    app.wait_for("使用意图稳定的 key，不翻译测试 ID").unwrap();
    assert!(app.screen().shows("组件画廊"));
}
