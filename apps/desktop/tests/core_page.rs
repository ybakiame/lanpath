//! Headless 测试（`nsh_testing::Headless`，NSH 0.5.0）：核心页（核心库存与
//! 下载/选择）。真实联网场景经本线程 127.0.0.1 HTTP 服务器（release metadata
//! JSON 回指本服务器资产 + 真实 tar.gz 归档），不依赖互联网；Unix 用脚本
//! 核心做真实 version probe，Windows 原生验收由主代理执行。每个场景独立
//! 临时数据根；release metadata API 经 storage 键
//! `lanpath_desktop_nsh.core_api_base` 注入（生产默认 GitHub）。

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{TestRoute, TestServer, data_root, start_app, texts};
use lanpath_client::{ClientConfig, RequestProxyMode};
use lanpath_common::CoreType;

const API_BASE_KEY: &str = "lanpath_desktop_nsh.core_api_base";

fn open_core_page(app: &mut nsh_testing::Headless) {
    app.press("核心").unwrap();
    app.wait_for("管理本数据根 cores/ 下的核心文件 · 校验并启动为真实二进制校验后的用户态 loopback 运行，不接管系统代理/TUN")
        .unwrap();
}

/// 数据根内的库存二进制路径（共享 on-disk 命名规则）。
fn core_binary(root: &Path, core_type: CoreType, version: &str) -> PathBuf {
    let name = if cfg!(windows) {
        format!("{}.exe", core_type.binary_name())
    } else {
        core_type.binary_name().to_string()
    };
    root.join("cores")
        .join(core_type.binary_name())
        .join(version)
        .join(name)
}

/// 种子一个库存核心：Unix 为可真实 version probe 的脚本核心，其他平台为
/// 仅用于列表/删除路径的元数据文件（选择/下载的 probe 原生验收由主代理做）。
fn seed_core(root: &Path, core_type: CoreType, version: &str, reported: &str) -> PathBuf {
    let path = core_binary(root, core_type, version);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    #[cfg(unix)]
    {
        let output = match core_type {
            CoreType::SingBox => format!("sing-box version {reported}"),
            CoreType::Mihomo => format!("Mihomo Meta v{reported} linux amd64"),
        };
        std::fs::write(&path, format!("#!/bin/sh\necho '{output}'\n")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    #[cfg(not(unix))]
    std::fs::write(&path, b"inventory metadata fixture").unwrap();
    path
}

/// 持久化选中核心（core_binary 指向种子路径，core_type 由共享推断）。
fn save_core_config(root: &Path, binary: &Path) {
    ClientConfig::new(
        root.to_path_buf(),
        "https://hub.example",
        "token",
        binary.to_path_buf(),
    )
    .save()
    .unwrap();
}

/// 先启动一次应用初始化数据根（写入归属标记），再预置库存与配置。
fn init_root_then(
    entries: &std::collections::BTreeMap<String, String>,
    root: &Path,
    seed: impl FnOnce(&Path),
) {
    {
        let mut app = start_app(entries.clone()).0;
        app.wait_for("尚无订阅。").unwrap();
    }
    seed(root);
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
    app.wait_for("尚无订阅。").unwrap();
    app.press("核心").unwrap();
    app.wait_for("尚未选择核心").unwrap();
    let seen = texts(&app);
    for expected in [
        "管理本数据根 cores/ 下的核心文件 · 校验并启动为真实二进制校验后的用户态 loopback 运行，不接管系统代理/TUN",
        "校验并启动：先以真实核心二进制校验配置（sing-box check / Mihomo -t），再以用户态 loopback mixed + Clash API 启动 · 不接管系统代理/TUN",
        "未运行",
        "此通道尚无本地版本，可切换通道或检查更新后下载。",
        "本地：未安装",
        "远程：尚未检查",
        "sing-box",
        "Mihomo",
        "稳定版",
        "预发布版",
    ] {
        assert!(
            seen.iter().any(|text| text.contains(expected)),
            "missing {expected}: {seen:?}"
        );
    }
    // 运行卡片按钮存在（fixture 启动用例见下文；这里只验证空态展示）。
    assert!(seen.contains(&"校验并启动"), "{seen:?}");
    assert!(seen.contains(&"停止"), "{seen:?}");
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
    app.wait_for("尚无订阅。").unwrap();
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
    app.wait_for("尚无订阅。").unwrap();
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
    app.wait_for("尚无订阅。").unwrap();
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
    app.wait_for("尚无订阅。").unwrap();
    open_core_page(&mut app);
    app.press("id:core-check-remote").unwrap();
    app.wait_for("远程：v1.2.3").unwrap();
    let seen = texts(&app);
    assert!(
        !seen.iter().any(|text| text.contains("检查更新失败")),
        "成功后旧错误清除: {seen:?}"
    );
}

/// 下载 busy 全局单飞（重复点击不重复请求）；下载中途离开页面再回来
/// 不永久 busy、可立即查询；但旧下载仍在后台时，新页面的下载/选择/删除
/// 被业务层同根 mutation 锁立即拒绝（真实 busy 错误，不产生第二个资产
/// 请求）；旧下载完成后锁释放，可再次下载（缓存重新校验并选中）。
#[cfg(unix)]
#[test]
fn download_is_single_flight_and_recovers_after_leaving_the_page() {
    let (dir, mut entries) = data_root();
    let server = release_server("1.2.3", Duration::from_millis(1500));
    entries.insert(API_BASE_KEY.to_owned(), server.url(""));
    let mut app = start_app(entries).0;
    app.wait_for("尚无订阅。").unwrap();
    open_core_page(&mut app);
    app.press("id:core-check-remote").unwrap();
    app.wait_for("远程：v1.2.3").unwrap();

    app.press("id:core-download").unwrap();
    // busy 中重复点击 no-op（全局单飞，不启动第二个下载）。
    app.press("id:core-download").unwrap();

    // 下载在途时离开页面再回来：全新页面状态，不永久 busy，可查询。
    app.press("订阅").unwrap();
    app.wait_for("尚无订阅。").unwrap();
    app.press("核心").unwrap();
    app.wait_for("尚未选择核心").unwrap();
    app.press("id:core-check-remote").unwrap();
    app.wait_for("远程：v1.2.3").unwrap();

    // 旧下载仍在后台（离页的 owner 只取消 UI 轮询）：新页面再下载被
    // 业务层 mutation 锁立即拒绝——真实 busy 错误，且不产生第二个资产请求。
    app.press("id:core-download").unwrap();
    app.wait_for_within("text:/另一项核心库存操作正在进行/", Duration::from_secs(10))
        .unwrap();
    assert_eq!(server.count_path("/asset"), 1, "不产生第二个资产请求");

    // 旧下载真实完成：落盘并释放锁。
    let binary = core_binary(dir.path(), CoreType::SingBox, "1.2.3");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !binary.is_file() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(binary.is_file(), "在途下载完成并落盘: {binary:?}");
    assert_eq!(server.count_path("/asset"), 1, "单飞：资产只下载一次");

    // 锁释放后可再操作：缓存路径重新校验并自动选中。
    app.press("id:core-download").unwrap();
    app.wait_for("sing-box · v1.2.3").unwrap();
    assert_eq!(server.count_path("/asset"), 1);
    let config = ClientConfig::load(dir.path()).unwrap();
    assert_eq!(config.core_binary, binary);
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
    app.wait_for("尚无订阅。").unwrap();
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
    app.wait_for("尚无订阅。").unwrap();
    open_core_page(&mut app);
    app.wait_for("sing-box · v1.2.2").unwrap();

    app.press("id:core-use-sing-box-1.2.3").unwrap();
    app.wait_for("sing-box · v1.2.3").unwrap();
    let seen = texts(&app);
    assert!(
        seen.iter().any(|text| text.contains("已选中")),
        "选择成功反馈: {seen:?}"
    );

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
    app.wait_for("尚无订阅。").unwrap();
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

const CLASH_YAML: &str = "proxies:\n  - name: 节点甲\n    type: ss\n    server: 192.0.2.1\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n  - name: 节点乙\n    type: ss\n    server: 192.0.2.2\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n";

/// 种子 fixture 核心 + 已启用生效订阅（含原文/缓存）+ 端口与核心选择。
fn seed_runtime_target(
    root: &Path,
    core_type: CoreType,
    version: &str,
    mixed: u16,
    api: u16,
) -> PathBuf {
    let binary = common::seed_fixture_core(root, core_type, version);
    let store = lanpath_client::SubscriptionStore::new(root.to_path_buf());
    let subscription = store
        .add("运行订阅", "https://example.com/c.yaml", true, None)
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
                singbox_nodes: fetched.singbox_nodes,
            },
        )
        .unwrap();
    let mut config = ClientConfig::load_for_requests(root).unwrap();
    config.active_subscription_id = Some(subscription.id);
    config.core_binary = binary.clone();
    config.mixed_port = mixed;
    config.clash_api_port = api;
    config.save().unwrap();
    binary
}

/// 校验并启动 → 真实进程/真实监听 → 停止回收：结果文案明确“已完成二进制
/// 校验”，状态行显示真实版本/端口，停止回执含真实 kind。
#[test]
fn start_button_validates_launches_and_stops_core() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        seed_runtime_target(root, CoreType::SingBox, "1.2.3", 18301, 18302);
    });
    let mut app = start_app(entries).0;
    app.wait_for("Lanpath NSH Desktop").unwrap();
    open_core_page(&mut app);
    app.wait_for_within("未运行", Duration::from_secs(30))
        .unwrap();

    app.press("id:core-start").unwrap();
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

    app.press("id:core-stop").unwrap();
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
        let binary = seed_runtime_target(root, CoreType::SingBox, "1.2.3", 18311, 18312);
        std::fs::write(
            binary.with_extension("behavior"),
            br#"{"fixture_fail":"ui-check-boom"}"#,
        )
        .unwrap();
    });
    let mut app = start_app(entries).0;
    app.wait_for("Lanpath NSH Desktop").unwrap();
    open_core_page(&mut app);
    app.press("id:core-start").unwrap();
    app.wait_for_within("text:/启动核心失败/", Duration::from_secs(45))
        .unwrap();
    app.wait_for_within("text:/核心配置校验失败/", Duration::from_secs(5))
        .unwrap();
    app.wait_for_within("text:/ui-check-boom/", Duration::from_secs(5))
        .unwrap();
    app.wait_for_within("未运行", Duration::from_secs(30))
        .unwrap();
}

/// 运行状态跨页共享：启动后切到订阅页再回核心页，仍显示运行中（不是
/// 页面级按钮状态）；随后停止。
#[test]
fn running_state_is_shared_across_page_switches() {
    let (dir, entries) = data_root();
    init_root_then(&entries, dir.path(), |root| {
        seed_runtime_target(root, CoreType::SingBox, "1.2.3", 18321, 18322);
    });
    let mut app = start_app(entries).0;
    app.wait_for("Lanpath NSH Desktop").unwrap();
    open_core_page(&mut app);
    app.press("id:core-start").unwrap();
    app.wait_for_within("text:/运行中：sing-box v1.2.3/", Duration::from_secs(45))
        .unwrap();

    app.press("订阅").unwrap();
    app.wait_for("Lanpath NSH Desktop").unwrap();
    app.press("核心").unwrap();
    app.wait_for_within(
        "text:/运行中：sing-box v1.2.3 · mixed 18321/",
        Duration::from_secs(30),
    )
    .unwrap();

    app.press("id:core-stop").unwrap();
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
    app.wait_for("Lanpath NSH Desktop").unwrap();
    open_core_page(&mut app);
    app.wait_for("v1.2.2").unwrap();
    app.press("id:core-start").unwrap();
    app.wait_for_within("text:/运行中：sing-box v1.2.3/", Duration::from_secs(45))
        .unwrap();

    app.press("id:core-use-sing-box-1.2.2").unwrap();
    app.wait_for_within(
        "text:/核心运行期间不可修改核心文件或选择/",
        Duration::from_secs(30),
    )
    .unwrap();

    app.press("id:core-stop").unwrap();
    app.wait_for_within(
        "已停止核心（强制终止并回收本次进程）",
        Duration::from_secs(30),
    )
    .unwrap();
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
    app.wait_for("Lanpath NSH Desktop").unwrap();
    open_core_page(&mut app);
    app.press("id:core-start").unwrap();
    app.wait_for_within("text:/运行中：sing-box v1.2.3/", Duration::from_secs(45))
        .unwrap();
    // fixture 2.5 秒后自行退出；轮询必须发现异常退出而不是继续显示运行中。
    app.wait_for_within("text:/核心进程异常退出/", Duration::from_secs(45))
        .unwrap();
    app.wait_for_within("未运行", Duration::from_secs(30))
        .unwrap();
}
