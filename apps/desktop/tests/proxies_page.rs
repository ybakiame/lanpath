//! 运行时「策略与节点」页端到端集成测试：
//! 未运行空态与模式持久化、核心启动后自动拉取策略组、实时切换节点并写入 app.toml、
//! 单节点与组内延迟测速及运行模式热切换。

mod common;

use std::net::TcpListener;
use std::time::Duration;

use common::{data_root, init_root_then, seed_runtime_target, start_app_desktop as start_app};
use lanpath_common::CoreType;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[test]
fn proxies_page_handles_stopped_state_live_switching_delay_tests_and_mode_change() {
    let (dir, entries) = data_root();
    let mixed_port = free_port();
    let api_port = free_port();
    init_root_then(&entries, dir.path(), |root| {
        seed_runtime_target(root, CoreType::SingBox, "1.14.0", mixed_port, api_port);
    });

    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);

    // 1. 核心未运行时进入「策略与节点」页：显示未运行引导，并可预切模式持久化。
    common::navigate(&mut app, "策略与节点");
    app.wait_for("id:proxies-go-overview").unwrap();
    app.press("id:proxies-mode-direct >> text:直连模式")
        .unwrap();
    app.wait_for("运行模式已切换为 直连模式。").unwrap();
    let saved_cfg = lanpath_client::ClientConfig::load_for_requests(dir.path()).unwrap();
    assert_eq!(saved_cfg.rule_mode, "direct");

    // 2. 切回规则模式并前往概览启动核心。
    app.press("id:proxies-mode-rule >> text:规则模式").unwrap();
    app.wait_for("运行模式已切换为 规则模式。").unwrap();
    app.press("id:proxies-go-overview").unwrap();
    app.wait_for("id:overview-start").unwrap();
    app.press("id:overview-start").unwrap();
    app.wait_for_within("id:overview-stop", Duration::from_secs(20))
        .unwrap();

    // 3. 返回「策略与节点」页：自动拉取运行中核心的策略组（proxy / auto / final）。
    common::navigate(&mut app, "策略与节点");
    app.wait_for_within("id:proxy-group-tab-proxy", Duration::from_secs(10))
        .unwrap();
    app.wait_for("id:proxy-select-节点甲").unwrap();

    // 4. 在 proxy 策略组中选择「节点乙」，验证界面反馈与 app.toml group_selections 持久化。
    app.press("id:proxy-select-节点乙").unwrap();
    app.wait_for("已将 proxy 切换为 节点乙（已记住下次启动回放）。")
        .unwrap();
    let saved_after_select = lanpath_client::ClientConfig::load_for_requests(dir.path()).unwrap();
    assert_eq!(
        saved_after_select
            .group_selections
            .get("proxy")
            .map(String::as_str),
        Some("节点乙")
    );

    // 5. 执行单节点测速与组内测速，显示真实延迟徽章。
    app.press("id:proxy-test-节点甲").unwrap();
    app.wait_for("42 ms").unwrap();
    app.press("id:proxies-test-group").unwrap();
    app.wait_for_within("proxy 测速完成：3 / 3 项可用。", Duration::from_secs(10))
        .unwrap();

    // 6. 运行中热切换全局模式，并停止核心确认回到未运行态。
    app.press("id:proxies-mode-global >> text:全局模式")
        .unwrap();
    app.wait_for("运行模式已切换为 全局模式。").unwrap();

    common::navigate(&mut app, "概览");
    app.wait_for("id:overview-stop").unwrap();
    app.press("id:overview-stop").unwrap();
    app.wait_for_within("id:overview-start", Duration::from_secs(15))
        .unwrap();
}
