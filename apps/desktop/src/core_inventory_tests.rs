//! 核心库存业务测试（纯 Rust，本线程 127.0.0.1 最小 HTTP 服务器 + 真实
//! tar.gz/gzip 归档 fixture，不依赖互联网）。Unix 用脚本核心做真实 version
//! probe；Windows 原生验收由主代理执行。

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lanpath_client::RequestProxyMode;
use lanpath_common::CoreType;

use super::CoreInventory;

/// 数据根内的库存二进制路径（按共享 on-disk 命名规则）。
fn binary_at(root: &Path, core_type: CoreType, version: &str) -> PathBuf {
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

/// 种子一个可真实 version probe 的脚本核心（Unix）。
#[cfg(unix)]
fn seed_script_core(root: &Path, core_type: CoreType, version: &str, reported: &str) -> PathBuf {
    let path = binary_at(root, core_type, version);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let output = match core_type {
        CoreType::SingBox => format!("sing-box version {reported}"),
        CoreType::Mihomo => format!("Mihomo Meta v{reported} linux amd64"),
    };
    std::fs::write(&path, format!("#!/bin/sh\necho '{output}'\n")).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// 种子一个不可执行的库存元数据文件（Windows 原生场景不执行 probe，
/// 行为由主代理验收；此处仅用于列表/删除路径）。
#[cfg(not(unix))]
fn seed_script_core(root: &Path, core_type: CoreType, version: &str, _reported: &str) -> PathBuf {
    let path = binary_at(root, core_type, version);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"inventory fixture").unwrap();
    path
}

// ---------- 本地 HTTP fixture ----------

#[derive(Clone)]
struct Route {
    /// 以该后缀匹配的请求路径（含 query 时给出完整后缀）。
    path_suffix: String,
    status: u16,
    body: Vec<u8>,
    /// 响应前延迟（busy/互斥窗口用）。
    delay: Duration,
}

/// 最小 HTTP/1.1 服务器：按路径后缀路由、`{addr}` 占位符替换为实际地址
/// （release JSON 里回指本服务器资产）、记录请求路径、`Connection: close`。
struct FixtureServer {
    addr: std::net::SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
}

impl FixtureServer {
    fn new(routes: Vec<Route>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_requests = requests.clone();
        let thread_stop = stop.clone();
        std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let requests = thread_requests.clone();
                        let routes = routes.clone();
                        let addr = addr.to_string();
                        std::thread::spawn(move || {
                            handle_connection(stream, requests, routes, addr)
                        });
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(2)),
                }
            }
        });
        Self {
            addr,
            requests,
            stop,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    fn base(&self) -> String {
        self.url("")
    }

    fn recorded(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for FixtureServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn handle_connection(
    mut stream: TcpStream,
    requests: Arc<Mutex<Vec<String>>>,
    routes: Vec<Route>,
    addr: String,
) {
    let mut head = Vec::new();
    let mut buffer = [0_u8; 4096];
    while !head.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => head.extend_from_slice(&buffer[..read]),
            Err(_) => return,
        }
        if head.len() > 16384 {
            return;
        }
    }
    let request = String::from_utf8_lossy(&head);
    let target = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_string();
    requests.lock().unwrap().push(target.clone());
    let route = routes
        .iter()
        .find(|route| target.ends_with(&route.path_suffix));
    let (status, body, delay) = match route {
        Some(route) => (route.status, route.body.clone(), route.delay),
        None => (404, Vec::new(), Duration::ZERO),
    };
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
    let body = replace_addr_placeholder(&body, &addr);
    let reason = if (200..300).contains(&status) {
        "OK"
    } else {
        "ERR"
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/octet-stream\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n",
        length = body.len(),
    );
    let mut bytes = response.into_bytes();
    bytes.extend_from_slice(&body);
    let _ = stream.write_all(&bytes);
}

fn replace_addr_placeholder(body: &[u8], addr: &str) -> Vec<u8> {
    let text = String::from_utf8_lossy(body);
    if text.contains("{addr}") {
        text.replace("{addr}", addr).into_bytes()
    } else {
        body.to_vec()
    }
}

/// 真实 tar.gz 归档：内含可执行脚本核心（共享 extract_tgz 按文件名匹配）。
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

/// Mihomo Unix 发布形态：单文件 gzip。
#[cfg(unix)]
fn mihomo_gz(reported: &str) -> Vec<u8> {
    let script = format!("#!/bin/sh\necho 'Mihomo Meta v{reported} linux amd64'\n");
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(script.as_bytes()).unwrap();
    encoder.finish().unwrap()
}

#[cfg(unix)]
fn singbox_asset_name(version: &str) -> String {
    // 与共享 `target_spec_for` 同一平台映射（测试需要期望资产名）。
    let (platform, is_windows) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => ("linux-amd64", false),
        ("linux", "aarch64") => ("linux-arm64", false),
        ("macos", "x86_64") => ("darwin-amd64", false),
        ("macos", "aarch64") => ("darwin-arm64", false),
        (os, arch) => panic!("unsupported core download test platform: {os}-{arch}"),
    };
    format!(
        "sing-box-{version}-{platform}.{}",
        if is_windows { "zip" } else { "tar.gz" }
    )
}

fn release_json(asset_name: &str) -> String {
    format!(
        "{{\"assets\":[{{\"name\":\"{asset_name}\",\"browser_download_url\":\"http://{{addr}}/asset\"}}]}}"
    )
}

#[test]
fn empty_root_lists_no_cores_and_no_active() {
    let root = tempfile::tempdir().unwrap();
    let inventory = CoreInventory::for_root(root.path()).unwrap();
    assert!(inventory.list_local(CoreType::SingBox).unwrap().is_empty());
    assert!(inventory.list_local(CoreType::Mihomo).unwrap().is_empty());
    assert!(inventory.active_core().unwrap().is_none());
}

#[test]
fn type_switch_lists_independent_inventories_and_active_survives() {
    let root = tempfile::tempdir().unwrap();
    // 先开数据根（写入归属标记），再预置库存文件与配置。
    let inventory = CoreInventory::for_root(root.path()).unwrap();
    let singbox = seed_script_core(root.path(), CoreType::SingBox, "1.2.3", "1.2.3");
    let mihomo = seed_script_core(root.path(), CoreType::Mihomo, "1.19.29", "1.19.29");
    inventory.select(CoreType::SingBox, &singbox).unwrap();

    let singbox_list = inventory.list_local(CoreType::SingBox).unwrap();
    assert_eq!(singbox_list.len(), 1);
    assert!(singbox_list[0].is_active());
    assert_eq!(singbox_list[0].version(), "1.2.3");
    let mihomo_list = inventory.list_local(CoreType::Mihomo).unwrap();
    assert_eq!(mihomo_list.len(), 1);
    assert!(!mihomo_list[0].is_active());

    let active = inventory.active_core().unwrap().unwrap();
    assert_eq!(active.core_type(), CoreType::SingBox);
    assert_eq!(active.version(), "1.2.3");

    // 切换 Mihomo 选中：active 摘要独立更新，sing-box 库存行不再 active。
    inventory.select(CoreType::Mihomo, &mihomo).unwrap();
    let active = inventory.active_core().unwrap().unwrap();
    assert_eq!(active.core_type(), CoreType::Mihomo);
    assert_eq!(active.version(), "1.19.29");
    assert!(!inventory.list_local(CoreType::SingBox).unwrap()[0].is_active());
    let config = lanpath_client::ClientConfig::load(root.path()).unwrap();
    assert_eq!(config.core_type, CoreType::Mihomo);
    assert_eq!(config.core_binary, mihomo);
}

#[test]
fn select_preserves_existing_config_fields_and_request_policy() {
    let root = tempfile::tempdir().unwrap();
    let inventory = CoreInventory::for_root(root.path()).unwrap();
    let selected = seed_script_core(root.path(), CoreType::SingBox, "1.2.3", "1.2.3");
    let mut config = lanpath_client::ClientConfig::new(
        root.path().to_path_buf(),
        "https://hub.example",
        "token",
        PathBuf::new(),
    );
    config.request_proxy_mode = RequestProxyMode::Custom;
    config.request_proxy_url = "http://127.0.0.1:8080".into();
    config.mixed_port = 18765;
    config.save().unwrap();
    let before = lanpath_client::ClientConfig::load(root.path()).unwrap();

    inventory.select(CoreType::SingBox, &selected).unwrap();

    let after = lanpath_client::ClientConfig::load(root.path()).unwrap();
    assert_eq!(after.core_binary, selected);
    assert_eq!(after.core_type, CoreType::SingBox);
    assert_eq!(after.request_proxy_mode, before.request_proxy_mode);
    assert_eq!(after.request_proxy_url, before.request_proxy_url);
    assert_eq!(after.mixed_port, before.mixed_port);
    assert_eq!(after.hub_url, before.hub_url);
    assert_eq!(after.active_subscription_id, before.active_subscription_id);
    // 统一 TOML 布局：配置与选择都只写 app.toml，不再产生 client.json。
    assert!(lanpath_client::settings::app_settings_path(root.path()).is_file());
    assert!(!root.path().join("client.json").exists());
}

#[test]
fn select_on_fresh_install_persists_direct_request_default() {
    let root = tempfile::tempdir().unwrap();
    let inventory = CoreInventory::for_root(root.path()).unwrap();
    let selected = seed_script_core(root.path(), CoreType::SingBox, "1.2.3", "1.2.3");
    inventory.select(CoreType::SingBox, &selected).unwrap();

    let config = lanpath_client::ClientConfig::load(root.path()).unwrap();
    assert_eq!(config.core_binary, selected);
    assert_eq!(
        config.request_proxy_mode,
        RequestProxyMode::None,
        "新装默认请求策略必须固化为直连，不能被核心选择改回 System"
    );
    assert!(lanpath_client::settings::app_settings_path(root.path()).is_file());
    assert!(!root.path().join("client.json").exists());
}

#[test]
fn delete_active_rejected_and_inactive_removed() {
    let root = tempfile::tempdir().unwrap();
    let inventory = CoreInventory::for_root(root.path()).unwrap();
    let active = seed_script_core(root.path(), CoreType::SingBox, "1.2.3", "1.2.3");
    let inactive = seed_script_core(root.path(), CoreType::SingBox, "1.2.2", "1.2.2");
    inventory.select(CoreType::SingBox, &active).unwrap();

    assert!(inventory.delete(CoreType::SingBox, &active).is_err());
    assert!(active.exists());
    inventory.delete(CoreType::SingBox, &inactive).unwrap();
    assert!(!inactive.parent().unwrap().exists());
    assert!(active.exists());
    assert_eq!(inventory.list_local(CoreType::SingBox).unwrap().len(), 1);
}

#[tokio::test]
async fn remote_channels_filtered_to_stable_and_prerelease() {
    let root = tempfile::tempdir().unwrap();
    let server = FixtureServer::new(vec![Route {
        path_suffix: "/releases?per_page=30".into(),
        status: 200,
        delay: Duration::ZERO,
        body:
            br#"[{"tag_name":"v1.2.3"},{"tag_name":"v1.3.0-beta.1"},{"tag_name":"v1.3.0-alpha.1"}]"#
                .to_vec(),
    }]);
    let inventory = CoreInventory::for_root_with_api_base(root.path(), server.base()).unwrap();
    let channels = inventory.remote_channels(CoreType::SingBox).await.unwrap();
    let pairs: Vec<_> = channels
        .iter()
        .map(|channel| (channel.channel().to_owned(), channel.version().to_owned()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("stable".to_string(), "1.2.3".to_string()),
            ("prerelease".to_string(), "1.3.0-alpha.1".to_string()),
        ],
        "beta 通道被过滤，stable/prerelease 保留: {pairs:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn download_installs_verifies_selects_and_prunes_old_channel_versions() {
    let root = tempfile::tempdir().unwrap();
    let inventory = CoreInventory::for_root(root.path()).unwrap();
    let old = seed_script_core(root.path(), CoreType::SingBox, "1.2.1", "1.2.1");
    let previous = seed_script_core(root.path(), CoreType::SingBox, "1.2.2", "1.2.2");
    inventory.select(CoreType::SingBox, &previous).unwrap();

    let server = FixtureServer::new(vec![
        Route {
            path_suffix: "/repos/SagerNet/sing-box/releases/tags/v1.2.3".into(),
            status: 200,
            delay: Duration::ZERO,
            body: release_json(&singbox_asset_name("1.2.3")).into_bytes(),
        },
        Route {
            path_suffix: "/asset".into(),
            status: 200,
            delay: Duration::ZERO,
            body: singbox_archive("1.2.3"),
        },
    ]);
    let with_base = CoreInventory::for_root_with_api_base(root.path(), server.base()).unwrap();
    let stages = Arc::new(Mutex::new(Vec::new()));
    let stage_log = stages.clone();
    let installed = with_base
        .download(CoreType::SingBox, "1.2.3", &move |stage| {
            stage_log.lock().unwrap().push(format!("{stage:?}"));
        })
        .await
        .unwrap();

    let expected = binary_at(root.path(), CoreType::SingBox, "1.2.3");
    assert_eq!(installed.path(), expected);
    assert!(installed.is_active());
    assert!(expected.is_file());
    // 共享语义：验证 + 自动选中 + 按通道保留当前与前一版本。
    assert!(!old.exists(), "同通道最旧版本被剪枝: {old:?}");
    assert!(previous.exists(), "前一版本保留");
    let config = lanpath_client::ClientConfig::load(root.path()).unwrap();
    assert_eq!(config.core_binary, expected);
    assert_eq!(config.request_proxy_mode, RequestProxyMode::None);
    assert!(lanpath_client::settings::app_settings_path(root.path()).is_file());
    assert!(!root.path().join("client.json").exists());
    // 真实阶段序列（共享层回报），无伪造 byte 进度。
    let stages = stages.lock().unwrap();
    assert!(stages.contains(&"Downloading".to_string()), "{stages:?}");
    assert!(stages.contains(&"Verifying".to_string()), "{stages:?}");
    assert!(stages.contains(&"Selecting".to_string()), "{stages:?}");
    // 真实请求序列：release tags metadata + 资产各一次（共享 resolve_asset_url）。
    let recorded = server.recorded();
    assert_eq!(
        recorded
            .iter()
            .filter(|path| path.ends_with("/repos/SagerNet/sing-box/releases/tags/v1.2.3"))
            .count(),
        1,
        "{recorded:?}"
    );
    assert_eq!(
        recorded
            .iter()
            .filter(|path| path.ends_with("/asset"))
            .count(),
        1,
        "{recorded:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn download_failure_preserves_previous_selection_and_inventory() {
    let root = tempfile::tempdir().unwrap();
    let inventory = CoreInventory::for_root(root.path()).unwrap();
    let previous = seed_script_core(root.path(), CoreType::SingBox, "1.2.2", "1.2.2");
    inventory.select(CoreType::SingBox, &previous).unwrap();
    let bytes = std::fs::read(lanpath_client::settings::app_settings_path(root.path())).unwrap();

    let server = FixtureServer::new(vec![Route {
        path_suffix: "/repos/SagerNet/sing-box/releases/tags/v1.2.3".into(),
        status: 502,
        delay: Duration::ZERO,
        body: Vec::new(),
    }]);
    let with_base = CoreInventory::for_root_with_api_base(root.path(), server.base()).unwrap();
    assert!(
        with_base
            .download(CoreType::SingBox, "1.2.3", &|_| {})
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read(lanpath_client::settings::app_settings_path(root.path())).unwrap(),
        bytes,
        "失败保留旧选择与其他字段"
    );
    assert!(previous.exists());
    assert!(!binary_at(root.path(), CoreType::SingBox, "1.2.3").exists());
}

/// Mihomo 文件名/版本规范走共享函数：Unix `.gz` 资产按共享命名匹配、
/// gzip 解压、`-v` probe 验证并自动选中。
#[cfg(unix)]
#[tokio::test]
async fn mihomo_download_uses_shared_asset_naming_and_verification() {
    let root = tempfile::tempdir().unwrap();
    let server = FixtureServer::new(vec![
        Route {
            path_suffix: "/repos/MetaCubeX/mihomo/releases/tags/v1.19.29".into(),
            status: 200,
            delay: Duration::ZERO,
            body: release_json("mihomo-linux-amd64-v2-v1.19.29.gz").into_bytes(),
        },
        Route {
            path_suffix: "/asset".into(),
            status: 200,
            delay: Duration::ZERO,
            body: mihomo_gz("1.19.29"),
        },
    ]);
    let inventory = CoreInventory::for_root_with_api_base(root.path(), server.base()).unwrap();
    let installed = inventory
        .download(CoreType::Mihomo, "1.19.29", &|_| {})
        .await
        .unwrap();
    let expected = binary_at(root.path(), CoreType::Mihomo, "1.19.29");
    assert_eq!(installed.path(), expected);
    assert_eq!(installed.core_type(), CoreType::Mihomo);
    assert!(expected.is_file());
    let active = inventory.active_core().unwrap().unwrap();
    assert_eq!(active.core_type(), CoreType::Mihomo);
    assert_eq!(active.version(), "1.19.29");
    let config = lanpath_client::ClientConfig::load(root.path()).unwrap();
    assert_eq!(config.core_type, CoreType::Mihomo);
}

/// 跨页面/跨句柄 mutation 互斥：同根第二个句柄在下载在途时，下载/选择/
/// 删除立即报“另一项核心库存操作正在进行”，绝不排队；只读查询不被阻塞；
/// 在途完成后锁释放，后续 mutation 正常。
#[cfg(unix)]
#[tokio::test]
async fn concurrent_mutation_on_the_same_root_is_rejected_then_released() {
    let root = tempfile::tempdir().unwrap();
    let first = CoreInventory::for_root(root.path()).unwrap();
    let selected = seed_script_core(root.path(), CoreType::SingBox, "1.2.2", "1.2.2");
    first.select(CoreType::SingBox, &selected).unwrap();

    // A：慢资产下载在途（metadata 立即、资产延迟 600ms）。
    let server = FixtureServer::new(vec![
        Route {
            path_suffix: "/repos/SagerNet/sing-box/releases/tags/v1.2.3".into(),
            status: 200,
            delay: Duration::ZERO,
            body: release_json(&singbox_asset_name("1.2.3")).into_bytes(),
        },
        Route {
            path_suffix: "/asset".into(),
            status: 200,
            delay: Duration::from_millis(600),
            body: singbox_archive("1.2.3"),
        },
    ]);
    let downloading = CoreInventory::for_root_with_api_base(root.path(), server.base()).unwrap();
    let worker = downloading.clone();
    let download_task = tokio::spawn(async move {
        worker
            .download(CoreType::SingBox, "1.2.3", &|_| {})
            .await
            .map(|core| core.path().to_path_buf())
    });

    // 等 A 进入资产下载（观察到 tags metadata 请求即已持锁），保证互斥窗口。
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !server
        .recorded()
        .iter()
        .any(|path| path.ends_with("/releases/tags/v1.2.3"))
        && std::time::Instant::now() < deadline
    {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        server
            .recorded()
            .iter()
            .any(|path| path.ends_with("/releases/tags/v1.2.3")),
        "下载未发起 metadata 请求"
    );

    // B（新句柄、同根）：mutation 全部立即拒绝，同一文案。
    let busy = CoreInventory::for_root_with_api_base(root.path(), server.base()).unwrap();
    for result in [
        busy.select(CoreType::SingBox, &selected)
            .unwrap_err()
            .to_string(),
        busy.download(CoreType::SingBox, "1.2.3", &|_| {})
            .await
            .unwrap_err()
            .to_string(),
        busy.delete(CoreType::SingBox, &selected)
            .unwrap_err()
            .to_string(),
    ] {
        assert!(
            result.contains(super::MUTATION_BUSY),
            "mutation 应被互斥拒绝: {result}"
        );
    }
    // 只读查询不被 mutation 锁阻塞。
    assert_eq!(busy.list_local(CoreType::SingBox).unwrap().len(), 1);

    // A 正常完成 → 锁释放 → B 的 mutation 恢复可用。
    let installed_path = download_task.await.unwrap().unwrap();
    assert_eq!(
        installed_path,
        binary_at(root.path(), CoreType::SingBox, "1.2.3")
    );
    let target = seed_script_core(root.path(), CoreType::SingBox, "1.2.4", "1.2.4");
    busy.select(CoreType::SingBox, &target).unwrap();
    assert_eq!(
        lanpath_client::ClientConfig::load(root.path())
            .unwrap()
            .core_binary,
        target
    );
}
