//! Headless 测试共用 harness（`nsh_testing::Headless`，NSH 0.5.0）：
//! 内存 storage 后端（预置条目、可回放写入）、桌面视口启动、
//! 每场景独立的临时数据根，以及本线程内的最小 HTTP 测试服务器
//! （订阅刷新用例真实联网，不依赖互联网）。
//! 本模块被每个集成测试二进制编译，各场景按需取用，允许未用项。

#![allow(dead_code)]

use nsh_core::element::Root;
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

/// 本地 HTTP 响应描述。
pub struct TestRoute {
    pub path: &'static str,
    pub status: u16,
    pub body: String,
    pub delay: Duration,
}

impl TestRoute {
    pub fn ok(path: &'static str, body: &str) -> Self {
        Self {
            path,
            status: 200,
            body: body.to_string(),
            delay: Duration::ZERO,
        }
    }

    pub fn delayed(path: &'static str, body: &str, delay: Duration) -> Self {
        Self {
            path,
            status: 200,
            body: body.to_string(),
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
        std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let requests = thread_requests.clone();
                        let routes = routes.clone();
                        std::thread::spawn(move || handle_connection(stream, requests, routes));
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
    path: &'static str,
    status: u16,
    body: String,
    delay: Duration,
}

fn handle_connection(
    mut stream: TcpStream,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    routes: Vec<Route>,
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
    let route = routes.iter().find(|route| route.path == path);
    let (status, body, delay) = match route {
        Some(route) => (route.status, route.body.clone(), route.delay),
        None => (404, String::new(), Duration::ZERO),
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
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{body}",
        length = body.len(),
    );
    let _ = stream.write_all(response.as_bytes());
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
/// 开关/删除按钮，默认 390pt 手机视口过窄）启动应用。
pub fn start_app(entries: BTreeMap<String, String>) -> (Headless, Arc<MemoryStorage>) {
    let storage = MemoryStorage::new(entries);
    let config = EngineConfig {
        viewport: Size::new(Points(1100.0), Points(800.0)),
        storage: Some(storage.clone()),
        ..EngineConfig::new(Root::of::<lanpath_desktop_nsh::App>())
    };
    let app = Headless::start(config).expect("start the headless app");
    (app, storage)
}

/// 每个场景独立的临时数据根：经 storage 键注入，走与生产一致的数据根准入。
pub fn data_root() -> (tempfile::TempDir, BTreeMap<String, String>) {
    let dir = tempfile::tempdir().expect("create a temporary data root");
    let mut entries = BTreeMap::new();
    entries.insert(
        "lanpath_desktop_nsh.data_dir".to_owned(),
        dir.path().to_string_lossy().into_owned(),
    );
    (dir, entries)
}

/// 屏幕上全部文本。
pub fn texts(app: &Headless) -> Vec<&str> {
    app.screen().texts()
}
