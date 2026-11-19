//! Headless 测试（`nsh_testing::Headless`）：配置页共享合成预览。
//! 每个场景使用独立临时数据根（见 `common::data_root`）；联网用例经
//! `common::TestServer` 本地真实 HTTP 服务器（含自定义代理的绝对形式请求），
//! 不依赖互联网。

mod common;

use std::path::Path;
use std::time::Duration;

use common::{TestRoute, TestServer, data_root, start_app, texts};
use lanpath_client::{
    CachedSubscriptionContent, RequestProxyMode, SubscriptionSource, parse_subscription_body,
};

const YAML: &str = "proxies:\n  - name: 节点甲\n    type: ss\n    server: 192.0.2.1\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n  - name: 节点乙\n    type: ss\n    server: 192.0.2.2\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n";

const DISCLAIMER: &str = "共享合成预览，未进行核心二进制校验 · 按当前生效订阅与已保存设置合成 sing-box JSON / Mihomo YAML · 不启动核心/系统代理/TUN";

/// 先用应用本身初始化数据根（写归属标记与统一 TOML 布局），再落种子
/// 数据——直接把配置文件写进未归属目录会被准入当作生产根拒绝（与请求
/// 设置页测试的预置方式一致）。
fn init_data_root(entries: &std::collections::BTreeMap<String, String>) {
    let mut app = start_app(entries.clone()).0;
    common::wait_ready(&mut app);
}

/// 新增已启用订阅并设为当前生效（不写节点缓存），返回 id。
fn seed_active_subscription(root: &Path, url: &str) -> uuid::Uuid {
    let store = lanpath_client::SubscriptionStore::new(root.to_path_buf());
    let subscription = store.add("预览订阅", url, true, None).unwrap();
    let mut config = lanpath_client::ClientConfig::load_for_requests(root).unwrap();
    config.active_subscription_id = Some(subscription.id);
    config.save().unwrap();
    subscription.id
}

/// 给已存在订阅写“上次成功”的原文与节点缓存。
fn seed_cache(root: &Path, id: uuid::Uuid) {
    let store = lanpath_client::SubscriptionStore::new(root.to_path_buf());
    let fetched = parse_subscription_body(YAML, None).unwrap();
    store
        .write_source(
            id,
            &SubscriptionSource {
                format: fetched.format,
                content: fetched.source.clone(),
            },
        )
        .unwrap();
    store
        .write_cached_content(
            id,
            &CachedSubscriptionContent {
                format: fetched.format,
                singbox_nodes: fetched.singbox_nodes,
            },
        )
        .unwrap();
}

/// 保存请求策略（Core = 预览全程离线）。
fn save_request_mode(root: &Path, mode: RequestProxyMode, proxy_url: &str) {
    let mut config = lanpath_client::ClientConfig::load_for_requests(root).unwrap();
    config.request_proxy_mode = mode;
    config.request_proxy_url = proxy_url.to_owned();
    config.save().unwrap();
}

fn json_text(app: &nsh_testing::Headless) -> Option<String> {
    texts(app)
        .into_iter()
        .find(|text| text.contains("\"outbounds\""))
        .map(str::to_owned)
}

fn open_config_page(app: &mut nsh_testing::Headless) {
    // 等首帧渲染完成（侧栏标题上屏）再导航。
    app.wait_for("Lanpath NSH Desktop").unwrap();
    app.press("配置").unwrap();
    app.wait_for("尚未生成配置预览").unwrap();
}

/// 页面打开即显示“共享合成预览，未进行核心二进制校验”的明确文案与空闲状态。
#[test]
fn config_page_opens_with_disclaimer_and_idle_state() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    app.press("配置").unwrap();
    app.wait_for("尚未生成配置预览").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&DISCLAIMER), "{seen:?}");
    assert!(seen.contains(&"生成配置预览"), "{seen:?}");
    assert!(
        seen.contains(&"点击「生成配置预览」查看完整 sing-box JSON / Mihomo YAML。"),
        "{seen:?}"
    );
}

/// 有缓存 + Core 策略：点击后后台合成，完整 pretty JSON、摘要与降级警告
/// 真实上屏；JSON 不含面板下载字段；全程不联网、不启动核心。
#[test]
fn composing_renders_the_full_json_summary_and_warnings() {
    let (dir, entries) = data_root();
    init_data_root(&entries);
    let id = seed_active_subscription(dir.path(), "https://example.com/c.yaml");
    seed_cache(dir.path(), id);
    save_request_mode(dir.path(), RequestProxyMode::Core, "");

    let mut app = start_app(entries).0;
    open_config_page(&mut app);
    app.press("生成配置预览").unwrap();
    app.wait_for_within(
        "text:/上次成功生成：sing-box · JSON · 2 个节点/",
        Duration::from_secs(10),
    )
    .unwrap();
    app.wait_for_within("text:/内置规则集缺失/", Duration::from_secs(10))
        .unwrap();

    let seen = texts(&app);
    assert!(
        seen.iter().any(|text| text.contains("警告:内置规则集缺失")),
        "{seen:?}"
    );
    let json = json_text(&app).expect("preview JSON on screen");
    assert!(json.contains("\"tag\": \"节点甲\""), "{json}");
    assert!(json.contains("\"tag\": \"节点乙\""), "{json}");
    assert!(json.contains("\"listen_port\": 17899"), "{json}");
    assert!(
        !json.contains("external_ui"),
        "预览不得携带面板下载字段:{json}"
    );
    // 明确文案始终可见，绝不显示“已验证/已启动”。
    assert!(texts(&app).contains(&DISCLAIMER));
}

/// busy 单飞：在途预览期间重复点击不启动第二次合成；完成后 busy 恢复，
/// 可再次生成（缓存命中，不再请求订阅地址）。
#[test]
fn busy_is_single_flight_and_recovers_after_completion() {
    // 经自定义代理的请求是 absolute-form：共享服务器按后缀兼容匹配。
    let server = TestServer::new(vec![TestRoute::delayed(
        "/sub.yaml",
        YAML,
        Duration::from_millis(500),
    )]);
    let (dir, entries) = data_root();
    init_data_root(&entries);
    seed_active_subscription(dir.path(), &server.url("/sub.yaml"));
    save_request_mode(dir.path(), RequestProxyMode::Custom, &server.url(""));

    let mut app = start_app(entries).0;
    open_config_page(&mut app);
    app.press("生成配置预览").unwrap();
    app.wait_for("正在合成配置预览…").unwrap();
    // 在途重复点击：不启动第二个合成任务。
    app.press("生成配置预览").unwrap();
    assert!(texts(&app).contains(&"正在合成配置预览…"));
    app.wait_for_within(
        "text:/上次成功生成：sing-box · JSON · 2 个节点/",
        Duration::from_secs(10),
    )
    .unwrap();

    let hits = server
        .recorded()
        .into_iter()
        .filter(|request| request.path.ends_with("/sub.yaml"))
        .count();
    assert_eq!(hits, 1, "busy 单飞只允许一次订阅请求");
    assert!(!texts(&app).contains(&"正在合成配置预览…"));

    // busy 恢复：再次点击立即重新生成（已有缓存，离线完成）。
    app.press("生成配置预览").unwrap();
    app.wait_for_within(
        "text:/上次成功生成：sing-box · JSON · 2 个节点/",
        Duration::from_secs(10),
    )
    .unwrap();
}

/// 失败保留旧预览：同一会话里第二次合成失败时显示真实错误，上次成功的
/// JSON 与摘要仍保留在屏幕上，不伪报新成功。
#[test]
fn failure_keeps_the_previous_preview_and_shows_the_real_error() {
    let (dir, entries) = data_root();
    init_data_root(&entries);
    let id = seed_active_subscription(dir.path(), "https://example.com/keep.yaml");
    seed_cache(dir.path(), id);
    save_request_mode(dir.path(), RequestProxyMode::Core, "");

    let mut app = start_app(entries).0;
    open_config_page(&mut app);
    app.press("生成配置预览").unwrap();
    app.wait_for_within(
        "text:/上次成功生成：sing-box · JSON · 2 个节点/",
        Duration::from_secs(10),
    )
    .unwrap();

    // 生效订阅在页面外被停用（直接落盘构造“生效但停用”的存量状态）；
    // 预览状态保留在页面内，不随这次外部修改消失。
    let store = lanpath_client::SubscriptionStore::new(dir.path().to_path_buf());
    let mut list = store.load().unwrap();
    list.iter_mut()
        .find(|subscription| subscription.id == id)
        .unwrap()
        .enabled = false;
    store.save(&list).unwrap();

    app.press("生成配置预览").unwrap();
    app.wait_for_within("text:/生成配置预览失败:/", Duration::from_secs(10))
        .unwrap();
    let seen = texts(&app);
    assert!(
        seen.iter().any(|text| text.contains("所选订阅已停用")),
        "{seen:?}"
    );
    // 旧预览与摘要保留，不伪报新成功。
    assert!(
        seen.iter()
            .any(|text| text.contains("上次成功生成：sing-box · JSON · 2 个节点")),
        "{seen:?}"
    );
    let json = json_text(&app).expect("previous preview JSON kept");
    assert!(json.contains("\"tag\": \"节点甲\""), "{json}");
}

/// 离开页面再回来不会造成永久 busy：在途任务随页面卸载取消，重进后可
/// 立即重新开始并完成。
#[test]
fn leaving_the_page_mid_compose_does_not_stay_busy() {
    let server = TestServer::new(vec![TestRoute::delayed(
        "/sub.yaml",
        YAML,
        Duration::from_millis(600),
    )]);
    let (dir, entries) = data_root();
    init_data_root(&entries);
    seed_active_subscription(dir.path(), &server.url("/sub.yaml"));
    save_request_mode(dir.path(), RequestProxyMode::Custom, &server.url(""));

    let mut app = start_app(entries).0;
    open_config_page(&mut app);
    app.press("生成配置预览").unwrap();
    app.wait_for("正在合成配置预览…").unwrap();

    // 切走再切回：页面是全新状态，busy 不得卡住。
    app.press("日志").unwrap();
    app.wait_for("日志").unwrap();
    app.press("配置").unwrap();
    let seen = texts(&app);
    assert!(!seen.contains(&"正在合成配置预览…"), "{seen:?}");

    // 等第一次后台请求落定，再重新开始：必须能成功完成（缓存已写入，
    // 第二次合成离线完成，busy 只在短时间内闪烁）。
    std::thread::sleep(Duration::from_millis(700));
    app.press("生成配置预览").unwrap();
    app.wait_for_within(
        "text:/上次成功生成：sing-box · JSON · 2 个节点/",
        Duration::from_secs(10),
    )
    .unwrap();
    assert!(!texts(&app).contains(&"正在合成配置预览…"));
}

/// 已保存 Mihomo 核心选择：配置页真实合成 Mihomo YAML 预览（GPUI
/// Mihomo 分支同路径）——摘要显示格式，正文是 YAML 关键字段/节点/端口，
/// 面板下载字段移除，不伪报 JSON。
#[test]
fn mihomo_selection_composes_a_yaml_preview() {
    let (dir, entries) = data_root();
    init_data_root(&entries);
    let id = seed_active_subscription(dir.path(), "https://example.com/m.yaml");
    seed_cache(dir.path(), id);
    save_request_mode(dir.path(), RequestProxyMode::Core, "");
    let mihomo = dir.path().join("cores/mihomo/1.19.29/mihomo");
    std::fs::create_dir_all(mihomo.parent().unwrap()).unwrap();
    std::fs::write(&mihomo, b"inventory fixture").unwrap();
    let mut config = lanpath_client::ClientConfig::load_for_requests(dir.path()).unwrap();
    config.core_binary = mihomo;
    config.save().unwrap();

    let mut app = start_app(entries).0;
    open_config_page(&mut app);
    app.press("生成配置预览").unwrap();
    app.wait_for_within(
        "text:/上次成功生成：mihomo · YAML · 2 个节点/",
        Duration::from_secs(10),
    )
    .unwrap();
    let seen = texts(&app);
    let yaml = seen
        .iter()
        .find(|text| text.contains("mixed-port"))
        .expect("YAML preview on screen");
    assert!(yaml.contains("mixed-port: 17899"), "{yaml}");
    assert!(yaml.contains("节点甲"), "{yaml}");
    assert!(yaml.contains("节点乙"), "{yaml}");
    assert!(yaml.contains("external-controller"), "{yaml}");
    assert!(!yaml.contains("external-ui"), "面板下载字段必须移除:{yaml}");
    assert!(
        !yaml.contains("\"outbounds\""),
        "Mihomo 预览不得是 sing-box JSON:{yaml}"
    );
    // 明确文案始终可见，绝不显示“已验证/已启动”。
    assert!(texts(&app).contains(&DISCLAIMER));
}

/// 合成成功可能更新订阅节点清单；完成后概览/状态栏共享摘要必须重读，
/// 不等待用户再次进入订阅页。
#[test]
fn successful_compose_refreshes_the_shared_subscription_summary() {
    let server = TestServer::new(vec![TestRoute::ok("/sub.yaml", YAML)]);
    let (dir, entries) = data_root();
    init_data_root(&entries);
    let core = common::seed_core(
        dir.path(),
        lanpath_common::CoreType::SingBox,
        "1.2.3",
        "1.2.3",
    );
    common::save_core_config(dir.path(), &core);
    seed_active_subscription(dir.path(), &server.url("/sub.yaml"));
    save_request_mode(dir.path(), RequestProxyMode::Custom, &server.url(""));

    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    // 订阅已生效但尚无节点：概览仍处于首配流程。
    assert!(texts(&app).contains(&"刷新节点"), "{:?}", texts(&app));

    open_config_page(&mut app);
    app.press("生成配置预览").unwrap();
    app.wait_for_within(
        "text:/上次成功生成：sing-box · JSON · 2 个节点/",
        Duration::from_secs(10),
    )
    .unwrap();

    app.press("概览").unwrap();
    app.wait_for_within(
        "text:/预览订阅 · 2 个节点 · 格式 ClashYaml/",
        Duration::from_secs(10),
    )
    .unwrap();
    assert!(texts(&app).contains(&"预览订阅 · 2 个节点 · 格式 ClashYaml"));
}
