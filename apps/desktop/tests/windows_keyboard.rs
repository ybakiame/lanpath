//! Windows environment regression: custom controls participate in native-style Tab navigation.
mod common;

use nsh_core::{
    Points, Size,
    platform::{Idiom, Os},
};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

#[test]
fn windows_tab_space_and_enter_activate_the_shared_button() {
    let (_dir, entries) = common::data_root();
    let mut config = EngineConfig {
        storage: Some(common::MemoryStorage::new(entries)),
        viewport: Size::new(Points(1180.0), Points(800.0)),
        ..EngineConfig::new(lanpath_desktop_nsh::root())
    };
    config.environment.os = Os::Windows;
    config.environment.idiom = Idiom::Desktop;
    let mut app = Headless::start(config).unwrap();
    common::open_subscriptions(&mut app);
    for (name, key) in [
        ("space-subscription", "space"),
        ("enter-subscription", "enter"),
    ] {
        app.type_text("id:subscription-name", name).unwrap();
        app.type_text("id:subscription-url", "https://example.com/keyboard.yaml")
            .unwrap();
        app.type_text("id:subscription-user-agent", "key").unwrap();
        app.press_key("tab").unwrap();
        app.press_key("shift+tab").unwrap();
        app.press_key("x").unwrap();
        assert!(
            app.screen()
                .input_text("id:subscription-user-agent")
                .unwrap()
                .is_some_and(|value| value.ends_with('x')),
            "Shift+Tab returns to the user-agent input"
        );
        app.press_key("tab").unwrap();
        app.press_key(key).unwrap();
        app.wait_for(name).unwrap();
    }
    app.wait_for("共 2 个订阅").unwrap();
}
