//! 「连接」监控页与概览图表端到端集成测试：
//! 概览实时流量图/代理比例环/7日柱状图渲染、核心启动后拉取活跃连接与命中规则、
//! 搜索与出站筛选、一键写入直连规则（持久化到 rules.toml + Toast 反馈）、
//! 跳转自定义规则预填以及单条/全部断开连接。

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
fn connections_page_tracks_live_traffic_filters_adds_quick_rules_and_closes() {
    let (dir, entries) = data_root();
    let mixed_port = free_port();
    let api_port = free_port();
    init_root_then(&entries, dir.path(), |root| {
        seed_runtime_target(root, CoreType::SingBox, "1.14.0", mixed_port, api_port);
    });

    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);

    // 1. 概览页已替换为实时流量图、代理直连比例环形图、近 7 日柱状图与快捷入口。
    app.wait_for("id:overview-traffic-chart").unwrap();
    app.wait_for("id:overview-ratio-card").unwrap();
    app.wait_for("id:overview-ratio-canvas").unwrap();
    app.wait_for("id:overview-daily-chart").unwrap();

    // 2. 启动核心并经概览快捷按钮进入「连接」页。
    app.press("id:overview-start").unwrap();
    app.wait_for_within("id:overview-stop", Duration::from_secs(20))
        .unwrap();
    app.press("id:overview-goto-connections").unwrap();

    // 3. 验证实时活跃连接、命中规则与进程展示。
    app.wait_for_within("id:connection-row-conn-gh-1", Duration::from_secs(10))
        .unwrap();
    app.wait_for("github.com").unwrap();
    app.wait_for("DOMAIN-SUFFIX · github.com").unwrap();
    app.wait_for("id:connection-row-conn-dns-2").unwrap();

    // 4. 一键添加到直连规则：写入 rules.toml 顶部并弹出 Toast 反馈。
    app.press("id:connections-rule-direct-conn-gh-1").unwrap();
    app.wait_for("已添加规则：suffix github.com -> direct")
        .unwrap();
    let saved_rules = lanpath_client::rules::load_rules(dir.path()).unwrap();
    assert!(
        saved_rules
            .iter()
            .any(|r| r.target == "github.com"
                && r.action == lanpath_client::rules::RuleAction::Direct),
        "rules.toml 应包含一键添加的 github.com 直连规则，实际：{saved_rules:?}"
    );

    // 重复点击不产生重复规则并提示已存在。
    app.press("id:connections-rule-direct-conn-gh-1").unwrap();
    app.wait_for("规则已存在：suffix github.com -> direct")
        .unwrap();
    let after_dup = lanpath_client::rules::load_rules(dir.path()).unwrap();
    assert_eq!(saved_rules.len(), after_dup.len());

    // 5. 搜索过滤仅保留匹配连接，清空后恢复。
    app.type_text("id:connections-search", "114.114").unwrap();
    app.wait_for("id:connection-row-conn-dns-2").unwrap();
    app.clear("id:connections-search").unwrap();
    app.wait_for("id:connection-row-conn-gh-1").unwrap();

    // 6. 断开单条连接并验证 Toast 反馈。
    app.press("id:connections-close-conn-gh-1").unwrap();
    app.wait_for("已断开指定连接。").unwrap();

    // 7. 断开剩余全部连接，再回到概览停止核心。
    app.press("id:connections-close-all").unwrap();
    app.wait_for("已断开全部活跃连接。").unwrap();

    common::navigate(&mut app, "概览");
    app.wait_for("id:overview-stop").unwrap();
    app.press("id:overview-stop").unwrap();
    app.wait_for_within("id:overview-start", Duration::from_secs(15))
        .unwrap();
}
