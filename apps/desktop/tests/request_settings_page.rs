//! Headless 测试（`nsh_testing::Headless`）：请求设置页的模式
//! 选择、自定义代理与 GitHub 前缀保存/重开、非法输入脱敏、重置恢复，
//! 以及两条真实联网路径：页面保存 Custom 后经订阅刷新验证请求确实走
//! 新代理（本线程 127.0.0.1 HTTP server 以绝对形式收到请求）；Core 模式
//! 在核心未启动时刷新真实失败（本机闲置端口短连接拒绝，不等待超时）。
//! 每个场景使用独立临时数据根，不依赖互联网。

mod common;

use std::time::Duration;

use common::{TestRoute, TestServer, data_root, start_app, texts};

/// 固定 YAML 测试订阅（两个 SS 节点，共享解析器可识别为 ClashYaml）。
const YAML: &str = "proxies:\n  - name: 节点甲\n    type: ss\n    server: 192.0.2.1\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n  - name: 节点乙\n    type: ss\n    server: 192.0.2.2\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n";

fn open_request_settings(app: &mut nsh_testing::Headless) {
    common::navigate(app, "请求设置");
    app.wait_for("请求代理模式").unwrap();
}

/// 侧栏导航到请求设置页：四种模式与作用域说明真实展示。
#[test]
fn navigating_to_the_request_settings_page_shows_modes_and_scope_notes() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    open_request_settings(&mut app);
    let seen = texts(&app);
    for expected in [
        "直连（不使用代理）",
        "跟随系统代理",
        "核心代理（指向已保存的 mixed_port）",
        "自定义 HTTP(S) / SOCKS5 代理",
        "下一次请求使用新设置",
        "核心未运行时，请求会失败",
    ] {
        assert!(
            seen.iter().any(|text| text.contains(expected)),
            "missing {expected}: {seen:?}"
        );
    }
}

/// 新装数据根默认直连：数据根初始化已建 app.toml（统一 TOML 布局），
/// 页面只读——打开页面不额外改写配置文件字节，也不产生 client.json。
#[test]
fn fresh_root_defaults_to_direct_and_never_writes_config_on_read() {
    let (dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    let app_toml = dir.path().join("app.toml");
    assert!(app_toml.is_file(), "数据根初始化必须已建 app.toml");
    let before = std::fs::read(&app_toml).unwrap();
    open_request_settings(&mut app);
    let seen = texts(&app);
    assert!(!seen.iter().any(|line| line.starts_with("已保存：")));
    assert!(!seen.contains(&"设置与已保存的请求策略一致"), "{seen:?}");
    assert_eq!(
        std::fs::read(&app_toml).unwrap(),
        before,
        "读取页面不得改写配置字节"
    );
    assert!(!dir.path().join("client.json").exists());
}

/// 自定义模式 + 代理地址 + GitHub 前缀保存：提示、已保存摘要更新，
/// 重开应用后输入框与磁盘配置都恢复（不重显旧草稿）。
#[test]
fn saving_custom_mode_with_url_and_prefix_persists_and_reopens() {
    let (dir, entries) = data_root();
    let mut app = start_app(entries.clone()).0;
    common::wait_ready(&mut app);
    open_request_settings(&mut app);

    app.press("id:request-mode-custom").unwrap();
    app.type_text("id:request-custom-url", "http://127.0.0.1:8080")
        .unwrap();
    app.type_text("id:request-github-prefix", "https://mirror.example/")
        .unwrap();
    app.press("保存").unwrap();
    app.wait_for("请求设置已保存；下一次订阅刷新与远程资源请求将使用新策略")
        .unwrap();
    let seen = texts(&app);
    assert!(!seen.iter().any(|line| line.starts_with("已保存：")));
    assert!(!seen.contains(&"设置与已保存的请求策略一致"), "{seen:?}");
    drop(app);

    let saved = lanpath_client::ClientConfig::load_for_requests(dir.path()).unwrap();
    assert_eq!(
        saved.request_proxy_mode,
        lanpath_client::RequestProxyMode::Custom
    );
    assert_eq!(saved.request_proxy_url, "http://127.0.0.1:8080");
    assert_eq!(saved.github_proxy_prefix, "https://mirror.example/");

    let mut reopened = start_app(entries).0;
    // 等第二次启动渲染完成（侧栏标题常驻）再导航。
    reopened.wait_for("id:nav-overview").unwrap();
    open_request_settings(&mut reopened);
    assert_eq!(
        reopened
            .screen()
            .input_text("id:request-custom-url")
            .unwrap(),
        Some("http://127.0.0.1:8080")
    );
    assert_eq!(
        reopened
            .screen()
            .input_text("id:request-github-prefix")
            .unwrap(),
        Some("https://mirror.example/")
    );
    let seen = texts(&reopened);
    assert!(!seen.iter().any(|line| line.starts_with("已保存：")));
}

/// 切出自定义模式不清空地址：草稿保留并可随其他模式保存，切回继续可用。
#[test]
fn switching_modes_keeps_the_custom_url_draft() {
    let (dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    open_request_settings(&mut app);

    app.press("id:request-mode-custom").unwrap();
    app.type_text("id:request-custom-url", "socks5://127.0.0.1:1080")
        .unwrap();
    app.press("id:request-mode-none").unwrap();
    // 非自定义模式输入只读但值仍在。
    assert_eq!(
        app.screen().input_text("id:request-custom-url").unwrap(),
        Some("socks5://127.0.0.1:1080")
    );
    app.press("保存").unwrap();
    app.wait_for_within("text:/请求设置已保存/", Duration::from_secs(10))
        .unwrap();

    let saved = lanpath_client::ClientConfig::load_for_requests(dir.path()).unwrap();
    assert_eq!(
        saved.request_proxy_mode,
        lanpath_client::RequestProxyMode::None
    );
    assert_eq!(saved.request_proxy_url, "socks5://127.0.0.1:1080");
}

/// 非法自定义代理：错误不含用户名/密码/完整 URL，草稿保留，保存失败
/// 不改写 app.toml 字节（新装默认仍是最初持久化的直连）。
#[test]
fn invalid_custom_proxy_shows_a_redacted_error_and_keeps_the_draft() {
    let (dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    let app_toml = dir.path().join("app.toml");
    let before = std::fs::read(&app_toml).unwrap();
    open_request_settings(&mut app);

    app.press("id:request-mode-custom").unwrap();
    let bad_url = "http://secret-user:secret-password@127.0.0.1:8080/private";
    app.type_text("id:request-custom-url", bad_url).unwrap();
    app.type_text("id:request-github-prefix", "changed-mirror")
        .unwrap();
    app.press("保存").unwrap();
    app.wait_for_within("text:/保存请求设置失败/", Duration::from_secs(10))
        .unwrap();
    let seen = texts(&app);
    assert!(
        seen.iter().any(|text| text.contains("自定义代理地址无效")),
        "{seen:?}"
    );
    for secret in ["secret-user", "secret-password", bad_url] {
        assert!(
            seen.iter().all(|text| !text.contains(secret)),
            "screen leaked {secret}: {seen:?}"
        );
    }
    // 草稿保留待修改；保存失败不改写配置字节，已保存值仍是新装默认直连。
    assert_eq!(
        app.screen().input_text("id:request-custom-url").unwrap(),
        Some(bad_url)
    );
    assert_eq!(std::fs::read(&app_toml).unwrap(), before);
    assert!(!dir.path().join("client.json").exists());
    assert!(!texts(&app).iter().any(|line| line.starts_with("已保存：")));
}

/// 重置：有未保存修改先确认；放弃修改后草稿恢复为已保存值。
#[test]
fn resetting_a_dirty_draft_asks_and_restores_the_saved_values() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    open_request_settings(&mut app);
    // 先保存一份“跟随系统”为基线。
    app.press("id:request-mode-system").unwrap();
    app.press("保存").unwrap();
    app.wait_for_within("text:/请求设置已保存/", Duration::from_secs(10))
        .unwrap();

    // 改成自定义 + 地址（脏），再重置。
    app.press("id:request-mode-custom").unwrap();
    app.type_text("id:request-custom-url", "http://127.0.0.1:9090")
        .unwrap();
    app.press("重置").unwrap();
    app.wait_for("放弃未保存的请求设置修改？").unwrap();
    app.press("继续编辑").unwrap();
    // 继续编辑：确认框关闭，草稿还在。
    assert_eq!(
        app.screen().input_text("id:request-custom-url").unwrap(),
        Some("http://127.0.0.1:9090")
    );
    app.press("重置").unwrap();
    app.wait_for("放弃未保存的请求设置修改？").unwrap();
    app.press("放弃修改").unwrap();
    app.wait_for("已重新载入保存的请求设置").unwrap();
    assert_eq!(
        app.screen().input_text("id:request-custom-url").unwrap(),
        Some("")
    );
    let seen = texts(&app);
    assert!(!seen.iter().any(|line| line.starts_with("已保存：")));
    assert!(!seen.contains(&"设置与已保存的请求策略一致"), "{seen:?}");
}

/// 页面保存 Custom 代理后经订阅刷新验证：请求以绝对形式到达代理服务器
/// （订阅目标 192.0.2.99 不可达，只有真正走代理才可能成功）。
#[test]
fn subscription_refresh_goes_through_the_ui_saved_custom_proxy() {
    let proxied_path = "http://192.0.2.99/proxied.yaml";
    let proxy = TestServer::new(vec![TestRoute::ok(proxied_path, YAML)]);
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    open_request_settings(&mut app);
    app.press("id:request-mode-custom").unwrap();
    app.type_text("id:request-custom-url", &proxy.url(""))
        .unwrap();
    app.press("保存").unwrap();
    app.wait_for_within("text:/请求设置已保存/", Duration::from_secs(10))
        .unwrap();

    common::open_subscriptions(&mut app);
    app.type_text("id:subscription-name", "代理订阅").unwrap();
    app.type_text("id:subscription-url", "http://192.0.2.99/proxied.yaml")
        .unwrap();
    app.press("新增订阅").unwrap();
    app.wait_for_within("text:/刷新完成：2 个节点/", Duration::from_secs(10))
        .unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"2 个节点 · 格式 ClashYaml"), "{seen:?}");
    // 代理服务器必须收到绝对形式的完整目标 URL，且只请求一次。
    let recorded = proxy.recorded();
    assert_eq!(recorded.len(), 1, "{recorded:?}");
    assert_eq!(recorded[0].path, proxied_path, "{recorded:?}");
}

/// Core 模式 + 核心未启动：刷新在闲置端口上立即连接拒绝、真实失败上屏，
/// 不伪报成功、不拖慢到超时；行内错误随记录持久化。
#[test]
fn core_mode_refresh_fails_for_real_when_the_core_is_not_running() {
    let (dir, entries) = data_root();
    // 先初始化数据根（写归属标记），再预置 mixed_port 指向本机闲置端口。
    {
        let mut first = start_app(entries.clone()).0;
        common::wait_ready(&mut first);
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let idle_port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut config = lanpath_client::ClientConfig::load_for_requests(dir.path()).unwrap();
    config.mixed_port = idle_port;
    config.save().unwrap();

    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    open_request_settings(&mut app);
    app.press("id:request-mode-core").unwrap();
    app.press("保存").unwrap();
    app.wait_for_within("text:/请求设置已保存/", Duration::from_secs(10))
        .unwrap();
    let saved = lanpath_client::ClientConfig::load_for_requests(dir.path()).unwrap();
    assert_eq!(
        saved.request_proxy_mode,
        lanpath_client::RequestProxyMode::Core
    );
    assert_eq!(saved.mixed_port, idle_port);

    common::open_subscriptions(&mut app);
    app.type_text("id:subscription-name", "核心负例").unwrap();
    app.type_text("id:subscription-url", "https://example.com/core.yaml")
        .unwrap();
    app.press("新增订阅").unwrap();
    app.wait_for_within("text:/拉取失败/", Duration::from_secs(10))
        .unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"未获取节点"), "{seen:?}");
    assert!(!seen.contains(&"2 个节点"), "{seen:?}");

    // 真实错误随记录持久化：连接拒绝（不是超时、不是伪造成功）。
    let subscriptions = lanpath_client::SubscriptionStore::new(dir.path().to_path_buf())
        .load()
        .unwrap();
    let record = subscriptions
        .iter()
        .find(|subscription| subscription.name == "核心负例")
        .unwrap();
    assert_eq!(record.node_count, 0);
    let error = record.error.clone().unwrap();
    assert!(error.contains("拉取失败"), "{error}");
}
