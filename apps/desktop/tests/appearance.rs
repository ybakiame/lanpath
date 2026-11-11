//! Headless 测试（`nsh_testing::Headless`，NSH 0.5.0）：侧栏导航与外观页
//! 真实切换、持久化与恢复。每个场景使用独立临时数据根。

mod common;

use nsh_engine::Environment;

use common::{data_root, start_app, texts};

/// 侧栏在订阅页与外观页之间导航，两页内容互不串扰。
#[test]
fn navigating_between_the_two_sidebar_pages() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    app.wait_for("解析共享订阅").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"订阅"), "{seen:?}");
    assert!(app.screen().shows("解析共享订阅"));
    assert!(!app.screen().matches("id:appearance-light").unwrap());

    app.press("外观").unwrap();
    let seen = texts(&app);
    for expected in ["浅色", "深色", "跟随系统"] {
        assert!(seen.contains(&expected), "missing {expected}: {seen:?}");
    }
    assert!(!app.screen().shows("解析共享订阅"));
    assert!(app.screen().matches("id:appearance-light").unwrap());

    app.press("订阅").unwrap();
    assert!(app.screen().shows("解析共享订阅"));
    assert!(!app.screen().matches("id:appearance-light").unwrap());
}

/// 浅色 / 深色 / 跟随系统真实切换：选择立即生效，写入 NSH storage；
/// 系统外观变化只影响“跟随系统”。
#[test]
fn switching_appearance_light_dark_and_system_for_real() {
    let (_dir, entries) = data_root();
    let (mut app, storage) = start_app(entries);
    app.wait_for("解析共享订阅").unwrap();
    app.press("外观").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"已选择：跟随系统"), "{seen:?}");
    assert!(seen.contains(&"当前生效：浅色"), "{seen:?}");

    // 选深色：立即生效并持久化。
    app.press("id:appearance-dark").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"已选择：深色"), "{seen:?}");
    assert!(seen.contains(&"当前生效：深色"), "{seen:?}");
    assert_eq!(storage.get("appearance"), Some("dark".to_owned()));
    // 系统外观变化不影响已选的深色。
    set_color_scheme(&mut app, "light");
    assert!(texts(&app).contains(&"当前生效：深色"));

    // 跟随系统：系统外观决定生效值。
    app.press("id:appearance-system").unwrap();
    assert!(texts(&app).contains(&"已选择：跟随系统"));
    assert_eq!(storage.get("appearance"), Some("system".to_owned()));
    set_color_scheme(&mut app, "dark");
    assert!(texts(&app).contains(&"当前生效：深色"));
    set_color_scheme(&mut app, "light");
    assert!(texts(&app).contains(&"当前生效：浅色"));

    // 选浅色：立即生效并持久化。
    app.press("id:appearance-light").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"已选择：浅色"), "{seen:?}");
    assert!(seen.contains(&"当前生效：浅色"), "{seen:?}");
    assert_eq!(storage.get("appearance"), Some("light".to_owned()));
}

/// 启动时恢复 storage 预置的外观选择（深色）。
#[test]
fn restores_the_persisted_appearance_choice() {
    let (_dir, mut entries) = data_root();
    entries.insert("appearance".to_owned(), "dark".to_owned());
    let mut app = start_app(entries).0;
    app.wait_for("解析共享订阅").unwrap();
    app.press("外观").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"已选择：深色"), "{seen:?}");
    assert!(seen.contains(&"当前生效：深色"), "{seen:?}");
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
