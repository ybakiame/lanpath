//! Headless test of the root (`nsh_testing::Headless`, NSH 0.5.0): pressing
//! “解析共享订阅” parses the fixture through the shared `lanpath-client` code.

use nsh_core::element::Root;
use nsh_engine::EngineConfig;
use nsh_testing::Headless;

const PARSED: &str =
    "解析成功：2 个节点\n格式：ClashYaml\n警告：0 条\n\n东京测试节点\n新加坡测试节点";

#[test]
fn pressing_parse_parses_the_shared_subscription() {
    let mut app =
        Headless::start(EngineConfig::new(Root::of::<nsh_app_lanpath_nsh::App>())).unwrap();
    app.wait_for("尚未解析共享订阅。").unwrap();
    app.press("解析共享订阅").unwrap();
    let texts = app.screen().texts();
    assert!(texts.contains(&PARSED), "{texts:?}");
}
