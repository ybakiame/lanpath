//! Headless 测试（`nsh_testing::Headless`，NSH 0.5.0）：侧栏导航、外观页
//! 多主题/模式/语言/动效的真实切换、持久化与恢复、语言切换不重建
//! 业务状态。每个场景使用独立临时数据根（界面语言预置 zh-CN）。

mod common;

use nsh_engine::Environment;

use common::{data_root, start_app, texts};

/// 侧栏在订阅页与外观页之间导航，两页内容互不串扰；导航项有稳定
/// test_id（不依赖翻译文案）。
#[test]
fn navigating_between_the_two_sidebar_pages() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    app.wait_for("解析共享订阅").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"订阅"), "{seen:?}");
    assert!(app.screen().shows("解析共享订阅"));
    assert!(!app.screen().matches("id:appearance-light").unwrap());

    app.press("role:button[外观]").unwrap();
    let seen = texts(&app);
    for expected in ["浅色", "深色", "跟随系统"] {
        assert!(seen.contains(&expected), "missing {expected}: {seen:?}");
    }
    // 13 个主题预览网格。
    for theme in [
        "amethyst", "indigo", "ocean", "forest", "sand", "rose", "graphite", "cyan", "orange",
        "teal", "crimson", "gold", "slate",
    ] {
        assert!(
            app.screen().matches(&format!("id:theme-{theme}")).unwrap(),
            "missing theme {theme}"
        );
    }
    assert!(!app.screen().shows("解析共享订阅"));
    assert!(app.screen().matches("id:appearance-light").unwrap());
    // 选中主题同时显示名称与勾选（默认 amethyst 选中）。
    assert!(app.screen().matches("id:theme-amethyst >> text:✓").unwrap());
    assert!(!app.screen().matches("id:theme-ocean >> text:✓").unwrap());

    app.press("role:button[订阅]").unwrap();
    assert!(app.screen().shows("解析共享订阅"));
    assert!(!app.screen().matches("id:appearance-light").unwrap());
}

/// 浅色 / 深色 / 跟随系统真实切换：选择立即生效并持久化；系统外观
/// 变化只影响“跟随系统”。
#[test]
fn switching_appearance_light_dark_and_system_for_real() {
    let (_dir, entries) = data_root();
    let (mut app, storage) = start_app(entries);
    app.wait_for("解析共享订阅").unwrap();
    app.press("role:button[外观]").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"已选择：amethyst"), "{seen:?}");
    assert!(seen.contains(&"当前生效：浅色"), "{seen:?}");

    // 选深色：立即生效并持久化（存储键沿用既有 `appearance`）。
    app.press("id:appearance-dark >> text:深色").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"当前生效：深色"), "{seen:?}");
    assert_eq!(storage.get("appearance"), Some("dark".to_owned()));
    set_color_scheme(&mut app, "light");
    assert!(texts(&app).contains(&"当前生效：深色"));

    // 跟随系统：系统外观决定生效值。
    app.press("id:appearance-system >> text:跟随系统").unwrap();
    assert_eq!(storage.get("appearance"), Some("system".to_owned()));
    set_color_scheme(&mut app, "dark");
    assert!(texts(&app).contains(&"当前生效：深色"));
    set_color_scheme(&mut app, "light");
    assert!(texts(&app).contains(&"当前生效：浅色"));

    // 选浅色：立即生效并持久化。
    app.press("id:appearance-light >> text:浅色").unwrap();
    assert!(texts(&app).contains(&"当前生效：浅色"));
    assert_eq!(storage.get("appearance"), Some("light".to_owned()));
}

/// 主题网格点击即应用并持久化；默认主题为 amethyst。
#[test]
fn switching_theme_applies_and_persists() {
    let (_dir, entries) = data_root();
    let (mut app, storage) = start_app(entries);
    app.wait_for("解析共享订阅").unwrap();
    app.press("role:button[外观]").unwrap();
    assert_eq!(storage.get("theme"), None, "未选择时无 theme 键");

    app.press("id:theme-ocean >> text:ocean").unwrap();
    assert_eq!(storage.get("theme"), Some("ocean".to_owned()));
    assert!(texts(&app).contains(&"已选择：ocean"));
    // 勾选随选择响应式移动，不只靠边框。
    assert!(app.screen().matches("id:theme-ocean >> text:✓").unwrap());
    assert!(!app.screen().matches("id:theme-amethyst >> text:✓").unwrap());

    app.press("id:theme-graphite >> text:graphite").unwrap();
    assert_eq!(storage.get("theme"), Some("graphite".to_owned()));
    assert!(texts(&app).contains(&"已选择：graphite"));
    assert!(app.screen().matches("id:theme-graphite >> text:✓").unwrap());
    assert!(!app.screen().matches("id:theme-ocean >> text:✓").unwrap());
}

/// 启动时恢复 storage 预置的外观选择（主题 ocean + 深色 + 减少动效）。
#[test]
fn restores_the_persisted_appearance_choices() {
    let (_dir, mut entries) = data_root();
    entries.insert("theme".to_owned(), "ocean".to_owned());
    entries.insert("appearance".to_owned(), "dark".to_owned());
    entries.insert("reduce_motion".to_owned(), "reduce".to_owned());
    let mut app = start_app(entries).0;
    app.wait_for("解析共享订阅").unwrap();
    app.press("role:button[外观]").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"已选择：ocean"), "{seen:?}");
    assert!(seen.contains(&"当前生效：深色"), "{seen:?}");
    assert!(
        app.screen()
            .selected("id:motion-group >> role:radio[减少动效]")
            .unwrap()
    );
}

/// 界面语言经外观页选择器切换：立即生效、持久化，且导航与业务页
/// 状态不重建（订阅页内容在切换前后保持）。
#[test]
fn switching_language_applies_without_rebuilding_state() {
    let (_dir, entries) = data_root();
    let (mut app, storage) = start_app(entries);
    app.wait_for("解析共享订阅").unwrap();
    // 先证明业务页在中文下内容存在。
    app.press("role:button[订阅]").unwrap();
    assert!(app.screen().shows("解析共享订阅"));

    app.press("role:button[外观]").unwrap();
    app.press("id:language-en >> text:English").unwrap();
    assert_eq!(storage.get("language"), Some("en".to_owned()));
    // 导航立即变为英文。
    assert!(app.screen().shows("Appearance"));
    assert!(app.screen().shows("Subscriptions"));

    // 业务页状态未重建：切回订阅页，共享解析验证仍在。
    app.press("role:button[Subscriptions]").unwrap();
    assert!(app.screen().shows("解析共享订阅"));

    // 切回中文（此时导航仍为英文）。
    app.press("role:button[Appearance]").unwrap();
    app.press("id:language-zh-cn >> text:Simplified Chinese")
        .unwrap();
    assert_eq!(storage.get("language"), Some("zh-CN".to_owned()));
    assert!(app.screen().shows("外观"));
}

/// 动效偏好持久化；减少动效选项真实写入 storage。
#[test]
fn switching_motion_preference_persists() {
    let (_dir, entries) = data_root();
    let (mut app, storage) = start_app(entries);
    app.wait_for("解析共享订阅").unwrap();
    app.press("role:button[外观]").unwrap();
    assert!(
        app.screen()
            .selected("id:motion-group >> role:radio[跟随系统]")
            .unwrap()
    );

    app.press("id:motion-reduce >> text:减少动效").unwrap();
    assert_eq!(storage.get("reduce_motion"), Some("reduce".to_owned()));
    app.press("id:motion-system >> text:跟随系统").unwrap();
    assert_eq!(storage.get("reduce_motion"), Some("system".to_owned()));
}

/// 如平台报告系统外观变化（TS 侧 `app.setColorScheme` 的等价步骤）。
fn set_color_scheme(app: &mut nsh_testing::Headless, scheme: &str) {
    app.set_environment(
        &format!("setColorScheme({scheme})"),
        |env: &mut Environment| {
            env.color_scheme = match scheme {
                "dark" => nsh_core::platform::ColorScheme::Dark,
                _ => nsh_core::platform::ColorScheme::Light,
            };
        },
    )
    .unwrap();
}
