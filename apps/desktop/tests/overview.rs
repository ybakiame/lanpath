//! Headless 测试（`nsh_testing::Headless`）：概览默认首页、
//! 未配置步骤只高亮当前可执行步、真实订阅/核心摘要与全局状态栏共享、
//! 跨页启动/停止任务存活且状态一致、概览操作导航到现有页。
//! 每个场景使用独立临时数据根（见 `common::data_root`）；运行时用例使用
//! `common::seed_runtime_target` 的 rustc 现场编译 fixture 假核心（真实
//! 进程/真实监听/真实回收），Windows 真实核心验收由主代理执行。

mod common;

use std::time::Duration;

#[cfg(unix)]
use common::seed_core;
use common::{
    CLASH_YAML, TestRoute, TestServer, data_root, init_root_then, open_subscriptions,
    seed_runtime_target, start_app, texts,
};
use lanpath_client::ClientConfig;
use lanpath_common::CoreType;

/// 概览就绪：默认首页渲染出未配置步骤（新数据根）。
fn wait_overview_steps(app: &mut nsh_testing::Headless) {
    app.wait_for("开始之前").unwrap();
}

/// 种子已启用生效订阅（含原文/节点缓存），不选核心。
fn seed_active_subscription(root: &std::path::Path, name: &str) {
    let store = lanpath_client::SubscriptionStore::new(root.to_path_buf());
    let subscription = store
        .add(name, "https://example.com/overview.yaml", true, None)
        .unwrap();
    let fetched = lanpath_client::parse_subscription_body(CLASH_YAML, None).unwrap();
    store
        .write_source(
            subscription.id,
            &lanpath_client::SubscriptionSource {
                format: fetched.format,
                content: fetched.source.clone(),
            },
        )
        .unwrap();
    store
        .write_cached_content(
            subscription.id,
            &lanpath_client::CachedSubscriptionContent {
                format: fetched.format,
                singbox_nodes: fetched.singbox_nodes.clone(),
            },
        )
        .unwrap();
    // 清单 node_count/format 与缓存一致（等同真实刷新后的落盘形态）。
    let mut subscriptions = store.load().unwrap();
    if let Some(target) = subscriptions
        .iter_mut()
        .find(|candidate| candidate.id == subscription.id)
    {
        target.node_count = fetched.singbox_nodes.len() as u64;
        target.format = Some(fetched.format);
    }
    store.save(&subscriptions).unwrap();
    let mut config = ClientConfig::load_for_requests(root).unwrap();
    config.active_subscription_id = Some(subscription.id);
    config.save().unwrap();
}

/// 新数据根默认打开概览：分组导航真实展示，四个步骤按序列出，只有第一
/// 步“添加订阅”可执行（其余步骤没有动作按钮，不显示假进度）。
#[test]
fn fresh_root_defaults_to_overview_with_steps_and_only_first_actionable() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    wait_overview_steps(&mut app);
    let seen = texts(&app);
    for expected in ["工作区", "核心配置", "工具与恢复", "设置"] {
        assert!(
            seen.contains(&expected),
            "missing nav group {expected}: {seen:?}"
        );
    }
    for expected in ["添加订阅", "刷新节点", "选择核心", "校验并启动"] {
        assert!(
            seen.contains(&expected),
            "missing step {expected}: {seen:?}"
        );
    }
    // 未配置：无“已完成”标记，只有当前一步有动作按钮（第一步=去订阅页）。
    assert!(!seen.contains(&"已完成"), "{seen:?}");
    assert!(
        app.screen()
            .matches("id:overview-step-action >> text:去订阅页")
            .unwrap(),
        "当前可执行步骤是添加订阅: {seen:?}"
    );
    assert!(
        !app.screen()
            .matches("id:overview-step-action >> text:去核心页")
            .unwrap(),
        "后续步骤没有动作按钮: {seen:?}"
    );
    // 不展示未实现内容：无流量/出口区域。
    assert!(
        !seen.iter().any(|text| text.contains("活跃连接")),
        "{seen:?}"
    );

    // 当前步骤动作真实导航到订阅页。
    app.press("id:overview-step-action").unwrap();
    app.wait_for("id:subscription-name").unwrap();
}

/// 未配置步骤随真实配置推进：有生效订阅（含节点）但未选核心时，前两步
/// 标记完成、只有“选择核心”可执行；动作导航到核心页。
#[test]
fn steps_advance_with_real_configuration() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        seed_active_subscription(root, "概览订阅");
    });
    let mut app = start_app(entries).0;
    wait_overview_steps(&mut app);
    let seen = texts(&app);
    assert!(seen.contains(&"已完成"), "{seen:?}");
    assert_eq!(
        seen.iter().filter(|text| **text == "已完成").count(),
        2,
        "添加订阅/刷新节点已完成: {seen:?}"
    );
    assert!(
        app.screen()
            .matches("id:overview-step-action >> text:去核心页")
            .unwrap(),
        "只有选择核心可执行: {seen:?}"
    );
    // 摘要视图不出现（尚未完全配置）。
    assert!(!seen.contains(&"当前订阅"), "{seen:?}");

    app.press("id:overview-step-action").unwrap();
    app.wait_for("id:core-check-remote").unwrap();
}

/// 订阅页离页后 worker 仍完成刷新：概览无需重新进入订阅页，App 级版本
/// 变化会重读当前生效订阅摘要，显示真实节点数。
#[test]
fn overview_updates_after_an_offpage_subscription_refresh_completes() {
    const YAML: &str = "proxies:\n  - name: 节点甲\n    type: ss\n    server: 192.0.2.1\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n  - name: 节点乙\n    type: ss\n    server: 192.0.2.2\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n";
    let server = TestServer::new(vec![TestRoute::delayed(
        "/overview.yaml",
        YAML,
        Duration::from_millis(1200),
    )]);
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        seed_runtime_target(root, CoreType::SingBox, "1.2.3", 18401, 18402);
    });
    let mut app = start_app(entries).0;
    open_subscriptions(&mut app);
    app.type_text("id:subscription-name", "离页生效").unwrap();
    app.type_text("id:subscription-url", &server.url("/overview.yaml"))
        .unwrap();
    app.press("新增订阅").unwrap();
    app.wait_for("正在刷新订阅…").unwrap();

    // 刷新在途时设为生效，再立即离开订阅页。
    app.press("id:subscription-row-离页生效 >> text:设为生效")
        .unwrap();
    app.press("role:button[概览]").unwrap();
    // 新生效订阅尚无节点，概览保持未配置步骤，不伪造已就绪。
    let seen = texts(&app);
    assert!(!seen.contains(&"当前订阅"), "{seen:?}");

    // 旧 worker 完成后 App 级轮询读到版本变化，概览自动出现真实节点。
    app.wait_for_within(
        "text:/离页生效 · 2 个节点 · 格式 ClashYaml/",
        Duration::from_secs(10),
    )
    .unwrap();
}

/// 已配置概览显示真实当前生效订阅、实际选中核心与真实运行状态；全局
/// 状态栏与概览同一状态源（不是页面私有数据）。
#[test]
fn configured_overview_shows_real_subscription_core_summary_and_status_bar() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        seed_runtime_target(root, CoreType::SingBox, "1.2.3", 18361, 18362);
    });
    let mut app = start_app(entries).0;
    app.wait_for("当前订阅").unwrap();
    let seen = texts(&app);
    for expected in [
        "运行订阅 · 2 个节点 · 格式 ClashYaml",
        "sing-box v1.2.3",
        "未运行",
        "订阅：运行订阅 · 核心：sing-box v1.2.3",
    ] {
        assert!(seen.contains(&expected), "missing {expected}: {seen:?}");
    }
    // 管理操作导航到现有真实页面。
    app.press("id:overview-manage-subscriptions").unwrap();
    app.wait_for("id:subscription-name").unwrap();
    app.press("概览").unwrap();
    app.wait_for("当前订阅").unwrap();
    app.press("id:overview-choose-core").unwrap();
    app.wait_for("id:core-check-remote").unwrap();
    let _ = dir;
}

/// 从概览启动真实核心（fixture：真实进程/监听），运行状态在全局状态栏、
/// 概览与核心页之间一致；切页不取消启动任务、不产生永久 busy。
#[test]
fn starting_from_overview_shares_real_state_across_pages() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        seed_runtime_target(root, CoreType::SingBox, "1.2.3", 18371, 18372);
    });
    let mut app = start_app(entries).0;
    app.wait_for("当前订阅").unwrap();
    app.press("id:overview-start").unwrap();
    app.wait_for_within(
        "text:/已完成二进制校验并启动：sing-box v1.2.3 · mixed 18371/",
        Duration::from_secs(45),
    )
    .unwrap();
    app.wait_for_within(
        "text:/运行中：sing-box v1.2.3 · mixed 18371/",
        Duration::from_secs(30),
    )
    .unwrap();
    assert!(
        texts(&app)
            .iter()
            .any(|text| text.contains("运行中：sing-box")),
        "状态栏共享运行状态: {:?}",
        texts(&app)
    );

    // 切到核心页：同一共享状态（不是页面私有轮询），停止在核心页完成。
    app.press("核心").unwrap();
    app.wait_for("id:core-check-remote").unwrap();
    app.wait_for_within(
        "text:/运行中：sing-box v1.2.3 · mixed 18371/",
        Duration::from_secs(30),
    )
    .unwrap();
    app.press("id:core-stop").unwrap();
    app.wait_for_within(
        "已停止核心（强制终止并回收本次进程）",
        Duration::from_secs(30),
    )
    .unwrap();

    // 回概览：真实状态一致（未运行），且不再 busy。
    app.press("概览").unwrap();
    app.wait_for_within("未运行", Duration::from_secs(30))
        .unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"未运行"), "{seen:?}");
    assert!(
        !app.screen().matches("id:overview-step-action").unwrap(),
        "已配置摘要视图没有步骤动作按钮: {seen:?}"
    );
}

/// 启动任务跨页面存活：概览发起启动后立即切到核心页，结果仍真实上屏
/// （任务挂在 App owner 下，切页不取消；命令单飞不会半途 busy）。
#[test]
fn start_task_survives_leaving_the_overview_page() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        seed_runtime_target(root, CoreType::SingBox, "1.2.3", 18381, 18382);
    });
    let mut app = start_app(entries).0;
    app.wait_for("当前订阅").unwrap();
    app.press("id:overview-start").unwrap();
    // 启动在途时立即离页：核心页必须看到同一命令的完成结果。
    app.press("核心").unwrap();
    app.wait_for("id:core-check-remote").unwrap();
    app.wait_for_within(
        "text:/已完成二进制校验并启动：sing-box v1.2.3 · mixed 18381/",
        Duration::from_secs(45),
    )
    .unwrap();
    app.wait_for_within(
        "text:/运行中：sing-box v1.2.3 · mixed 18381/",
        Duration::from_secs(30),
    )
    .unwrap();
    // 停止并确认状态归零（无半启动/永久 busy）。
    app.press("id:core-stop").unwrap();
    app.wait_for_within(
        "已停止核心（强制终止并回收本次进程）",
        Duration::from_secs(30),
    )
    .unwrap();
    app.wait_for_within("未运行", Duration::from_secs(30))
        .unwrap();
}

/// 选择核心后概览与状态栏的实际选中摘要立即更新（共享信号，不独立轮询）。
#[cfg(unix)]
#[test]
fn selecting_a_core_updates_the_shared_summary() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        seed_active_subscription(root, "概览订阅");
        seed_core(root, CoreType::SingBox, "1.2.3", "1.2.3");
    });
    let mut app = start_app(entries).0;
    wait_overview_steps(&mut app);
    // 当前可执行步骤是“选择核心”：导航到核心页并真实选中本地版本。
    app.press("id:overview-step-action").unwrap();
    app.wait_for("id:core-check-remote").unwrap();
    app.press("id:core-use-sing-box-1.2.3").unwrap();
    app.wait_for("sing-box · v1.2.3").unwrap();
    // 回到概览：进入已配置摘要视图，订阅/核心摘要为真实值。
    app.press("概览").unwrap();
    app.wait_for("当前订阅").unwrap();
    let seen = texts(&app);
    for expected in [
        "概览订阅 · 2 个节点 · 格式 ClashYaml",
        "sing-box v1.2.3",
        "订阅：概览订阅 · 核心：sing-box v1.2.3",
    ] {
        assert!(seen.contains(&expected), "missing {expected}: {seen:?}");
    }
}

/// 英文界面回归：已配置概览的卡片标题、运行状态与全局状态栏全部经
/// `t!` 取值，不出现本阶段新增的中文硬编码（订阅名等业务数据保持原样）。
#[test]
fn configured_overview_and_status_bar_follow_english() {
    let (dir, mut entries) = data_root();
    entries.insert("language".to_owned(), "en".to_owned());
    init_root_then(&entries, dir.path(), |root| {
        seed_runtime_target(root, CoreType::SingBox, "1.2.3", 18391, 18392);
    });
    let mut app = start_app(entries).0;
    app.wait_for("Current Subscription").unwrap();
    let seen = texts(&app);
    for expected in [
        "Current Subscription",
        "Manage subscriptions…",
        "Core",
        "Choose core…",
        "Runtime Status",
        "Validate & Start",
        "Stop",
        "Not running",
        "Subscription: 运行订阅 · Core: sing-box v1.2.3",
    ] {
        assert!(seen.contains(&expected), "missing {expected}: {seen:?}");
    }
    // 本阶段 UI 文案不得残留中文硬编码。
    for unexpected in [
        "当前订阅",
        "管理订阅",
        "运行状态",
        "校验并启动",
        "未运行",
        "选择核心",
    ] {
        assert!(
            !seen.contains(&unexpected),
            "英文界面出现中文硬编码 {unexpected}: {seen:?}"
        );
    }
}

/// 摘要读取失败不得覆盖最后有效值：损坏订阅清单后，概览仍显示上次订阅，
/// 同时显示独立错误与恢复入口；状态栏提示摘要不可用。
#[test]
fn subscription_summary_failure_keeps_last_value_and_surfaces_error() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        seed_active_subscription(root, "保留订阅");
    });
    let mut app = start_app(entries).0;
    wait_overview_steps(&mut app);
    assert!(
        texts(&app)
            .iter()
            .any(|text| text.contains("订阅：保留订阅"))
    );

    let manifest = dir.path().join("subscriptions.toml");
    let original = std::fs::read(&manifest).expect("read valid manifest");
    std::fs::write(&manifest, "broken = [").expect("corrupt subscriptions manifest");
    app.press("订阅").unwrap();
    app.wait_for_within("text:/加载订阅失败/", Duration::from_secs(10))
        .unwrap();
    app.press("概览").unwrap();
    app.wait_for("id:overview-subscription-error").unwrap();

    let seen = texts(&app);
    assert!(
        seen.iter().any(|text| text.contains("订阅：保留订阅")),
        "最后有效摘要必须保留: {seen:?}"
    );
    assert!(
        seen.iter().any(|text| text.contains("当前订阅读取失败")),
        "读取失败必须独立上屏: {seen:?}"
    );
    assert!(
        seen.iter().any(|text| text.contains("摘要读取失败")),
        "全局状态栏必须提示摘要错误: {seen:?}"
    );

    // 修复清单并重新读取后错误清除，值仍为真实数据。
    std::fs::write(&manifest, original).expect("restore manifest");
    app.press("订阅").unwrap();
    app.wait_for("id:subscription-name").unwrap();
    app.press("概览").unwrap();
    wait_overview_steps(&mut app);
    let seen = texts(&app);
    assert!(
        !seen.iter().any(|text| text.contains("当前订阅读取失败")),
        "恢复成功后旧错误必须清除: {seen:?}"
    );
    assert!(
        seen.iter().any(|text| text.contains("订阅：保留订阅")),
        "恢复后摘要仍为真实数据: {seen:?}"
    );
}

/// 核心摘要读取失败保留最后有效选择；不在概览伪装成“尚未选择核心”。
#[cfg(unix)]
#[test]
fn core_summary_failure_keeps_last_value_and_surfaces_error() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        let core = seed_core(root, CoreType::SingBox, "1.2.3", "1.2.3");
        common::save_core_config(root, &core);
        seed_active_subscription(root, "保留订阅");
    });
    let mut app = start_app(entries).0;
    app.wait_for("当前订阅").unwrap();
    assert!(texts(&app).contains(&"sing-box v1.2.3"));

    std::fs::write(dir.path().join("app.toml"), "broken = [").expect("corrupt app config");
    app.press("核心").unwrap();
    app.wait_for("当前使用").unwrap();
    app.wait_for("id:core-selection-error").unwrap();
    app.press("概览").unwrap();
    app.wait_for("id:overview-core-error").unwrap();

    let seen = texts(&app);
    assert!(
        seen.contains(&"sing-box v1.2.3"),
        "最后有效核心选择必须保留: {seen:?}"
    );
    assert!(
        seen.iter().any(|text| text.contains("核心选择读取失败")),
        "核心读取失败必须独立上屏: {seen:?}"
    );
}

/// 已有命令结果与运行状态在语言切换后重新翻译，不保留旧语言快照。
#[test]
fn runtime_result_and_status_follow_language_changes() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        seed_runtime_target(root, CoreType::SingBox, "1.2.3", 18395, 18396);
    });
    let mut app = start_app(entries).0;
    app.wait_for("当前订阅").unwrap();
    app.press("id:overview-start").unwrap();
    app.wait_for_within(
        "text:/已完成二进制校验并启动：sing-box v1.2.3/",
        Duration::from_secs(45),
    )
    .unwrap();
    app.wait_for_within("text:/运行中：sing-box v1.2.3/", Duration::from_secs(30))
        .unwrap();

    app.press("外观").unwrap();
    app.wait_for("id:language-en").unwrap();
    app.press("id:language-en >> text:English").unwrap();
    app.wait_for("Overview").unwrap();
    app.press("id:nav-overview >> role:button").unwrap();
    app.wait_for_within(
        "text:/Validated \\(binary check passed\\) and started: sing-box v1.2.3/",
        Duration::from_secs(10),
    )
    .unwrap();
    app.wait_for_within("text:/Running: sing-box v1.2.3/", Duration::from_secs(10))
        .unwrap();
    assert!(
        !texts(&app).iter().any(|text| text.contains("运行中：")),
        "语言切换后不得保留旧的已翻译运行状态"
    );

    app.press("id:overview-stop").unwrap();
    app.wait_for_within("text:/Core stopped/", Duration::from_secs(30))
        .unwrap();
}

/// 快速连续触发启动只能形成一个共享命令：同步预约 busy 后第二次操作
/// 不得再启动 worker，也不得用“核心已运行”覆盖第一次的成功结果。
#[test]
fn rapid_repeated_start_is_single_flight_and_keeps_the_success_result() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        seed_runtime_target(root, CoreType::SingBox, "1.2.3", 18401, 18402);
    });
    let mut app = start_app(entries).0;
    app.wait_for("当前订阅").unwrap();

    app.press("id:overview-start").unwrap();
    // 第二个 press 在 busy 预约后应被禁用/拒绝；某些 headless 驱动会返回
    // disabled 错误，因此不把这次 press 的 Result 当作产品结果。
    let _ = app.press("id:overview-start");
    app.wait_for_within(
        "text:/已完成二进制校验并启动：sing-box v1.2.3/",
        Duration::from_secs(45),
    )
    .unwrap();
    app.wait_for_within("text:/运行中：sing-box v1.2.3/", Duration::from_secs(30))
        .unwrap();
    let seen = texts(&app);
    assert!(
        !seen
            .iter()
            .any(|text| text.contains("请先停止后再启动") || text.contains("already running")),
        "重复启动不得形成第二个命令并覆盖成功结果: {seen:?}"
    );

    app.press("id:overview-stop").unwrap();
    app.wait_for_within("text:/已停止核心/", Duration::from_secs(30))
        .unwrap();
}
