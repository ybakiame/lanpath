//! Headless 测试（`nsh_testing::Headless`）：侧栏导航、外观页
//! 多主题/模式/语言/动效的真实切换、持久化与恢复、语言切换不重建
//! 业务状态。每个场景使用独立临时数据根（界面语言预置 zh-CN）。

mod common;

use nsh_engine::Environment;

use common::{data_root, start_app, start_app_desktop, texts};

/// 侧栏在订阅页与外观页之间导航，两页内容互不串扰；导航项有稳定
/// test_id（不依赖翻译文案）。
#[test]
fn navigating_between_the_two_sidebar_pages() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    app.wait_for("开始之前").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"订阅"), "{seen:?}");
    // 默认首页是概览：未配置时只显示步骤（添加订阅为当前可执行步）。
    assert!(app.screen().shows("开始之前"));
    assert!(app.screen().shows("选择核心"));
    assert!(!app.screen().matches("id:appearance-light").unwrap());

    common::navigate(&mut app, "role:button[外观]");
    let seen = texts(&app);
    for expected in ["浅色", "深色", "跟随系统"] {
        assert!(seen.contains(&expected), "missing {expected}: {seen:?}");
    }
    // 14 个主题预览网格（含高饱和清新主题 mint）。
    for theme in [
        "amethyst", "indigo", "ocean", "mint", "forest", "sand", "rose", "graphite", "cyan",
        "orange", "teal", "crimson", "gold", "slate",
    ] {
        assert!(
            app.screen().matches(&format!("id:theme-{theme}")).unwrap(),
            "missing theme {theme}"
        );
    }
    assert!(!app.screen().shows("开始之前"));
    assert!(app.screen().matches("id:appearance-light").unwrap());
    // 新数据根默认 Ocean；已有已保存主题继续保留。
    assert!(app.screen().matches("id:theme-ocean >> text:✓").unwrap());
    assert!(!app.screen().matches("id:theme-amethyst >> text:✓").unwrap());

    common::open_subscriptions(&mut app);
    app.wait_for("id:subscription-name").unwrap();
    assert!(!app.screen().matches("id:appearance-light").unwrap());
}

/// 浅色 / 深色 / 跟随系统真实切换：选择立即生效并持久化；系统外观
/// 变化只影响“跟随系统”。
#[test]
fn switching_appearance_light_dark_and_system_for_real() {
    let (_dir, entries) = data_root();
    let (mut app, storage) = start_app(entries);
    app.wait_for("开始之前").unwrap();
    common::navigate(&mut app, "role:button[外观]");
    let light = app
        .screen()
        .view_style("id:app-sidebar")
        .unwrap()
        .unwrap()
        .background_color;
    app.press("id:appearance-dark >> text:深色").unwrap();
    let dark = app
        .screen()
        .view_style("id:app-sidebar")
        .unwrap()
        .unwrap()
        .background_color;
    assert_ne!(light, dark);
    assert_eq!(storage.get("appearance"), Some("dark".into()));
    set_color_scheme(&mut app, "light");
    assert_eq!(
        app.screen()
            .view_style("id:app-sidebar")
            .unwrap()
            .unwrap()
            .background_color,
        dark
    );
    app.press("id:appearance-system >> text:跟随系统").unwrap();
    assert_eq!(storage.get("appearance"), Some("system".into()));
    set_color_scheme(&mut app, "dark");
    assert_eq!(
        app.screen()
            .view_style("id:app-sidebar")
            .unwrap()
            .unwrap()
            .background_color,
        dark
    );
    set_color_scheme(&mut app, "light");
    assert_eq!(
        app.screen()
            .view_style("id:app-sidebar")
            .unwrap()
            .unwrap()
            .background_color,
        light
    );
    app.press("id:appearance-light >> text:浅色").unwrap();
    assert_eq!(storage.get("appearance"), Some("light".into()));
}

/// 主题网格点击即应用并持久化；默认主题为 amethyst。
#[test]
fn switching_theme_applies_and_persists() {
    let (_dir, entries) = data_root();
    let (mut app, storage) = start_app(entries);
    app.wait_for("开始之前").unwrap();
    common::navigate(&mut app, "role:button[外观]");
    assert_eq!(storage.get("theme"), None, "未选择时无 theme 键");

    app.press("id:theme-ocean >> text:ocean").unwrap();
    assert_eq!(storage.get("theme"), Some("ocean".to_owned()));
    // 勾选随选择响应式移动，不只靠边框。
    assert!(app.screen().matches("id:theme-ocean >> text:✓").unwrap());
    assert!(!app.screen().matches("id:theme-amethyst >> text:✓").unwrap());

    app.press("id:theme-graphite >> text:graphite").unwrap();
    assert_eq!(storage.get("theme"), Some("graphite".to_owned()));
    assert!(app.screen().matches("id:theme-graphite >> text:✓").unwrap());
    assert!(!app.screen().matches("id:theme-ocean >> text:✓").unwrap());
}

/// 主题卡暴露可访问名称与选中语义，读屏不只依赖 ✓ 图形。
#[test]
fn theme_cards_expose_selected_state_and_names() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    app.wait_for("开始之前").unwrap();
    common::navigate(&mut app, "role:button[外观]");
    let amethyst = "role:button[主题：amethyst]";
    let ocean = "role:button[主题：ocean]";
    assert!(app.screen().selected(ocean).unwrap());
    assert!(!app.screen().selected(amethyst).unwrap());
    app.press("id:theme-amethyst >> text:amethyst").unwrap();
    assert!(app.screen().selected(amethyst).unwrap());
    assert!(!app.screen().selected(ocean).unwrap());
}

/// 指针悬停只预览不写盘；A→B→离开后回已保存主题，点击才提交；
/// 悬停中离页由页面清理恢复 confirmed 主题。
#[test]
fn theme_hover_preview_is_transient_and_cleans_up_on_leave() {
    let (_dir, entries) = data_root();
    let (mut app, storage) = start_app_desktop(entries);
    app.wait_for("开始之前").unwrap();
    common::navigate(&mut app, "role:button[外观]");
    app.hover("id:theme-indigo").unwrap();
    app.wait_for("正在预览：indigo；点击应用并保存").unwrap();
    assert_eq!(storage.get("theme"), None, "hover 不得写 storage");
    app.hover("id:theme-amethyst").unwrap();
    app.wait_for("正在预览：amethyst；点击应用并保存").unwrap();
    assert_eq!(storage.get("theme"), None);
    app.hover("id:appearance-light").unwrap();
    app.wait_for("指向主题可预览，点击应用并保存").unwrap();
    assert_eq!(storage.get("theme"), None);

    app.hover("id:theme-indigo").unwrap();
    app.wait_for("正在预览：indigo；点击应用并保存").unwrap();
    app.press("id:theme-ocean >> text:ocean").unwrap();
    assert_eq!(storage.get("theme"), Some("ocean".to_owned()));
    assert!(app.screen().selected("role:button[主题：ocean]").unwrap());

    // 悬停后离页：页面 owner 清理，不把临时预览写盘或留在选中态。
    app.hover("id:theme-forest").unwrap();
    app.wait_for("正在预览：forest；点击应用并保存").unwrap();
    common::open_subscriptions(&mut app);
    app.wait_for("id:subscription-name").unwrap();
    assert_eq!(storage.get("theme"), Some("ocean".to_owned()));
    common::navigate(&mut app, "role:button[外观]");
    assert!(app.screen().selected("role:button[主题：ocean]").unwrap());
    assert!(app.screen().selected("role:button[主题：ocean]").unwrap());
    assert!(!app.screen().selected("role:button[主题：forest]").unwrap());
}

/// storage 配额拒绝写入时，四类外观偏好都不改变信号/运行时且显示可恢复
/// 错误；用户看到“已选择”和实际重启恢复值不再背离。
#[test]
fn appearance_save_failure_keeps_previous_values_and_reports() {
    let (_dir, mut entries) = data_root();
    entries.insert(
        "filler".to_owned(),
        "x".repeat(nsh_core::storage::QUOTA - "filler".len()),
    );
    let (mut app, storage) = start_app(entries);
    app.wait_for("开始之前").unwrap();
    common::navigate(&mut app, "role:button[外观]");
    let failure = "外观设置未能保存，仍使用上一有效值；请释放应用存储空间后重试。";

    app.press("id:theme-indigo >> text:indigo").unwrap();
    assert_eq!(storage.get("theme"), None);
    assert!(app.screen().selected("role:button[主题：ocean]").unwrap());
    assert!(texts(&app).contains(&failure));

    app.press("id:appearance-dark >> text:深色").unwrap();
    assert_eq!(storage.get("appearance"), None);
    assert!(app.screen().selected("id:appearance-system").unwrap());

    app.press("id:language-en >> text:English").unwrap();
    assert_eq!(storage.get("language"), Some("zh-CN".to_owned()));
    assert!(app.screen().shows("外观"));

    app.press("id:motion-reduce >> text:减少动效").unwrap();
    assert_eq!(storage.get("reduce_motion"), None);
    assert!(
        app.screen()
            .selected("id:motion-group >> role:radio[跟随系统]")
            .unwrap()
    );
}

/// 启动时恢复 storage 预置的外观选择（主题 ocean + 深色 + 减少动效）。
#[test]
fn restores_the_persisted_appearance_choices() {
    let (_dir, mut entries) = data_root();
    entries.insert("theme".to_owned(), "ocean".to_owned());
    entries.insert("appearance".to_owned(), "dark".to_owned());
    entries.insert("reduce_motion".to_owned(), "reduce".to_owned());
    let mut app = start_app(entries).0;
    app.wait_for("开始之前").unwrap();
    common::navigate(&mut app, "role:button[外观]");
    assert!(app.screen().selected("role:button[主题：ocean]").unwrap());
    assert!(app.screen().selected("id:appearance-dark").unwrap());
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
    app.wait_for("开始之前").unwrap();
    // 先证明业务页在中文下内容存在，并添加一条真实订阅验证切换后仍在。
    common::open_subscriptions(&mut app);
    app.wait_for("id:subscription-name").unwrap();
    app.type_text("id:subscription-name", "语言订阅").unwrap();
    app.type_text("id:subscription-url", "https://example.com/lang.yaml")
        .unwrap();
    app.press("新增订阅").unwrap();
    assert!(texts(&app).contains(&"语言订阅"));

    common::navigate(&mut app, "role:button[外观]");
    app.press("id:language-en >> text:English").unwrap();
    assert_eq!(storage.get("language"), Some("en".to_owned()));
    // 导航立即变为英文。
    assert!(app.screen().shows("Appearance"));
    assert!(app.screen().shows("Overview"));

    // 业务页状态未重建：切回订阅页，真实列表内容仍在。
    common::open_subscriptions(&mut app);
    let seen = texts(&app);

    assert!(seen.contains(&"语言订阅"), "{seen:?}");

    // 切回中文（此时导航仍为英文）。
    common::navigate(&mut app, "role:button[Appearance]");
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
    app.wait_for("开始之前").unwrap();
    common::navigate(&mut app, "role:button[外观]");
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

/// 窗口背景材质（亚克力 / 云母 / 不透明）切换持久化，并真实改变表面透明度。
#[test]
fn switching_window_backdrop_persists_and_updates_surface() {
    let (_dir, entries) = data_root();
    let (mut app, storage) = start_app(entries);
    app.wait_for("开始之前").unwrap();
    common::navigate(&mut app, "role:button[外观]");
    assert!(app.screen().matches("id:backdrop-group").unwrap());
    assert!(app.screen().matches("id:backdrop-opacity-slider").unwrap());

    let acrylic_bg = app
        .screen()
        .view_style("id:app-sidebar")
        .unwrap()
        .unwrap()
        .background_color
        .unwrap();

    app.press("id:backdrop-solid >> text:纯色不透明").unwrap();
    assert_eq!(storage.get("window_material"), Some("solid".to_owned()));
    let solid_bg = app
        .screen()
        .view_style("id:app-sidebar")
        .unwrap()
        .unwrap()
        .background_color
        .unwrap();
    assert_ne!(
        acrylic_bg, solid_bg,
        "solid and acrylic surfaces should have distinct alpha/color"
    );

    app.press("id:backdrop-mica >> text:云母 Mica").unwrap();
    assert_eq!(storage.get("window_material"), Some("mica".to_owned()));

    app.press("id:backdrop-acrylic >> text:亚克力磨砂").unwrap();
    assert_eq!(storage.get("window_material"), Some("acrylic".to_owned()));
    assert_eq!(
        app.screen()
            .view_style("id:app-sidebar")
            .unwrap()
            .unwrap()
            .background_color,
        Some(acrylic_bg)
    );
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
