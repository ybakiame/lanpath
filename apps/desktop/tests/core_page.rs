//! Headless 测试（`nsh_testing::Headless`）：核心页（核心库存与
//! 下载/选择）。真实联网场景经本线程 127.0.0.1 HTTP 服务器（release metadata
//! JSON 回指本服务器资产 + 真实 tar.gz 归档），不依赖互联网；Unix 用脚本
//! 核心做真实 version probe，Windows 原生验收由主代理执行。每个场景独立
//! 临时数据根；release metadata API 经 storage 键
//! `lanpath_desktop_nsh.core_api_base` 注入（生产默认 GitHub）。

mod common;

use std::time::Duration;

use common::{
    TestRoute, TestServer, core_binary, data_root, init_root_then, save_core_config, seed_core,
    seed_runtime_target, start_app, texts,
};
use lanpath_client::ClientConfig;
#[cfg(unix)]
use lanpath_client::RequestProxyMode;
use lanpath_common::CoreType;

const API_BASE_KEY: &str = "lanpath_desktop_nsh.core_api_base";

fn open_core_page(app: &mut nsh_testing::Headless) {
    common::navigate(app, "核心");
    app.wait_for("准备核心版本，校验后启动本地代理。运行会话与系统代理分别确认。")
        .unwrap();
}

/// 检查更新（channels 列表）fixture 服务器：只含 release 列表路由。
fn channels_server(version: &str) -> TestServer {
    TestServer::new(vec![TestRoute::ok(
        "/repos/SagerNet/sing-box/releases?per_page=30",
        &format!("[{{\"tag_name\":\"v{version}\"}}]"),
    )])
}

/// 与共享 `target_spec_for` 同一平台映射（期望资产名用）。
#[cfg(unix)]
fn platform_spec() -> (&'static str, bool) {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => ("linux-amd64", false),
        ("linux", "aarch64") => ("linux-arm64", false),
        ("macos", "x86_64") => ("darwin-amd64", false),
        ("macos", "aarch64") => ("darwin-arm64", false),
        (os, arch) => panic!("unsupported core download test platform: {os}-{arch}"),
    }
}

/// 真实 tar.gz 归档：内含可执行脚本核心。
#[cfg(unix)]
fn singbox_archive(reported: &str) -> Vec<u8> {
    let script = format!("#!/bin/sh\necho 'sing-box version {reported}'\n");
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut tar = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu();
    header.set_size(script.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    tar.append_data(&mut header, "release/sing-box", script.as_bytes())
        .unwrap();
    tar.into_inner().unwrap().finish().unwrap()
}

/// 完整 release fixture 服务器：channels + tags metadata + 真实归档资产；
/// `delay` 施加在资产响应上（busy/离页恢复场景用）。
#[cfg(unix)]
fn release_server(version: &str, delay: Duration) -> TestServer {
    let (platform, _) = platform_spec();
    let asset = format!("sing-box-{version}-{platform}.tar.gz");
    let release = format!(
        "{{\"assets\":[{{\"name\":\"{asset}\",\"browser_download_url\":\"http://{{addr}}/asset\"}}]}}"
    );
    TestServer::new(vec![
        TestRoute::ok(
            "/repos/SagerNet/sing-box/releases?per_page=30",
            &format!("[{{\"tag_name\":\"v{version}\"}}]"),
        ),
        TestRoute::ok(
            &format!("/repos/SagerNet/sing-box/releases/tags/v{version}"),
            &release,
        ),
        TestRoute::delayed_bytes("/asset", singbox_archive(version), delay),
    ])
}

/// 侧栏导航到核心页：作用域说明、空态与通道/类型控件真实展示；
/// 无核心不是崩溃——显示“尚未选择核心”。
#[test]
fn navigating_to_the_core_page_shows_scope_notes_and_empty_states() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    common::navigate(&mut app, "核心");
    app.wait_for("尚未选择核心").unwrap();
    let seen = texts(&app);
    for expected in [
        "准备核心版本，校验后启动本地代理。运行会话与系统代理分别确认。",
        "运行状态与概览及状态栏同步；启停核心请前往概览页操作。核心仅监听本地地址，系统代理由独立守护接管和恢复。",
        "未运行",
        "此通道尚无本地版本，可切换通道或检查更新后下载。",
        "本地：未安装",
        "远程：尚未检查",
        "sing-box",
        "Mihomo",
        "稳定版",
        "预发布版",
        // 系统代理分区如实展示（支持性取决于平台，见下方分支断言）。
        "系统代理",
    ] {
        assert!(
            seen.iter().any(|text| text.contains(expected)),
            "missing {expected}: {seen:?}"
        );
    }
    #[cfg(not(windows))]
    {
        // 非 Windows：如实显示不支持；恢复操作隐藏，但开关可见
        // （只保存偏好，本机不接管），确认框流程可交互。
        let seen = texts(&app);
        assert!(
            seen.iter()
                .any(|text| text.contains("系统代理接管仅支持 Windows")),
            "missing unsupported note: {seen:?}"
        );
        assert!(
            !seen.iter().any(|text| text.contains("恢复系统代理")),
            "接管操作必须隐藏: {seen:?}"
        );
        assert!(
            seen.iter()
                .any(|text| text.contains("随核心启停设置系统代理")),
            "偏好开关仍应可见: {seen:?}"
        );
    }
    #[cfg(windows)]
    {
        // Windows：分区提供接管开关与显式恢复（真实 WinINet 验收由主代理）。
        let seen = texts(&app);
        assert!(
            seen.iter()
                .any(|text| text.contains("随核心启停设置系统代理")),
            "missing takeover switch: {seen:?}"
        );
        assert!(
            seen.iter().any(|text| text.contains("恢复系统代理")),
            "missing restore action: {seen:?}"
        );
    }
    // 核心启停统一收敛至概览页，核心页运行卡片仅作只读状态展示。
    assert!(!seen.contains(&"校验并启动"), "{seen:?}");
    assert!(!seen.contains(&"停止"), "{seen:?}");
    assert!(!app.screen().matches("id:core-start").unwrap());
    assert!(!app.screen().matches("id:core-stop").unwrap());
}

/// 已选中的核心独立显示；切到另一库存类型浏览时空列表提示出现，
/// 实际选中摘要不变。
#[test]
fn seeded_active_core_shown_and_type_switch_keeps_active_summary() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        let singbox = seed_core(root, CoreType::SingBox, "1.2.3", "1.2.3");
        save_core_config(root, &singbox);
    });
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    open_core_page(&mut app);
    app.wait_for("sing-box · v1.2.3").unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"v1.2.3"), "行内版本: {seen:?}");

    app.press("id:core-type-mihomo").unwrap();
    app.wait_for("此通道尚无本地版本，可切换通道或检查更新后下载。")
        .unwrap();
    let seen = texts(&app);
    assert!(
        seen.contains(&"sing-box · v1.2.3"),
        "浏览类型切换不改变实际选中: {seen:?}"
    );
}

/// 查询远端版本后下载指定版本：真实解压落盘、自动选中、新装默认请求
/// 策略固化为直连（不被核心选择改回 System）；统一 TOML 布局下选择写入
/// 启动时已初始化的 app.toml，不再产生 client.json。
#[cfg(unix)]
#[test]
fn check_remote_then_download_installs_selects_and_persists() {
    let (dir, mut entries) = data_root();
    let server = release_server("1.2.3", Duration::ZERO);
    entries.insert(API_BASE_KEY.to_owned(), server.url(""));
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    open_core_page(&mut app);

    app.press("id:core-check-remote").unwrap();
    app.wait_for("远程：v1.2.3").unwrap();
    app.press("id:core-download").unwrap();
    app.wait_for("已下载并选中 v1.2.3").unwrap();
    app.wait_for("sing-box · v1.2.3").unwrap();

    let binary = core_binary(dir.path(), CoreType::SingBox, "1.2.3");
    assert!(binary.is_file(), "真实解压落盘: {binary:?}");
    let config = ClientConfig::load(dir.path()).unwrap();
    assert_eq!(config.core_binary, binary);
    assert_eq!(
        config.request_proxy_mode,
        RequestProxyMode::None,
        "新装默认直连固化，不被核心选择改回 System"
    );
    assert!(
        lanpath_client::settings::app_settings_path(dir.path()).is_file(),
        "选择必须写入启动时初始化的 app.toml"
    );
    assert!(!dir.path().join("client.json").exists());
    // 真实请求序列：release channels + release tags + 资产各一次。
    assert_eq!(server.count_path("/asset"), 1, "{:?}", server.recorded());
    assert_eq!(
        server.count_path("/repos/SagerNet/sing-box/releases/tags/v1.2.3"),
        1
    );
}

/// 查询失败保留旧状态并显示真实错误；换用可用服务后同页恢复成功。
#[test]
fn check_remote_failure_keeps_state_and_recovers_with_a_good_server() {
    let (_dir, mut entries) = data_root();
    let bad = TestServer::new(vec![]);
    entries.insert(API_BASE_KEY.to_owned(), bad.url(""));
    let mut app = start_app(entries.clone()).0;
    common::wait_ready(&mut app);
    open_core_page(&mut app);
    app.press("id:core-check-remote").unwrap();
    app.wait_for_within("text:/检查更新失败/", Duration::from_secs(10))
        .unwrap();
    let seen = texts(&app);
    assert!(seen.contains(&"远程：尚未检查"), "失败保留旧状态: {seen:?}");
    drop(app);

    let good = channels_server("1.2.3");
    entries.insert(API_BASE_KEY.to_owned(), good.url(""));
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    open_core_page(&mut app);
    app.press("id:core-check-remote").unwrap();
    app.wait_for("远程：v1.2.3").unwrap();
    let seen = texts(&app);
    assert!(
        !seen.iter().any(|text| text.contains("检查更新失败")),
        "成功后旧错误清除: {seen:?}"
    );
}

/// 下载 busy 全局单飞（重复点击不重复请求）；下载中途离开页面再回来，
/// 新页面从业务层共享活动恢复真实 busy，而不是假装空闲或永久 busy；旧 worker
/// 完成后版本变化会清除 busy、重载列表并同步 App 级核心摘要。无需再次点击
/// 下载或手动查询即可看到真实选中核心。
#[cfg(unix)]
#[test]
fn download_is_single_flight_and_recovers_after_leaving_the_page() {
    let (dir, mut entries) = data_root();
    let server = release_server("1.2.3", Duration::from_millis(1500));
    entries.insert(API_BASE_KEY.to_owned(), server.url(""));
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    open_core_page(&mut app);
    app.press("id:core-check-remote").unwrap();
    app.wait_for("远程：v1.2.3").unwrap();

    app.press("id:core-download").unwrap();
    // busy 中重复点击 no-op（全局单飞，不启动第二个下载）。
    app.press("id:core-download").unwrap();

    // 下载在途时离开页面再回来：新页面看到同根真实活动，不能重复发请求。
    common::open_subscriptions(&mut app);
    common::wait_ready(&mut app);
    open_core_page(&mut app);
    app.wait_for_within("text:/正在下载/", Duration::from_secs(5))
        .unwrap();
    // 新页面处于共享 busy：下载入口不可按压，不产生第二个资产请求。
    assert!(
        !app.screen().matches("id:core-download").unwrap(),
        "共享在途时不能再次启动下载: {:?}",
        texts(&app)
    );
    assert_eq!(server.count_path("/asset"), 1, "不产生第二个资产请求");

    // 旧页面 worker 真实完成：落盘、活动清除、摘要更新，无需重进页面。
    let binary = core_binary(dir.path(), CoreType::SingBox, "1.2.3");
    app.wait_for_within("sing-box · v1.2.3", Duration::from_secs(45))
        .unwrap();
    assert!(binary.is_file(), "在途下载完成并落盘: {binary:?}");
    assert_eq!(server.count_path("/asset"), 1, "单飞：资产只下载一次");
    assert!(
        !texts(&app).iter().any(|text| text.contains("正在下载")),
        "共享活动完成后 busy 清除: {:?}",
        texts(&app)
    );
    let config = ClientConfig::load(dir.path()).unwrap();
    assert_eq!(config.core_binary, binary);
}

/// 核心下载在离页期间完成：重新进入时页面初始 busy 必须与共享活动一致，
/// 不能残留“正在下载”，且真实选中核心与 App 摘要可直接看到。
#[cfg(unix)]
#[test]
fn download_completed_before_returning_clears_initial_busy() {
    let (dir, mut entries) = data_root();
    let server = release_server("1.2.3", Duration::from_millis(300));
    entries.insert(API_BASE_KEY.to_owned(), server.url(""));
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    open_core_page(&mut app);
    app.press("id:core-check-remote").unwrap();
    app.wait_for("远程：v1.2.3").unwrap();
    app.press("id:core-download").unwrap();

    common::open_subscriptions(&mut app);
    common::wait_ready(&mut app);
    let binary = core_binary(dir.path(), CoreType::SingBox, "1.2.3");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let saved = ClientConfig::load(dir.path()).is_ok_and(|config| config.core_binary == binary);
        if binary.is_file() && saved {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(binary.is_file(), "下载已完成落盘");

    open_core_page(&mut app);
    app.wait_for("sing-box · v1.2.3").unwrap();
    let seen = texts(&app);
    assert!(
        !seen.iter().any(|text| text.contains("正在下载")),
        "完成先于重进时不得残留 busy: {seen:?}"
    );
    assert_eq!(server.count_path("/asset"), 1);
}

/// 删除非 active 版本需确认；active 行没有使用/删除按钮（共享层同样拒绝）。
#[test]
fn delete_inactive_needs_confirmation_and_active_row_has_no_actions() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        let previous = seed_core(root, CoreType::SingBox, "1.2.2", "1.2.2");
        let active = seed_core(root, CoreType::SingBox, "1.2.3", "1.2.3");
        save_core_config(root, &active);
        assert_eq!(previous, core_binary(root, CoreType::SingBox, "1.2.2"));
    });
    let inactive = core_binary(dir.path(), CoreType::SingBox, "1.2.2");
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    open_core_page(&mut app);
    app.wait_for("sing-box · v1.2.3").unwrap();

    // active 行无操作按钮（删除 active 共享层也会拒绝）。
    assert!(app.press("id:core-delete-sing-box-1.2.3").is_err());
    assert!(app.press("id:core-use-sing-box-1.2.3").is_err());

    // 取消：文件保留。
    app.press("id:core-delete-sing-box-1.2.2").unwrap();
    app.wait_for("删除核心 v1.2.2？").unwrap();
    app.press("取消").unwrap();
    assert!(inactive.is_file());

    // 确认：版本目录删除，active 保留。
    app.press("id:core-delete-sing-box-1.2.2").unwrap();
    app.wait_for("删除核心 v1.2.2？").unwrap();
    app.press("确认删除").unwrap();
    app.wait_for_within("text:/已删除/", Duration::from_secs(10))
        .unwrap();
    assert!(!inactive.exists(), "版本目录已删除");
    assert!(core_binary(dir.path(), CoreType::SingBox, "1.2.3").is_file());
    app.wait_for("sing-box · v1.2.3").unwrap();
    assert!(app.press("id:core-delete-sing-box-1.2.2").is_err());
}

/// 使用本地版本：version probe 真实执行、active 摘要切换、配置其余字段
/// （active 订阅、请求策略、端口等）不被无关改写。
#[cfg(unix)]
#[test]
fn selecting_a_local_version_switches_active_and_preserves_unrelated_config() {
    let (dir, entries) = data_root();
    let active_id = uuid::Uuid::new_v4();
    init_root_then(&entries, dir.path(), |root| {
        let previous = seed_core(root, CoreType::SingBox, "1.2.2", "1.2.2");
        let latest = seed_core(root, CoreType::SingBox, "1.2.3", "1.2.3");
        let mut config =
            ClientConfig::new(root.to_path_buf(), "https://hub.example", "token", previous);
        config.active_subscription_id = Some(active_id);
        config.request_proxy_mode = RequestProxyMode::Custom;
        config.request_proxy_url = "http://127.0.0.1:8080".into();
        config.mixed_port = 18765;
        config.save().unwrap();
        assert_eq!(latest, core_binary(root, CoreType::SingBox, "1.2.3"));
    });
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    open_core_page(&mut app);
    app.wait_for("sing-box · v1.2.2").unwrap();

    app.press("id:core-use-sing-box-1.2.3").unwrap();
    app.wait_for("sing-box · v1.2.3").unwrap();
    // 共享活动清除会先触发摘要重读；本页完成通知随后到达，单独等待。
    app.wait_for_within("text:/已选中/", Duration::from_secs(10))
        .unwrap();

    let config = ClientConfig::load(dir.path()).unwrap();
    assert_eq!(
        config.core_binary,
        core_binary(dir.path(), CoreType::SingBox, "1.2.3")
    );
    assert_eq!(config.core_type, CoreType::SingBox);
    assert_eq!(config.active_subscription_id, Some(active_id));
    assert_eq!(config.request_proxy_mode, RequestProxyMode::Custom);
    assert_eq!(config.request_proxy_url, "http://127.0.0.1:8080");
    assert_eq!(config.mixed_port, 18765);
}

/// Mihomo 选中：摘要显示 mihomo；配置页预览已支持 Mihomo（YAML），
/// 核心页不再提示“暂不支持”。浏览 Mihomo 库存本地行真实列出。
#[cfg(unix)]
#[test]
fn mihomo_selection_shows_summary_and_config_preview_note() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        let mihomo = seed_core(root, CoreType::Mihomo, "1.19.29", "1.19.29");
        save_core_config(root, &mihomo);
    });
    let mut app = start_app(entries).0;
    common::wait_ready(&mut app);
    open_core_page(&mut app);
    app.wait_for("mihomo · v1.19.29").unwrap();
    let seen = texts(&app);
    assert!(
        seen.iter()
            .all(|text| !text.contains("配置页合成预览暂不支持 Mihomo")),
        "预览已支持 Mihomo，不得再提示暂不支持:{seen:?}"
    );
    // 浏览 Mihomo 库存：本地行真实列出。
    app.press("id:core-type-mihomo").unwrap();
    app.wait_for("v1.19.29").unwrap();
}

// ---------- 运行卡片（校验并启动 / 停止） ----------
//
// 下列用例使用 rustc 现场编译的 fixture 假核心（真实进程/真实监听/真实
// 回收），不是真实 sing-box/Mihomo；Windows 真实核心验收由主代理执行。

/// 概览页校验并启动 → 真实进程/真实监听 → 核心页同步显示“运行中”徽标（不重复渲染启停按钮）→ 概览页停止回收。
#[test]
fn start_button_validates_launches_and_stops_core() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        seed_runtime_target(root, CoreType::SingBox, "1.2.3", 18301, 18302);
    });
    let mut app = start_app(entries).0;
    app.wait_for("id:overview-start").unwrap();
    app.press("id:overview-start").unwrap();
    app.wait_for_within(
        "text:/已完成二进制校验并启动：sing-box v1.2.3 · mixed 18301 · API 18302/",
        Duration::from_secs(45),
    )
    .unwrap();
    app.wait_for_within(
        "text:/运行中：sing-box v1.2.3 · mixed 18301 · API 18302/",
        Duration::from_secs(30),
    )
    .unwrap();
    // 真实监听：mixed 端口在宿主上可连接。
    std::net::TcpStream::connect(("127.0.0.1", 18301)).expect("mixed listener is real");

    // 切到核心页：展示“运行中”徽标且不渲染重复的启停按钮。
    open_core_page(&mut app);
    app.wait_for_within("运行中", Duration::from_secs(10))
        .unwrap();
    assert!(!app.screen().matches("id:core-start").unwrap());
    assert!(!app.screen().matches("id:core-stop").unwrap());

    common::navigate(&mut app, "概览");
    app.wait_for("id:overview-stop").unwrap();
    app.press("id:overview-stop").unwrap();
    app.wait_for_within(
        "已停止核心（强制终止并回收本次进程）",
        Duration::from_secs(30),
    )
    .unwrap();
    app.wait_for_within("未运行", Duration::from_secs(30))
        .unwrap();
    std::net::TcpStream::connect(("127.0.0.1", 18301)).expect_err("停止后端口释放");
}

/// 二进制校验失败：真实错误上屏（含核心诊断），不进入运行态。
#[test]
fn start_failure_shows_real_binary_check_error() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        let (mixed, api) = common::available_loopback_ports();
        let binary = seed_runtime_target(root, CoreType::SingBox, "1.2.3", mixed, api);
        std::fs::write(
            binary.with_extension("behavior"),
            br#"{"fixture_fail":"ui-check-boom"}"#,
        )
        .unwrap();
    });
    let mut app = start_app(entries).0;
    app.wait_for("id:overview-start").unwrap();
    app.press("id:overview-start").unwrap();
    app.wait_for_within("text:/启动核心失败/", Duration::from_secs(45))
        .unwrap();
    app.wait_for_within("text:/核心配置校验失败/", Duration::from_secs(5))
        .unwrap();
    app.wait_for_within("text:/ui-check-boom/", Duration::from_secs(5))
        .unwrap();
    app.wait_for_within("未运行", Duration::from_secs(30))
        .unwrap();
}

/// 运行状态跨页共享：概览启动后切到订阅页再切核心页，仍显示运行中；随后回概览停止。
#[test]
fn running_state_is_shared_across_page_switches() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        seed_runtime_target(root, CoreType::SingBox, "1.2.3", 18321, 18322);
    });
    let mut app = start_app(entries).0;
    app.wait_for("id:overview-start").unwrap();
    app.press("id:overview-start").unwrap();
    app.wait_for_within("text:/运行中：sing-box v1.2.3/", Duration::from_secs(45))
        .unwrap();

    common::open_subscriptions(&mut app);
    app.wait_for("id:nav-overview").unwrap();
    common::navigate(&mut app, "核心");
    app.wait_for_within(
        "text:/运行中：sing-box v1.2.3 · mixed 18321/",
        Duration::from_secs(30),
    )
    .unwrap();

    common::navigate(&mut app, "概览");
    app.wait_for("id:overview-stop").unwrap();
    app.press("id:overview-stop").unwrap();
    app.wait_for_within(
        "已停止核心（强制终止并回收本次进程）",
        Duration::from_secs(30),
    )
    .unwrap();
}

/// 运行中库存选择被业务层真实拒绝（不是按钮置灰）；停止后恢复可操作。
#[test]
fn inventory_select_is_rejected_while_core_is_running() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        seed_runtime_target(root, CoreType::SingBox, "1.2.3", 18331, 18332);
        common::seed_fixture_core(root, CoreType::SingBox, "1.2.2");
    });
    let mut app = start_app(entries).0;
    app.wait_for("id:overview-start").unwrap();
    app.press("id:overview-start").unwrap();
    app.wait_for_within("text:/运行中：sing-box v1.2.3/", Duration::from_secs(45))
        .unwrap();

    open_core_page(&mut app);
    app.wait_for("v1.2.2").unwrap();
    app.press("id:core-use-sing-box-1.2.2").unwrap();
    app.wait_for_within(
        "text:/核心运行期间不可修改核心文件或选择/",
        Duration::from_secs(30),
    )
    .unwrap();

    common::navigate(&mut app, "概览");
    app.wait_for("id:overview-stop").unwrap();
    app.press("id:overview-stop").unwrap();
    app.wait_for_within(
        "已停止核心（强制终止并回收本次进程）",
        Duration::from_secs(30),
    )
    .unwrap();
    open_core_page(&mut app);
    app.wait_for("id:core-use-sing-box-1.2.2").unwrap();
    app.press("id:core-use-sing-box-1.2.2").unwrap();
    app.wait_for_within("text:/已选中 v1.2.2/", Duration::from_secs(30))
        .unwrap();
    let config = ClientConfig::load(dir.path()).unwrap();
    assert_eq!(
        config.core_binary,
        core_binary(dir.path(), CoreType::SingBox, "1.2.2")
    );
}

/// 核心异常退出由状态轮询真实发现并上屏（不只记按钮状态）。
#[test]
fn unexpected_exit_is_detected_by_the_status_poll() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        let binary = seed_runtime_target(root, CoreType::SingBox, "1.2.3", 18341, 18342);
        std::fs::write(
            binary.with_extension("behavior"),
            br#"{"fixture_exit_after_ms":2500}"#,
        )
        .unwrap();
    });
    let mut app = start_app(entries).0;
    app.wait_for("id:overview-start").unwrap();
    app.press("id:overview-start").unwrap();
    app.wait_for_within("text:/运行中：sing-box v1.2.3/", Duration::from_secs(45))
        .unwrap();
    open_core_page(&mut app);
    // fixture 2.5 秒后自行退出；轮询必须发现异常退出而不是继续显示运行中。
    app.wait_for_within("text:/核心进程异常退出/", Duration::from_secs(45))
        .unwrap();
    app.wait_for_within("未运行", Duration::from_secs(30))
        .unwrap();
}

/// 启用系统代理前的一次性确认框：取消不写配置、不授权，开关保持关闭。
#[test]
fn cancelling_proxy_enable_confirmation_writes_nothing() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries.clone()).0;
    common::wait_ready(&mut app);
    open_core_page(&mut app);
    let config = std::path::Path::new(&entries["lanpath_desktop_nsh.data_dir"]).join("app.toml");
    let before = std::fs::read(&config).expect("read app.toml");

    app.press("id:proxy-toggle >> role:switch").unwrap();
    app.wait_for("启用系统代理接管？").unwrap();
    let seen = texts(&app);
    for expected in ["仅本次接管", "精确归还", "不保存", "重新确认"] {
        assert!(
            seen.iter().any(|text| text.contains(expected)),
            "missing {expected}: {seen:?}"
        );
    }
    app.press("id:proxy-confirm-cancel").unwrap();

    // 取消：配置字节不变（含默认 system_proxy_enabled=false 不被重写）。
    assert_eq!(std::fs::read(&config).expect("reread"), before);
    // 开关仍关：再次按下重新弹确认框。
    app.press("id:proxy-toggle >> role:switch").unwrap();
    app.wait_for("启用系统代理接管？").unwrap();
}

/// 确认后保存偏好并授予进程内一次性授权；授权不持久化（app.toml 字节
/// 只反映偏好字段本身）。
#[test]
fn confirming_proxy_enable_saves_preference_only() {
    let (_dir, entries) = data_root();
    let mut app = start_app(entries.clone()).0;
    common::wait_ready(&mut app);
    open_core_page(&mut app);
    let config = std::path::Path::new(&entries["lanpath_desktop_nsh.data_dir"]).join("app.toml");

    app.press("id:proxy-toggle >> role:switch").unwrap();
    app.wait_for("启用系统代理接管？").unwrap();
    app.press("id:proxy-confirm-accept").unwrap();
    app.wait_for_within("text:/系统代理偏好已保存/", Duration::from_secs(10))
        .unwrap();

    let saved = String::from_utf8(std::fs::read(&config).expect("read app.toml")).unwrap();
    assert!(
        saved.contains("system_proxy_enabled = true"),
        "偏好已保存: {saved}"
    );
}

/// 启停命令在途时修改系统代理偏好必须在写配置/授权前拒绝：不能先保存新
/// 偏好却因共享命令忙而不重启，导致界面偏好与运行核心快照相反。
#[test]
fn proxy_preference_change_is_rejected_while_a_core_command_is_busy() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        seed_runtime_target(root, CoreType::SingBox, "1.2.3", 18397, 18398);
    });
    let mut app = start_app(entries).0;
    app.wait_for("id:overview-start").unwrap();
    let config = dir.path().join("app.toml");
    let before = std::fs::read(&config).expect("read app.toml before start");

    app.press("id:overview-start").unwrap();
    open_core_page(&mut app);
    app.press("id:proxy-toggle >> role:switch").unwrap();
    assert!(
        !texts(&app)
            .iter()
            .any(|text| text.contains("启用系统代理接管？")),
        "命令在途时不得先弹确认/写偏好"
    );
    app.wait_for_within("text:/核心启动或停止进行中/", Duration::from_secs(10))
        .unwrap();
    assert_eq!(
        std::fs::read(&config).expect("read app.toml during start"),
        before,
        "拒绝必须发生在写配置之前"
    );

    app.wait_for_within("text:/运行中：sing-box v1.2.3/", Duration::from_secs(45))
        .unwrap();
    common::navigate(&mut app, "概览");
    app.wait_for("id:overview-stop").unwrap();
    app.press("id:overview-stop").unwrap();
    app.wait_for_within("text:/已停止核心/", Duration::from_secs(30))
        .unwrap();
}
