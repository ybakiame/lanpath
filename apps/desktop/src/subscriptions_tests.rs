//! 订阅业务测试：编辑、刷新（本地真实 HTTP server）、active 生效订阅。
//! 网络用例只连本线程内 `std::net` 起的 127.0.0.1 服务器，不依赖互联网。

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lanpath_client::{CachedSubscriptionContent, SubscriptionSource, parse_subscription_body};

use super::*;

/// 本地 HTTP 响应描述。
#[derive(Clone)]
struct Route {
    path: &'static str,
    status: u16,
    body: String,
    userinfo: Option<String>,
    delay: Duration,
}

/// 记录到的请求：路径 + User-Agent。
#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordedRequest {
    path: String,
    user_agent: Option<String>,
}

/// `std::net` 实现的最小 HTTP/1.1 服务器：按路径匹配路由，记录请求，
/// `Connection: close` 逐连接处理。
struct TestServer {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    stop: Arc<AtomicBool>,
}

impl TestServer {
    fn new(routes: Vec<Route>) -> Self {
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

    fn url(&self, path: &str) -> String {
        format!("http://{addr}{path}", addr = self.addr)
    }

    fn recorded(&self) -> Vec<RecordedRequest> {
        self.requests.lock().expect("test server requests").clone()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
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
    let mut lines = request.lines();
    let path = lines
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
    let (status, body, userinfo, delay) = match route {
        Some(route) => (
            route.status,
            route.body.clone(),
            route.userinfo.clone(),
            route.delay,
        ),
        None => (404, String::new(), None, Duration::ZERO),
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
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {length}\r\nConnection: close\r\n",
        length = body.len(),
    );
    if let Some(userinfo) = userinfo {
        response.push_str(&format!("subscription-userinfo: {userinfo}\r\n"));
    }
    response.push_str("\r\n");
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.write_all(body.as_bytes());
}

const YAML: &str = "proxies:\n  - name: 节点甲\n    type: ss\n    server: 192.0.2.1\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n  - name: 节点乙\n    type: ss\n    server: 192.0.2.2\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n";

fn yaml_route(path: &'static str) -> Route {
    Route {
        path,
        status: 200,
        body: YAML.to_string(),
        userinfo: None,
        delay: Duration::ZERO,
    }
}

/// 用共享解析结果写一份“上次成功”的原文与节点缓存。
fn seed_source_and_cache(service: &Subscriptions, id: Uuid) {
    let store = service.store();
    let fetched = parse_subscription_body(YAML, None).expect("parse the fixture yaml");
    let source = SubscriptionSource {
        format: fetched.format,
        content: fetched.source.clone(),
    };
    store.write_source(id, &source).expect("write source");
    let cache = CachedSubscriptionContent {
        format: fetched.format,
        singbox_nodes: fetched.singbox_nodes,
    };
    store
        .write_cached_content(id, &cache)
        .expect("write cached content");
    let mut subscriptions = store.load().expect("load subscriptions");
    let target = subscriptions
        .iter_mut()
        .find(|subscription| subscription.id == id)
        .expect("find the seeded subscription");
    target.node_count = 2;
    target.format = Some(source.format);
    store.save(&subscriptions).expect("save seeded result");
}

#[test]
fn empty_root_loads_an_empty_subscription_list() {
    let temporary = tempfile::tempdir().unwrap();
    let service = Subscriptions::for_root(temporary.path()).unwrap();
    assert!(service.load().unwrap().is_empty());
    assert!(!temporary.path().join("subscriptions.json").exists());
}

#[test]
fn add_persists_and_reopen_on_the_same_root_restores_it() {
    let temporary = tempfile::tempdir().unwrap();
    let service = Subscriptions::for_root(temporary.path()).unwrap();
    let added = service
        .add("东京专线", " https://example.com/sub.yaml ", None)
        .unwrap();
    assert!(added.enabled);
    assert_eq!(added.node_count, 0);
    assert!(added.format.is_none());

    // 同一数据根上的“重开”：释放业务句柄后重新打开。
    let path = temporary.path().to_path_buf();
    let reopened = Subscriptions::for_root(&path).unwrap();
    let loaded = reopened.load().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, added.id);
    assert_eq!(loaded[0].name, "东京专线");
    assert_eq!(loaded[0].url, "https://example.com/sub.yaml");
}

#[test]
fn add_rejects_invalid_input_and_existing_data_is_untouched() {
    let temporary = tempfile::tempdir().unwrap();
    let service = Subscriptions::for_root(temporary.path()).unwrap();
    let kept = service
        .add("保留", "https://example.com/a.yaml", None)
        .unwrap();

    for (name, url) in [
        ("", "https://example.com/b.yaml"),
        ("   ", "https://example.com/b.yaml"),
        ("新订阅", ""),
        ("新订阅", "ftp://example.com/c.yaml"),
        ("新订阅", "not-a-url"),
    ] {
        assert!(
            service.add(name, url, None).is_err(),
            "accepted {name:?} {url:?}"
        );
    }

    let loaded = service.load().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, kept.id);
    let text = std::fs::read_to_string(temporary.path().join("subscriptions.json")).unwrap();
    assert!(text.contains("保留"));
    assert!(!text.contains("新订阅"));
}

#[test]
fn enabled_toggle_and_remove_persist() {
    let temporary = tempfile::tempdir().unwrap();
    let service = Subscriptions::for_root(temporary.path()).unwrap();
    let first = service
        .add("一号", "https://example.com/1.yaml", None)
        .unwrap();
    let second = service
        .add("二号", "https://example.com/2.yaml", None)
        .unwrap();

    service.set_enabled(first.id, false).unwrap();
    let path = temporary.path().to_path_buf();
    let reopened = Subscriptions::for_root(&path).unwrap();
    let loaded = reopened.load().unwrap();
    let first = loaded.iter().find(|s| s.id == first.id).unwrap();
    let second = loaded.iter().find(|s| s.id == second.id).unwrap();
    assert!(!first.enabled);
    assert!(second.enabled);

    reopened.remove(first.id).unwrap();
    let loaded = reopened.load().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, second.id);
}

#[test]
fn save_error_does_not_overwrite_existing_list() {
    let temporary = tempfile::tempdir().unwrap();
    let service = Subscriptions::for_root(temporary.path()).unwrap();
    service
        .add("保留", "https://example.com/a.yaml", None)
        .unwrap();

    // 把订阅文件设为只读使保存失败：读取仍正常，新增失败，
    // 已有数据必须原样保留。
    let file = temporary.path().join("subscriptions.json");
    let mut permissions = std::fs::metadata(&file).unwrap().permissions();
    let readonly = permissions.readonly();
    permissions.set_readonly(true);
    std::fs::set_permissions(&file, permissions).unwrap();
    let result = service.add("新订阅", "https://example.com/b.yaml", None);
    let mut permissions = std::fs::metadata(&file).unwrap().permissions();
    permissions.set_readonly(readonly);
    std::fs::set_permissions(&file, permissions).unwrap();

    assert!(result.is_err());
    let loaded = service.load().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "保留");
}

#[test]
fn update_edits_fields_and_keeps_cache_until_url_changes() {
    let temporary = tempfile::tempdir().unwrap();
    let service = Subscriptions::for_root(temporary.path()).unwrap();
    let added = service
        .add("原样", "https://example.com/a.yaml", None)
        .unwrap();
    seed_source_and_cache(&service, added.id);

    // URL 不变：仅改名/UA 时上次结果与原文保留。
    let updated = service
        .update(added.id, "改名", " https://example.com/a.yaml ", Some("  "))
        .unwrap();
    assert_eq!(updated.name, "改名");
    assert_eq!(updated.url, "https://example.com/a.yaml");
    assert_eq!(updated.user_agent, None);
    assert_eq!(updated.node_count, 2);
    assert!(updated.format.is_some());
    assert!(service.source(updated.id).unwrap().is_some());

    // URL 变更：共享 store.update 清旧原文/缓存/上次结果。
    let changed = service
        .update(
            added.id,
            "换地址",
            "https://example.com/b.yaml",
            Some("custom-ua/1.0"),
        )
        .unwrap();
    assert_eq!(changed.url, "https://example.com/b.yaml");
    assert_eq!(changed.user_agent.as_deref(), Some("custom-ua/1.0"));
    assert_eq!(changed.node_count, 0);
    assert!(changed.format.is_none());
    assert!(changed.userinfo.is_none());
    assert!(service.source(changed.id).unwrap().is_none());
    assert!(service.store().load_cached_content(changed.id).is_none());
}

#[test]
fn update_rejects_invalid_input_and_missing_subscription() {
    let temporary = tempfile::tempdir().unwrap();
    let service = Subscriptions::for_root(temporary.path()).unwrap();
    let kept = service
        .add("保留", "https://example.com/a.yaml", None)
        .unwrap();

    for (name, url) in [
        ("", "https://example.com/b.yaml"),
        ("  ", "https://example.com/b.yaml"),
        ("新名", "ftp://example.com/c.yaml"),
        ("新名", "not-a-url"),
    ] {
        assert!(service.update(kept.id, name, url, None).is_err());
    }
    assert!(
        service
            .update(Uuid::new_v4(), "新名", "https://example.com/b.yaml", None)
            .is_err()
    );

    let loaded = service.load().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].name, "保留");
}

#[tokio::test]
async fn refresh_sends_saved_user_agent_and_persists_everything() {
    let server = TestServer::new(vec![Route {
        path: "/sub.yaml",
        status: 200,
        body: YAML.to_string(),
        userinfo: Some("upload=10; download=20; total=100; expire=1893456000".to_string()),
        delay: Duration::ZERO,
    }]);
    let temporary = tempfile::tempdir().unwrap();
    let service = Subscriptions::for_root(temporary.path()).unwrap();
    let added = service
        .add("联网", &server.url("/sub.yaml"), Some(" lanpath-test/9.9 "))
        .unwrap();

    let status = service.refresh(added.id).await.unwrap();
    let RefreshStatus::Updated { node_count, format } = status else {
        panic!("expected an updated refresh");
    };
    assert_eq!(node_count, 2);
    assert_eq!(format, lanpath_client::SubFormat::ClashYaml);
    let recorded = server.recorded();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].path, "/sub.yaml");
    assert_eq!(recorded[0].user_agent.as_deref(), Some("lanpath-test/9.9"));

    // 记录持久化：节点数/格式/userinfo/错误清空。
    let reloaded = service.load().unwrap();
    let target = reloaded.iter().find(|s| s.id == added.id).unwrap();
    assert_eq!(target.node_count, 2);
    assert_eq!(target.format, Some(lanpath_client::SubFormat::ClashYaml));
    assert!(target.error.is_none());
    let userinfo = target.userinfo.clone().unwrap();
    assert_eq!(userinfo.upload, Some(10));
    assert_eq!(userinfo.download, Some(20));
    assert_eq!(userinfo.total, Some(100));
    assert_eq!(userinfo.expire, Some(1893456000));

    // 原文与缓存已保存；释放句柄后重开同一数据根仍可读取（页面重开语义）。
    let source = service.source(added.id).unwrap().unwrap();
    assert_eq!(source.content, YAML);
    assert_eq!(source.format, lanpath_client::SubFormat::ClashYaml);
    let path = temporary.path().to_path_buf();
    let reopened = Subscriptions::for_root(&path).unwrap();
    let source = reopened.source(added.id).unwrap().unwrap();
    assert_eq!(source.content, YAML);
    let cache = reopened.store().load_cached_content(added.id).unwrap();
    assert_eq!(cache.format, lanpath_client::SubFormat::ClashYaml);
    assert_eq!(cache.singbox_nodes.len(), 2);
}

#[tokio::test]
async fn refresh_without_user_agent_uses_shared_default() {
    let server = TestServer::new(vec![yaml_route("/default.yaml")]);
    let temporary = tempfile::tempdir().unwrap();
    let service = Subscriptions::for_root(temporary.path()).unwrap();
    let added = service
        .add("默认UA", &server.url("/default.yaml"), None)
        .unwrap();

    let status = service.refresh(added.id).await.unwrap();
    assert!(matches!(
        status,
        RefreshStatus::Updated { node_count: 2, .. }
    ));
    let recorded = server.recorded();
    assert_eq!(recorded[0].user_agent.as_deref(), Some("clash.meta"));
}

#[tokio::test]
async fn refresh_failure_records_real_error_and_keeps_old_cache() {
    let temporary = tempfile::tempdir().unwrap();
    let service = Subscriptions::for_root(temporary.path()).unwrap();
    // 先成功一次：原文/缓存就位。
    let server = TestServer::new(vec![yaml_route("/ok.yaml")]);
    let added = service
        .add("先成后败", &server.url("/ok.yaml"), None)
        .unwrap();
    let status = service.refresh(added.id).await.unwrap();
    assert!(matches!(status, RefreshStatus::Updated { .. }));
    let old_source = service.source(added.id).unwrap().unwrap();
    drop(server);

    // 同一 URL 随后失败（连接被拒）：真实错误写回记录，旧原文/缓存/结果保留。
    let status = service.refresh(added.id).await.unwrap();
    let RefreshStatus::Failed { error } = status else {
        panic!("expected a failed refresh");
    };
    assert!(error.contains("拉取失败"), "{error}");
    let reloaded = service.load().unwrap();
    let target = reloaded.iter().find(|s| s.id == added.id).unwrap();
    assert_eq!(target.error.as_deref(), Some(error.as_str()));
    assert_eq!(target.node_count, 2);
    assert_eq!(target.format, Some(lanpath_client::SubFormat::ClashYaml));
    let kept = service.source(added.id).unwrap().unwrap();
    assert_eq!(kept, old_source);
    assert_eq!(
        service
            .store()
            .load_cached_content(added.id)
            .unwrap()
            .singbox_nodes
            .len(),
        2
    );

    // HTTP 500 的真实状态同样写回。
    let failing = TestServer::new(vec![Route {
        path: "/ok.yaml",
        status: 500,
        body: String::new(),
        userinfo: None,
        delay: Duration::ZERO,
    }]);
    let url = failing.url("/ok.yaml");
    service.update(added.id, "先成后败", &url, None).unwrap();
    let status = service.refresh(added.id).await.unwrap();
    let RefreshStatus::Failed { error } = status else {
        panic!("expected a failed refresh");
    };
    assert!(error.contains("HTTP 500"), "{error}");
    // URL 变更后旧缓存已清，失败不再留下原文。
    assert!(service.source(added.id).unwrap().is_none());
}

#[tokio::test]
async fn refresh_uses_saved_values_not_unsaved_edits() {
    let server = TestServer::new(vec![yaml_route("/saved.yaml"), yaml_route("/draft.yaml")]);
    let temporary = tempfile::tempdir().unwrap();
    let service = Subscriptions::for_root(temporary.path()).unwrap();
    let added = service
        .add("草稿", &server.url("/saved.yaml"), Some("saved-agent"))
        .unwrap();

    // “未保存表单”只是内存里的另一份草稿 URL/UA；业务刷新只认已保存记录，
    // 请求打向已保存地址并带已保存 UA。
    let status = service.refresh(added.id).await.unwrap();
    assert!(matches!(status, RefreshStatus::Updated { .. }));
    let recorded = server.recorded();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].path, "/saved.yaml");
    assert_eq!(recorded[0].user_agent.as_deref(), Some("saved-agent"));
}

#[tokio::test]
async fn refresh_result_is_stale_after_mid_flight_edit() {
    let server = TestServer::new(vec![Route {
        path: "/slow.yaml",
        status: 200,
        body: YAML.to_string(),
        userinfo: None,
        delay: Duration::from_millis(300),
    }]);
    let temporary = tempfile::tempdir().unwrap();
    let service = Subscriptions::for_root(temporary.path()).unwrap();
    let added = service
        .add("慢刷新", &server.url("/slow.yaml"), None)
        .unwrap();

    let editing = service.clone();
    let inflight = service.clone();
    let id = added.id;
    let handle = tokio::spawn(async move { inflight.refresh(id).await });
    // 请求在途期间编辑 URL：晚到的成功结果不得覆盖。
    tokio::time::sleep(Duration::from_millis(80)).await;
    editing
        .update(id, "慢刷新", &server.url("/elsewhere.yaml"), None)
        .unwrap();
    let status = handle.await.unwrap().unwrap();
    assert!(matches!(status, RefreshStatus::Stale), "{status:?}");
    let reloaded = service.load().unwrap();
    let target = reloaded.iter().find(|s| s.id == id).unwrap();
    assert!(target.url.ends_with("/elsewhere.yaml"));
    assert_eq!(target.node_count, 0);
}

#[test]
fn set_active_persists_and_validates_enabled_existing_subscription() {
    let temporary = tempfile::tempdir().unwrap();
    let service = Subscriptions::for_root(temporary.path()).unwrap();
    let enabled = service
        .add("可用", "https://example.com/a.yaml", None)
        .unwrap();
    let disabled = service
        .add("停用项", "https://example.com/b.yaml", None)
        .unwrap();
    service.set_enabled(disabled.id, false).unwrap();

    assert!(service.set_active(disabled.id).is_err());
    assert!(service.set_active(Uuid::new_v4()).is_err());
    service.set_active(enabled.id).unwrap();
    assert_eq!(service.active_id().unwrap(), Some(enabled.id));

    // 同一数据根重开后选择仍在（client.json 持久化）。
    let path = temporary.path().to_path_buf();
    let reopened = Subscriptions::for_root(&path).unwrap();
    assert_eq!(reopened.active_id().unwrap(), Some(enabled.id));
}

#[test]
fn disabling_or_removing_the_active_subscription_clears_it() {
    let temporary = tempfile::tempdir().unwrap();
    let service = Subscriptions::for_root(temporary.path()).unwrap();
    let first = service
        .add("一号", "https://example.com/1.yaml", None)
        .unwrap();
    let second = service
        .add("二号", "https://example.com/2.yaml", None)
        .unwrap();

    service.set_active(first.id).unwrap();
    service.set_enabled(first.id, false).unwrap();
    assert_eq!(service.active_id().unwrap(), None);
    // 停用项不能设为生效。
    assert!(service.set_active(first.id).is_err());

    service.set_active(second.id).unwrap();
    service.remove(second.id).unwrap();
    assert_eq!(service.active_id().unwrap(), None);
}

#[test]
fn request_config_pins_proxy_mode_none_and_this_root() {
    let temporary = tempfile::tempdir().unwrap();
    let service = Subscriptions::for_root(temporary.path()).unwrap();
    let added = service
        .add("配置", "https://example.com/c.yaml", None)
        .unwrap();
    service.set_active(added.id).unwrap();

    // set_active 已把配置写回本应用 client.json；重新读取时 data_dir 由
    // load_for_requests 固定为传入根（不信任文件里的旧路径），请求代理为
    // 本应用固定的直连策略。
    let config = lanpath_client::ClientConfig::load_for_requests(temporary.path()).unwrap();
    assert_eq!(config.active_subscription_id, Some(added.id));
    assert_eq!(config.data_dir.as_path(), temporary.path());
    assert_eq!(
        config.request_proxy_mode,
        lanpath_client::RequestProxyMode::None
    );
}

#[test]
fn reopening_the_same_root_reuses_the_same_handle() {
    let temporary = tempfile::tempdir().unwrap();
    let first = Subscriptions::for_root(temporary.path()).unwrap();
    let second = Subscriptions::for_root(temporary.path()).unwrap();
    // 同根句柄是同一个 Arc：切页/重开后在途刷新与新操作共享同一把写锁。
    assert!(Arc::ptr_eq(&first, &second));
    let third = first.clone();
    assert!(Arc::ptr_eq(&first, &third));
}

#[test]
fn existing_request_config_is_preserved_by_active_and_mutation_ops() {
    let temporary = tempfile::tempdir().unwrap();
    let service = Subscriptions::for_root(temporary.path()).unwrap();

    // 预置一份带自定义请求策略的“已有配置”。
    let mut config = lanpath_client::ClientConfig::load_for_requests(temporary.path()).unwrap();
    config.request_proxy_mode = lanpath_client::RequestProxyMode::Custom;
    config.request_proxy_url = "http://127.0.0.1:4790".to_string();
    config.github_proxy_prefix = "https://mirror.example/".to_string();
    config.save().unwrap();

    let first = service
        .add("一号", "https://example.com/1.yaml", None)
        .unwrap();
    let second = service
        .add("二号", "https://example.com/2.yaml", None)
        .unwrap();

    service.set_active(first.id).unwrap();
    service.set_enabled(first.id, false).unwrap();
    service.set_active(second.id).unwrap();
    service.remove(second.id).unwrap();
    assert_eq!(service.active_id().unwrap(), None);

    // set_active / set_enabled / remove / active_id 都不改写已保存的请求字段。
    let kept = lanpath_client::ClientConfig::load_for_requests(temporary.path()).unwrap();
    assert_eq!(kept.active_subscription_id, None);
    assert_eq!(
        kept.request_proxy_mode,
        lanpath_client::RequestProxyMode::Custom
    );
    assert_eq!(kept.request_proxy_url, "http://127.0.0.1:4790");
    assert_eq!(kept.github_proxy_prefix, "https://mirror.example/");
}

#[tokio::test]
async fn refresh_honors_the_saved_custom_proxy_policy() {
    // 代理服务器只记录请求；订阅目标不可达，证明请求是按已保存策略
    // 发给代理而不是直连。
    let proxy = TestServer::new(vec![]);
    let temporary = tempfile::tempdir().unwrap();
    let service = Subscriptions::for_root(temporary.path()).unwrap();
    let mut config = lanpath_client::ClientConfig::load_for_requests(temporary.path()).unwrap();
    config.request_proxy_mode = lanpath_client::RequestProxyMode::Custom;
    config.request_proxy_url = proxy.url("");
    config.save().unwrap();

    let added = service
        .add("走代理", "http://192.0.2.99/unreachable.yaml", None)
        .unwrap();
    let status = service.refresh(added.id).await.unwrap();
    assert!(matches!(status, RefreshStatus::Failed { .. }), "{status:?}");
    let recorded = proxy.recorded();
    assert_eq!(recorded.len(), 1);
    assert!(
        recorded[0].path.starts_with("http://192.0.2.99/"),
        "经代理的请求必须是绝对形式，实际：{}",
        recorded[0].path
    );
}
