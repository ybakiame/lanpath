//! Headless 测试共用 harness（`nsh_testing::Headless`，NSH 0.5.0）：
//! 内存 storage 后端（预置条目、可回放写入）、桌面视口启动、
//! 每场景独立的临时数据根，以及本线程内的最小 HTTP 测试服务器
//! （订阅刷新用例真实联网，不依赖互联网）。
//! 本模块被每个集成测试二进制编译，各场景按需取用，允许未用项。

#![allow(dead_code)]

use nsh_core::storage::{StorageBackend, StorageChange};
use nsh_core::{Points, Size};
use nsh_engine::EngineConfig;
use nsh_testing::Headless;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 本地 HTTP 响应描述。`body` 支持 `{addr}` 占位符（服务时替换为服务器
/// 实际地址，release JSON 可回指本服务器资产）；二进制归档直接给字节。
pub struct TestRoute {
    pub path: String,
    pub status: u16,
    pub body: Vec<u8>,
    pub delay: Duration,
}

impl TestRoute {
    pub fn ok(path: &str, body: &str) -> Self {
        Self {
            path: path.to_owned(),
            status: 200,
            body: body.to_string().into_bytes(),
            delay: Duration::ZERO,
        }
    }

    pub fn ok_bytes(path: &str, body: Vec<u8>) -> Self {
        Self {
            path: path.to_owned(),
            status: 200,
            body,
            delay: Duration::ZERO,
        }
    }

    pub fn with_status(path: &str, status: u16, body: &str) -> Self {
        Self {
            path: path.to_owned(),
            status,
            body: body.to_string().into_bytes(),
            delay: Duration::ZERO,
        }
    }

    pub fn delayed(path: &str, body: &str, delay: Duration) -> Self {
        Self {
            path: path.to_owned(),
            status: 200,
            body: body.to_string().into_bytes(),
            delay,
        }
    }

    pub fn delayed_bytes(path: &str, body: Vec<u8>, delay: Duration) -> Self {
        Self {
            path: path.to_owned(),
            status: 200,
            body,
            delay,
        }
    }
}

/// 记录到的请求：路径 + User-Agent。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedRequest {
    pub path: String,
    pub user_agent: Option<String>,
}

/// `std::net` 实现的最小 HTTP/1.1 服务器：按路径匹配路由、记录请求、
/// `Connection: close` 逐连接处理；只监听 127.0.0.1。
pub struct TestServer {
    addr: std::net::SocketAddr,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    stop: Arc<AtomicBool>,
}

impl TestServer {
    pub fn new(routes: Vec<TestRoute>) -> Self {
        let routes: Vec<Route> = routes
            .into_iter()
            .map(|route| Route {
                path: route.path,
                status: route.status,
                body: route.body,
                delay: route.delay,
            })
            .collect();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a local test server");
        listener
            .set_nonblocking(true)
            .expect("set the test server non-blocking");
        let addr = listener.local_addr().expect("read the test server address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_requests = requests.clone();
        let thread_stop = stop.clone();
        let thread_addr = addr.to_string();
        std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let requests = thread_requests.clone();
                        let routes = routes.clone();
                        let addr = thread_addr.clone();
                        std::thread::spawn(move || {
                            handle_connection(stream, requests, routes, addr)
                        });
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        Self {
            addr,
            requests,
            stop,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{addr}{path}", addr = self.addr)
    }

    pub fn recorded(&self) -> Vec<RecordedRequest> {
        self.requests.lock().expect("test server requests").clone()
    }

    /// 记录里指向 `path` 的请求数。
    pub fn count_path(&self, path: &str) -> usize {
        self.recorded()
            .iter()
            .filter(|request| request.path == path)
            .count()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Clone)]
struct Route {
    path: String,
    status: u16,
    body: Vec<u8>,
    delay: Duration,
}

fn handle_connection(
    mut stream: TcpStream,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    routes: Vec<Route>,
    addr: String,
) {
    let mut head = Vec::new();
    let mut buffer = [0_u8; 4096];
    while !head.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(_) => return,
        };
        head.extend_from_slice(&buffer[..read]);
        if head.len() > 16384 {
            return;
        }
    }
    let request = String::from_utf8_lossy(&head);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_string();
    let user_agent = request
        .lines()
        .skip(1)
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("user-agent")
                .then(|| value.trim().to_string())
        })
        .filter(|value| !value.is_empty());
    // 直连请求 path 以 "/" 开头；经代理的请求是 absolute-form 完整目标
    // URL——两者都按精确路径或其末尾匹配（路由路径以 "/" 开头，无歧义）。
    let route = routes.iter().find(|route| {
        path == route.path || (!route.path.starts_with("http") && path.ends_with(&route.path))
    });
    let (status, body, delay) = match route {
        Some(route) => (route.status, route.body.clone(), route.delay),
        None => (404, Vec::new(), Duration::ZERO),
    };
    // release JSON 里的 `{addr}` 回指本服务器资产地址。
    let body = match std::str::from_utf8(&body) {
        Ok(text) if text.contains("{addr}") => text.replace("{addr}", &addr).into_bytes(),
        _ => body,
    };
    requests
        .lock()
        .expect("test server requests")
        .push(RecordedRequest { path, user_agent });
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
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

/// NSH storage 的内存后端：以预置条目启动并记录写入，测试直接断言其状态。
#[derive(Debug, Default)]
pub struct MemoryStorage {
    entries: Mutex<BTreeMap<String, String>>,
}

impl MemoryStorage {
    pub fn new(entries: BTreeMap<String, String>) -> Arc<Self> {
        Arc::new(Self {
            entries: Mutex::new(entries),
        })
    }

    /// 键的当前值；仅外观场景断言使用（其余测试目标视为未用）。
    #[allow(dead_code)]
    pub fn get(&self, key: &str) -> Option<String> {
        self.entries.lock().unwrap().get(key).cloned()
    }
}

impl StorageBackend for MemoryStorage {
    fn load(&self) -> Result<BTreeMap<String, String>, String> {
        Ok(self.entries.lock().unwrap().clone())
    }

    fn write(&self, change: StorageChange) {
        let mut entries = self.entries.lock().unwrap();
        match change {
            StorageChange::Set(key, value) => {
                entries.insert(key, value);
            }
            StorageChange::Remove(key) => {
                entries.remove(&key);
            }
            StorageChange::Clear => entries.clear(),
        }
    }
}

/// 以给定 storage 预置在桌面视口（1100×800，与原生窗口同量级；行内含
/// 开关/删除按钮，默认 390pt 手机视口过窄）启动应用：与生产同一条
/// `root()` 构建链（翻译 + 默认主题 token 注入）。
pub fn start_app(entries: BTreeMap<String, String>) -> (Headless, Arc<MemoryStorage>) {
    let storage = MemoryStorage::new(entries);
    let config = EngineConfig {
        viewport: Size::new(Points(1100.0), Points(800.0)),
        storage: Some(storage.clone()),
        ..EngineConfig::new(lanpath_desktop_nsh::root())
    };
    let app = Headless::start(config).expect("start the headless app");
    (app, storage)
}

/// 以给定 storage 预置启动组件画廊（`--ui-gallery` 的同一根构建链）。
pub fn start_gallery(entries: BTreeMap<String, String>) -> (Headless, Arc<MemoryStorage>) {
    let storage = MemoryStorage::new(entries);
    let config = EngineConfig {
        viewport: Size::new(Points(1100.0), Points(800.0)),
        storage: Some(storage.clone()),
        ..EngineConfig::new(lanpath_desktop_nsh::gallery_root())
    };
    let app = Headless::start(config).expect("start the headless gallery");
    (app, storage)
}

/// 每个场景独立的临时数据根：经 storage 键注入，走与生产一致的数据根准入。
/// 预置界面语言 zh-CN：既有业务页用例以中文文案驱动（与翻译前的
/// 断言一致）；翻译相关用例自行覆盖该预置。
pub fn data_root() -> (tempfile::TempDir, BTreeMap<String, String>) {
    let dir = tempfile::tempdir().expect("create a temporary data root");
    let mut entries = BTreeMap::new();
    entries.insert(
        "lanpath_desktop_nsh.data_dir".to_owned(),
        dir.path().to_string_lossy().into_owned(),
    );
    entries.insert("language".to_owned(), "zh-CN".to_owned());
    (dir, entries)
}

/// 屏幕上全部文本。
pub fn texts(app: &Headless) -> Vec<&str> {
    app.screen().texts()
}

/// 用 `rustc` 把 `tests/fixtures/core_fixture.rs` 编译为真实假核心二进制
/// （真实进程身份/监听/退出码；与 crates/lanpath-client state tests 的
/// lifecycle fixture 同一模式，只链接 Cargo 已构建的 serde_json rlib）。
/// 这是 fixture 核心，不等于真实 sing-box/Mihomo（Windows 真实核心验收由
/// 主代理执行）。每测试进程编译一次。
pub fn compiled_core_fixture() -> &'static std::path::Path {
    static COMPILED: std::sync::LazyLock<(tempfile::TempDir, std::path::PathBuf)> =
        std::sync::LazyLock::new(|| {
            let directory = tempfile::tempdir().expect("fixture tempdir");
            let executable = std::env::current_exe().expect("current test binary");
            let dependencies = executable.parent().expect("deps directory");
            let serde_json = std::fs::read_dir(dependencies)
                .expect("read deps directory")
                .map(|entry| entry.expect("deps entry").path())
                .find(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| {
                            name.starts_with("libserde_json-") && name.ends_with(".rlib")
                        })
                })
                .expect("Cargo-built serde_json library");
            let output = std::process::Command::new("rustc")
                .args([
                    "--edition=2024",
                    "--crate-name=lanpath_desktop_core_fixture",
                ])
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/core_fixture.rs"
                ))
                .arg("--extern")
                .arg(format!("serde_json={}", serde_json.display()))
                .arg("-L")
                .arg(format!("dependency={}", dependencies.display()))
                .arg("-o")
                .arg(directory.path().join("core-fixture"))
                .output()
                .expect("run rustc for the core fixture");
            assert!(
                output.status.success(),
                "core fixture compile failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let binary = directory.path().join("core-fixture");
            (directory, binary)
        });
    &COMPILED.1
}

/// 把假核心复制进库存布局 `cores/<binary>/<version>/<文件名>`（共享
/// on-disk 命名规则），返回二进制路径。
pub fn seed_fixture_core(
    root: &std::path::Path,
    core_type: lanpath_common::CoreType,
    version: &str,
) -> std::path::PathBuf {
    let name = if cfg!(windows) {
        format!("{}.exe", core_type.binary_name())
    } else {
        core_type.binary_name().to_string()
    };
    let path = root
        .join("cores")
        .join(core_type.binary_name())
        .join(version)
        .join(name);
    std::fs::create_dir_all(path.parent().expect("inventory version directory"))
        .expect("create inventory directory");
    std::fs::copy(compiled_core_fixture(), &path).expect("copy fixture into inventory");
    path
}
