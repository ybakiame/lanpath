//! 配置合成预览业务测试：共享合成语义（GPUI `build_config_report` 的
//! sing-box 路径）落在本数据根。网络用例只连本线程内 `std::net` 起的
//! 127.0.0.1 服务器（含经自定义代理的绝对形式请求），不依赖互联网；
//! 规则集镜像经同一本地代理快速 404，覆盖“缺失按共享降级回报”路径。

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lanpath_client::{
    CachedSubscriptionContent, ClientConfig, RequestProxyMode, SubscriptionSource,
    parse_subscription_body,
};

use super::*;
use crate::profiles::Profiles;
use crate::subscriptions::Subscriptions;

/// 本地 HTTP 响应描述（路径支持经代理的绝对形式匹配）。
#[derive(Clone)]
struct Route {
    path: String,
    status: u16,
    body: String,
    delay: Duration,
}

impl Route {
    fn ok(path: &str, body: &str) -> Self {
        Self {
            path: path.to_owned(),
            status: 200,
            body: body.to_owned(),
            delay: Duration::ZERO,
        }
    }

    fn delayed(path: &str, body: &str, delay: Duration) -> Self {
        Self {
            path: path.to_owned(),
            status: 200,
            body: body.to_owned(),
            delay,
        }
    }
}

/// `std::net` 实现的最小 HTTP/1.1 服务器：按路径后缀匹配路由（兼容代理
/// 绝对形式），记录请求，`Connection: close` 逐连接处理。
struct TestServer {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
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

    fn recorded(&self) -> Vec<String> {
        self.requests.lock().expect("test server requests").clone()
    }

    /// 记录里指向 `path` 后缀的请求数。
    fn count_path(&self, path: &str) -> usize {
        self.recorded()
            .iter()
            .filter(|request| request.ends_with(path))
            .count()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn handle_connection(mut stream: TcpStream, requests: Arc<Mutex<Vec<String>>>, routes: Vec<Route>) {
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
    let route = routes
        .iter()
        .find(|route| path == route.path || path.ends_with(&route.path));
    let (status, body, delay) = match route {
        Some(route) => (route.status, route.body.clone(), route.delay),
        None => (404, String::new(), Duration::ZERO),
    };
    requests.lock().expect("test server requests").push(path);
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

const YAML: &str = "proxies:\n  - name: 节点甲\n    type: ss\n    server: 192.0.2.1\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n  - name: 节点乙\n    type: ss\n    server: 192.0.2.2\n    port: 8388\n    cipher: aes-256-gcm\n    password: test-only\n";

/// 打开同根订阅/覆写/合成业务（共享数据根与业务锁）。
fn services(root: &std::path::Path) -> (Arc<Subscriptions>, Profiles, ConfigComposer) {
    (
        Subscriptions::for_root(root).expect("open subscriptions"),
        Profiles::for_root(root).expect("open profiles"),
        ConfigComposer::for_root(root).expect("open composer"),
    )
}

/// 新增已启用订阅并设为当前生效；返回订阅 id。
fn add_active(service: &Subscriptions, name: &str, url: &str) -> uuid::Uuid {
    let subscription = service.add(name, url, None).expect("add subscription");
    service.set_active(subscription.id).expect("set active");
    subscription.id
}

/// 用共享解析结果写一份节点缓存（“上次成功”状态，合成不再联网）。
fn seed_cache(root: &std::path::Path, id: uuid::Uuid) {
    let store = SubscriptionStore::new(root.to_path_buf());
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
}

/// 读改写本根 `app.toml`（经共享 `load_for_requests` 语义）。
fn edit_config(root: &std::path::Path, edit: impl FnOnce(&mut ClientConfig)) {
    let mut config = crate::request_settings::load_request_config(root).expect("load config");
    edit(&mut config);
    config.save().expect("save config");
}

fn parsed_json(preview: &ConfigPreview) -> serde_json::Value {
    serde_json::from_str(preview.content()).expect("preview is valid JSON")
}

#[tokio::test]
async fn compose_rejects_without_active_subscription() {
    let temporary = tempfile::tempdir().unwrap();
    let (_, _, composer) = services(temporary.path());
    let error = composer
        .compose()
        .await
        .expect_err("no active subscription");
    assert!(error.to_string().contains("请先在订阅页选择要使用的订阅"));
}

#[tokio::test]
async fn compose_rejects_a_missing_active_target() {
    let temporary = tempfile::tempdir().unwrap();
    let (subscriptions, _, composer) = services(temporary.path());
    add_active(&subscriptions, "存在订阅", "https://example.com/a.yaml");
    edit_config(temporary.path(), |config| {
        config.active_subscription_id = Some(uuid::Uuid::new_v4());
    });
    let error = composer.compose().await.expect_err("missing target");
    assert!(error.to_string().contains("所选订阅不存在"));
}

#[tokio::test]
async fn compose_rejects_a_disabled_active_subscription() {
    let temporary = tempfile::tempdir().unwrap();
    let (subscriptions, _, composer) = services(temporary.path());
    let id = add_active(&subscriptions, "停用订阅", "https://example.com/off.yaml");
    // 停用会同时清除生效选择；直接落盘构造“生效但停用”的存量状态（与 GPUI
    // 的 enabled 校验一致），再恢复 active 引用。
    subscriptions.set_enabled(id, false).unwrap();
    let store = SubscriptionStore::new(temporary.path().to_path_buf());
    let mut list = store.load().unwrap();
    list.iter_mut()
        .find(|entry| entry.id == id)
        .unwrap()
        .enabled = false;
    store.save(&list).unwrap();
    edit_config(temporary.path(), |config| {
        config.active_subscription_id = Some(id);
    });
    let error = composer.compose().await.expect_err("disabled target");
    assert!(error.to_string().contains("所选订阅已停用"));
}

/// 种子 Mihomo 库存核心并保存选择（统一布局 core_type 由 [core].kind
/// 投影：Mihomo 只能来自 Mihomo 核心选择）。
fn seed_mihomo_selection(root: &std::path::Path) {
    let mihomo = root.join("cores/mihomo/1.19.29/mihomo");
    std::fs::create_dir_all(mihomo.parent().unwrap()).unwrap();
    std::fs::write(&mihomo, b"inventory fixture").unwrap();
    edit_config(root, |config| {
        config.core_binary = mihomo.clone();
    });
}

/// Mihomo 订阅配置合成（GPUI Mihomo 分支同路径）：真实 Clash 订阅原文
/// 提取代理，输出 YAML（不是 JSON）——关键字段、节点、端口真实，面板
/// 下载字段移除，全程离线（Core 策略 + 本地缓存）。
#[tokio::test]
async fn compose_mihomo_subscription_to_yaml_with_key_fields() {
    let temporary = tempfile::tempdir().unwrap();
    let (subscriptions, _, composer) = services(temporary.path());
    let id = add_active(&subscriptions, "Mihomo 订阅", "https://example.com/m.yaml");
    seed_cache(temporary.path(), id);
    seed_mihomo_selection(temporary.path());
    edit_config(temporary.path(), |config| {
        config.request_proxy_mode = RequestProxyMode::Core;
    });

    let preview = composer.compose().await.expect("mihomo composes");
    assert_eq!(preview.format(), super::PreviewFormat::Yaml);
    assert_eq!(preview.format().label(), "YAML");
    assert_eq!(preview.core_type(), CoreType::Mihomo);
    assert_eq!(preview.node_count(), 2);
    // 输出按 YAML 解析（JSON 解析器也能读 YAML 子集，这里用 serde_yaml
    // 验证它确实是合法 YAML），并断言关键字段。
    let value: serde_json::Value =
        serde_yaml::from_str(preview.content()).expect("YAML preview parses");
    assert_eq!(value["mixed-port"], 17899);
    assert_eq!(value["allow-lan"], false);
    assert_eq!(value["bind-address"], "127.0.0.1");
    assert_eq!(value["mode"], "rule");
    assert_eq!(value["external-controller"], "127.0.0.1:19090");
    assert!(value.get("secret").is_none(), "空密钥不写入 secret");
    // 两个订阅节点经原始 Clash 原文进入 proxies，代理组真实引用它们。
    let proxies = value["proxies"].as_array().expect("proxies array");
    let names: Vec<&str> = proxies
        .iter()
        .filter_map(|proxy| proxy["name"].as_str())
        .collect();
    assert!(names.contains(&"节点甲"), "{names:?}");
    assert!(names.contains(&"节点乙"), "{names:?}");
    let groups = value["proxy-groups"].as_array().expect("proxy-groups");
    assert!(groups.iter().any(|group| {
        group["proxies"]
            .as_array()
            .is_some_and(|nodes| nodes.len() == 2)
    }));
    // 面板目录/下载地址不得出现在预览（避免任何消费方触发面板下载）。
    assert!(value.get("external-ui").is_none());
    assert!(value.get("external-ui-url").is_none());
}

/// 无 Clash 原文时按共享转换逐节点生成 Mihomo 代理（sing-box 缓存路径）。
#[tokio::test]
async fn compose_mihomo_falls_back_to_converting_cached_nodes() {
    let temporary = tempfile::tempdir().unwrap();
    let (subscriptions, _, composer) = services(temporary.path());
    let id = add_active(&subscriptions, "Mihomo 订阅", "https://example.com/m.yaml");
    // 只写节点缓存、不写原文：走 sing-box 节点 → Mihomo 代理的共享转换。
    let store = SubscriptionStore::new(temporary.path().to_path_buf());
    let fetched = parse_subscription_body(YAML, None).unwrap();
    store
        .write_cached_content(
            id,
            &CachedSubscriptionContent {
                format: fetched.format,
                singbox_nodes: fetched.singbox_nodes,
            },
        )
        .unwrap();
    seed_mihomo_selection(temporary.path());
    edit_config(temporary.path(), |config| {
        config.request_proxy_mode = RequestProxyMode::Core;
    });

    let preview = composer.compose().await.expect("mihomo composes");
    let value: serde_json::Value = serde_yaml::from_str(preview.content()).unwrap();
    let proxies = value["proxies"].as_array().expect("proxies array");
    let names: Vec<&str> = proxies
        .iter()
        .filter_map(|proxy| proxy["name"].as_str())
        .collect();
    assert!(names.contains(&"节点甲"), "{names:?}");
    assert!(names.contains(&"节点乙"), "{names:?}");
    assert!(
        proxies
            .iter()
            .all(|proxy| proxy["type"] == serde_json::json!("ss"))
    );
}

/// 非默认 sing-box 配置切片在 Mihomo 合成中只警告、不应用（切片内容
/// 保留，供切换回 sing-box）；无定制切片时不出现该警告。
#[tokio::test]
async fn compose_mihomo_warns_but_does_not_apply_custom_slices() {
    let temporary = tempfile::tempdir().unwrap();
    let (subscriptions, _, composer) = services(temporary.path());
    let id = add_active(&subscriptions, "Mihomo 订阅", "https://example.com/m.yaml");
    seed_cache(temporary.path(), id);
    seed_mihomo_selection(temporary.path());
    edit_config(temporary.path(), |config| {
        config.request_proxy_mode = RequestProxyMode::Core;
    });
    // 用户定制切片：在 overwrite/sing-box/outbounds.json 加入自定义 selector。
    let overwrite_store = lanpath_client::singbox_overwrite::SingboxOverwriteStore::new(
        temporary.path().to_path_buf(),
    );
    overwrite_store
        .save_raw(
            lanpath_client::singbox_overwrite::SingboxSliceKind::Outbounds,
            &serde_json::to_string_pretty(&serde_json::json!({
                "outbounds": [
                    {
                        "type": "selector",
                        "tag": "slice-custom-selector",
                        "outbounds": ["direct", "$nodes"],
                        "default": "direct"
                    },
                    { "type": "direct", "tag": "direct" }
                ]
            }))
            .unwrap(),
        )
        .unwrap();

    let preview = composer.compose().await.expect("mihomo composes");
    assert!(
        preview
            .warnings()
            .iter()
            .any(|warning| warning.contains("未应用 sing-box 配置切片")),
        "{:?}",
        preview.warnings()
    );
    // 切片不被应用：自定义 selector 不出现在 Mihomo 配置里。
    let value: serde_json::Value = serde_yaml::from_str(preview.content()).unwrap();
    let groups = value["proxy-groups"].as_array().expect("proxy-groups");
    assert!(
        groups
            .iter()
            .all(|group| group["name"] != serde_json::json!("slice-custom-selector")),
        "{groups:?}"
    );
}

#[tokio::test]
async fn compose_rejects_core_mode_without_cache_and_stays_offline() {
    let server = TestServer::new(vec![Route::ok("/sub.yaml", YAML)]);
    let temporary = tempfile::tempdir().unwrap();
    let (subscriptions, _, composer) = services(temporary.path());
    add_active(&subscriptions, "核心订阅", &server.url("/sub.yaml"));
    edit_config(temporary.path(), |config| {
        config.request_proxy_mode = RequestProxyMode::Core;
    });
    let error = composer.compose().await.expect_err("core without cache");
    let message = error.to_string();
    assert!(message.contains("核心请求代理尚未就绪"), "{message}");
    assert!(message.contains("没有可用节点缓存"), "{message}");
    assert!(server.recorded().is_empty(), "core 模式无缓存不得联网");
}

#[tokio::test]
async fn compose_uses_cache_without_any_network() {
    let server = TestServer::new(vec![Route::ok("/sub.yaml", YAML)]);
    let temporary = tempfile::tempdir().unwrap();
    let (subscriptions, _, composer) = services(temporary.path());
    let id = add_active(&subscriptions, "缓存订阅", &server.url("/sub.yaml"));
    seed_cache(temporary.path(), id);
    edit_config(temporary.path(), |config| {
        // 故意选择 Core：cache_only 上下文连规则集镜像都不得联网。
        config.request_proxy_mode = RequestProxyMode::Core;
        config.mixed_port = 17891;
    });

    let preview = composer.compose().await.expect("compose from cache");
    assert_eq!(preview.node_count(), 2);
    assert!(
        server.recorded().is_empty(),
        "有缓存时合成必须完全离线:{:?}",
        server.recorded()
    );

    let value = parsed_json(&preview);
    let outbounds = value["outbounds"].as_array().expect("outbounds array");
    let tags: Vec<&str> = outbounds
        .iter()
        .filter_map(|entry| entry["tag"].as_str())
        .collect();
    assert!(tags.contains(&"节点甲"), "{tags:?}");
    assert!(tags.contains(&"节点乙"), "{tags:?}");
    // 入站端口取已保存配置的 mixed_port 快照。
    assert_eq!(value["inbounds"][0]["listen_port"], 17891);
    // 不再内置任何规则集，纯离线缓存合成不会产生规则集缺失告警。
    assert!(preview.missing_tags().is_empty(), "{preview:?}");
    assert!(preview.warnings().is_empty(), "{:?}", preview.warnings());
    assert_eq!(value["route"]["rule_set"], serde_json::json!([]));
}

#[tokio::test]
async fn compose_refreshes_missing_cache_with_a_fixed_config_snapshot() {
    let server = TestServer::new(vec![Route::delayed(
        "/sub.yaml",
        YAML,
        Duration::from_millis(500),
    )]);
    let temporary = tempfile::tempdir().unwrap();
    let (subscriptions, _, composer) = services(temporary.path());
    add_active(&subscriptions, "快照订阅", &server.url("/sub.yaml"));
    edit_config(temporary.path(), |config| {
        config.request_proxy_mode = RequestProxyMode::Custom;
        config.request_proxy_url = server.url("");
    });

    let in_flight = tokio::spawn({
        let composer = composer.clone();
        async move { composer.compose().await }
    });
    // 请求在途时把已保存策略改成 Core：本次合成必须仍按快照（Custom）完成；
    // 若实现途中重读配置，refresh 会走 Core 失败、RemoteFetchContext 也会拒绝。
    tokio::time::sleep(Duration::from_millis(150)).await;
    edit_config(temporary.path(), |config| {
        config.request_proxy_mode = RequestProxyMode::Core;
    });

    let preview = in_flight
        .await
        .expect("compose task")
        .expect("snapshot refresh must succeed");
    assert_eq!(preview.node_count(), 2);
    assert!(parsed_json(&preview)["outbounds"].is_array());
    assert_eq!(server.count_path("/sub.yaml"), 1);
}

#[tokio::test]
async fn compose_applies_profile_remote_and_local_overrides_to_the_json() {
    let server = TestServer::new(vec![
        Route::ok(
            "/ovr.yaml",
            "log:\n  level: debug\nremote_yaml_marker: true\n",
        ),
        Route::ok(
            "/ovr.js",
            "function main(config){ config.__chain = (config.__chain || \"\") + \"R\"; config.remote_js_marker = true; config.log.level = \"debug\"; return config; }",
        ),
    ]);
    let temporary = tempfile::tempdir().unwrap();
    let (subscriptions, profiles, composer) = services(temporary.path());
    let id = add_active(&subscriptions, "覆写订阅", "https://example.com/o.yaml");
    seed_cache(temporary.path(), id);
    let profile = profiles
        .save(None, "合成模板", "function main(config){ config.__chain = (config.__chain || \"\") + \"L\"; config.local_js_marker = true; config.log.level = \"trace\"; return config; }"
                .to_owned(), Some(server.url("/ovr.js")))
        .unwrap();
    let updated = subscriptions
        .load()
        .unwrap()
        .into_iter()
        .find(|subscription| subscription.id == id)
        .unwrap();
    subscriptions
        .update(
            id,
            &updated.name,
            &updated.url,
            updated.user_agent.as_deref(),
            Some(profile.id),
        )
        .unwrap();
    edit_config(temporary.path(), |config| {
        // 自定义代理指向本地服务器：远程覆写与规则集镜像都经它快速完成/失败。
        config.request_proxy_mode = RequestProxyMode::Custom;
        config.request_proxy_url = server.url("");
    });

    let preview = composer.compose().await.expect("compose with overrides");
    let value = parsed_json(&preview);
    // 旧远程/本地 YAML 均忽略；TS 链仍可修改 log。
    assert!(value["remote_yaml_marker"].is_null());
    assert!(value["local_yaml_marker"].is_null());
    assert_eq!(value["log"]["level"], "trace");
    // 远程 TS 先执行、本地 TS 后执行，链式结果完整保留。
    assert_eq!(value["__chain"], "RL");
    assert_eq!(value["remote_js_marker"], true);
    assert_eq!(value["local_js_marker"], true);
    // 远程覆写确实经已保存策略从本地服务器取到。
    assert_eq!(server.count_path("/ovr.yaml"), 0);
    assert_eq!(server.count_path("/ovr.js"), 1);
}

#[tokio::test]
async fn compose_surfaces_remote_override_failure_warnings() {
    let server = TestServer::new(vec![]);
    let temporary = tempfile::tempdir().unwrap();
    let (subscriptions, profiles, composer) = services(temporary.path());
    let id = add_active(&subscriptions, "降级订阅", "https://example.com/w.yaml");
    seed_cache(temporary.path(), id);
    let profile = profiles
        .save(
            None,
            "降级模板",
            String::new(),
            Some(server.url("/missing.ts")),
        )
        .unwrap();
    let updated = subscriptions
        .load()
        .unwrap()
        .into_iter()
        .find(|subscription| subscription.id == id)
        .unwrap();
    subscriptions
        .update(
            id,
            &updated.name,
            &updated.url,
            updated.user_agent.as_deref(),
            Some(profile.id),
        )
        .unwrap();
    edit_config(temporary.path(), |config| {
        config.request_proxy_mode = RequestProxyMode::Custom;
        config.request_proxy_url = server.url("");
    });

    // 远程覆写 404 且没有缓存：共享降级跳过，合成仍成功，警告不静默。
    let preview = composer
        .compose()
        .await
        .expect("compose falls back with warnings");
    assert!(
        preview
            .warnings()
            .iter()
            .any(|warning| warning.contains("profile remote js fetch failed")),
        "{:?}",
        preview.warnings()
    );
}

#[tokio::test]
async fn compose_strips_dashboard_download_fields_after_panel_features() {
    let server = TestServer::new(vec![]);
    let temporary = tempfile::tempdir().unwrap();
    let (subscriptions, _, composer) = services(temporary.path());
    let id = add_active(&subscriptions, "面板订阅", "https://example.com/p.yaml");
    seed_cache(temporary.path(), id);
    edit_config(temporary.path(), |config| {
        // 新装默认 clash_api_enabled = true：面板特性会注入 external_ui，
        // 预览必须在注入后移除，避免任何消费方触发面板下载。
        assert!(config.clash_api_enabled);
        config.request_proxy_mode = RequestProxyMode::Core;
    });
    let _ = server;

    let preview = composer.compose().await.expect("compose with clash api");
    let value = parsed_json(&preview);
    let clash_api = value["experimental"]["clash_api"]
        .as_object()
        .expect("clash_api injected");
    assert!(clash_api.contains_key("external_controller"));
    assert!(
        !clash_api.contains_key("external_ui"),
        "external_ui must be stripped:{clash_api:?}"
    );
    assert!(
        !clash_api.contains_key("external_ui_download_url"),
        "external_ui_download_url must be stripped:{clash_api:?}"
    );
}

#[tokio::test]
async fn compose_warns_and_falls_back_when_slices_are_corrupt() {
    let temporary = tempfile::tempdir().unwrap();
    let (subscriptions, _, composer) = services(temporary.path());
    let id = add_active(&subscriptions, "切片订阅", "https://example.com/s.yaml");
    seed_cache(temporary.path(), id);
    edit_config(temporary.path(), |config| {
        config.request_proxy_mode = RequestProxyMode::Core;
    });
    std::fs::write(
        temporary.path().join("overwrite/sing-box/dns.json"),
        "{not json",
    )
    .unwrap();

    let preview = composer
        .compose()
        .await
        .expect("corrupt slices fall back to defaults");
    assert_eq!(preview.node_count(), 2);
    assert!(
        preview
            .warnings()
            .iter()
            .any(|warning| warning.contains("配置切片读取失败") && warning.contains("dns.json")),
        "{:?}",
        preview.warnings()
    );
}

#[tokio::test]
async fn compose_applies_singbox_overwrite_slices_and_expands_nodes() {
    let temporary = tempfile::tempdir().unwrap();
    let (subscriptions, _, composer) = services(temporary.path());
    let id = add_active(&subscriptions, "切片合成订阅", "https://example.com/s.yaml");
    seed_cache(temporary.path(), id);
    edit_config(temporary.path(), |config| {
        config.request_proxy_mode = RequestProxyMode::Core;
    });
    let overwrite_store = lanpath_client::singbox_overwrite::SingboxOverwriteStore::new(
        temporary.path().to_path_buf(),
    );
    overwrite_store
        .save_raw(
            lanpath_client::singbox_overwrite::SingboxSliceKind::Log,
            r#"{ "log": { "level": "debug", "timestamp": false } }"#,
        )
        .unwrap();

    let preview = composer.compose().await.expect("sing-box composes");
    let value: serde_json::Value = serde_json::from_str(preview.content()).unwrap();
    assert_eq!(value["log"]["level"], "debug");
    assert_eq!(value["log"]["timestamp"], false);
    let proxy = value["outbounds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["tag"] == "proxy")
        .expect("proxy group");
    let members = proxy["outbounds"].as_array().unwrap();
    assert!(members.iter().any(|m| m == "节点甲"), "{members:?}");
    assert!(members.iter().any(|m| m == "节点乙"), "{members:?}");
    assert!(!members.iter().any(|m| m == "$nodes"), "{members:?}");
    assert!(
        !temporary.path().join("local_override.json").exists(),
        "sing-box composition must never create legacy local_override.json"
    );
    // 默认切片不再内置 geosite-private / geoip-private，完全开放给用户自选。
    let route_sets = value["route"]["rule_set"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        route_sets.is_empty(),
        "unexpected builtin rule sets: {route_sets:?}"
    );
}

#[tokio::test]
async fn compose_expands_ruleset_groups_across_rules_and_dns_and_injects_local_sets() {
    let temporary = tempfile::tempdir().unwrap();
    let (subscriptions, _, composer) = services(temporary.path());
    let id = add_active(&subscriptions, "分组展开订阅", "https://example.com/g.yaml");
    seed_cache(temporary.path(), id);
    edit_config(temporary.path(), |config| {
        config.request_proxy_mode = RequestProxyMode::Core;
    });

    let rs_store = lanpath_client::rulesets::RuleSetStore::new(temporary.path().to_path_buf());
    rs_store
        .save_manifest(&lanpath_client::rulesets::RuleSetManifest {
            version: lanpath_client::rulesets::MANIFEST_VERSION,
            items: vec![
                lanpath_client::rulesets::RuleSetEntry {
                    id: "rs-a".into(),
                    tag: "corp-a".into(),
                    name: "Corp A".into(),
                    category: "custom".into(),
                    source: lanpath_client::rulesets::RuleSetSource::Manual {
                        content: serde_json::json!({"version": 2, "rules": [{"domain_suffix": ["a.corp"]}]})
                            .to_string(),
                    },
                    remote_state: Default::default(),
                    created_at: 1_700_000_000,
                },
                lanpath_client::rulesets::RuleSetEntry {
                    id: "rs-b".into(),
                    tag: "corp-b".into(),
                    name: "Corp B".into(),
                    category: "custom".into(),
                    source: lanpath_client::rulesets::RuleSetSource::Manual {
                        content: serde_json::json!({"version": 2, "rules": [{"domain_suffix": ["b.corp"]}]})
                            .to_string(),
                    },
                    remote_state: Default::default(),
                    created_at: 1_700_000_000,
                },
            ],
            groups: vec![lanpath_client::rulesets::RuleSetGroup {
                id: "grp-corp".into(),
                tag: "corp-all".into(),
                name: "公司全部".into(),
                description: String::new(),
                member_tags: vec!["corp-a".into(), "corp-b".into()],
                created_at: 1_700_000_000,
            }],
        })
        .unwrap();

    // 在 rules.toml 中引用 @corp-all 分组
    crate::rules::Rules::for_root(temporary.path())
        .unwrap()
        .add_lines("ruleset @corp-all -> direct", true)
        .unwrap();

    // 在 dns.json 中引用 @corp-all 分组
    let overwrite_store = lanpath_client::singbox_overwrite::SingboxOverwriteStore::new(
        temporary.path().to_path_buf(),
    );
    let mut dns_val = overwrite_store
        .load_value(lanpath_client::singbox_overwrite::SingboxSliceKind::Dns)
        .unwrap();
    dns_val["dns"]["rules"] = serde_json::json!([{ "rule_set": "@corp-all", "server": "local" }]);
    overwrite_store
        .save_raw(
            lanpath_client::singbox_overwrite::SingboxSliceKind::Dns,
            &serde_json::to_string_pretty(&dns_val).unwrap(),
        )
        .unwrap();

    let preview = composer.compose().await.expect("compose succeeds");
    let value: serde_json::Value = serde_json::from_str(preview.content()).unwrap();
    assert_eq!(
        value["dns"]["rules"][0]["rule_set"],
        serde_json::json!(["corp-a", "corp-b"])
    );
    let route_rules = value["route"]["rules"].as_array().unwrap();
    assert!(route_rules.iter().any(|r| {
        r.get("rule_set") == Some(&serde_json::json!(["corp-a", "corp-b"]))
            && r.get("outbound") == Some(&serde_json::json!("direct"))
    }));
    let sets = value["route"]["rule_set"].as_array().unwrap();
    assert_eq!(sets.len(), 2);
    assert!(
        sets.iter()
            .any(|s| s["tag"] == "corp-a" && s["path"] == "rulesets/files/rs-a.json")
    );
    assert!(
        sets.iter()
            .any(|s| s["tag"] == "corp-b" && s["path"] == "rulesets/files/rs-b.json")
    );
}

#[tokio::test]
async fn compose_rejects_a_missing_bound_profile() {
    let temporary = tempfile::tempdir().unwrap();
    let (subscriptions, profiles, composer) = services(temporary.path());
    let id = add_active(&subscriptions, "失联订阅", "https://example.com/l.yaml");
    seed_cache(temporary.path(), id);
    let profile = profiles.save(None, "将删除", String::new(), None).unwrap();
    let updated = subscriptions
        .load()
        .unwrap()
        .into_iter()
        .find(|subscription| subscription.id == id)
        .unwrap();
    subscriptions
        .update(
            id,
            &updated.name,
            &updated.url,
            updated.user_agent.as_deref(),
            Some(profile.id),
        )
        .unwrap();
    edit_config(temporary.path(), |config| {
        config.request_proxy_mode = RequestProxyMode::Core;
    });
    // 模板被外部删除（同根另一操作/损坏回退空列表），引用必须真实报错。
    std::fs::remove_file(temporary.path().join("profiles.json")).unwrap();

    let error = composer.compose().await.expect_err("missing profile");
    assert!(error.to_string().contains("覆写模板不存在"), "{error}");
}

/// `compose_with_config`（核心启动路径）：合成严格使用调用方固定的快照——
/// 磁盘上的已保存端口/策略与快照不同也不影响本次合成（启动与校验同一份
/// 快照，`compose` 只是先读快照的便捷入口）。
#[tokio::test]
async fn compose_with_config_uses_the_caller_fixed_snapshot() {
    let temporary = tempfile::tempdir().unwrap();
    let (_subscriptions, _profiles, composer) = services(temporary.path());
    let store = SubscriptionStore::new(temporary.path().to_path_buf());
    let subscription = store
        .add("快照订阅", "https://example.com/s.yaml", true, None)
        .unwrap();
    seed_cache(temporary.path(), subscription.id);
    edit_config(temporary.path(), |config| {
        config.request_proxy_mode = RequestProxyMode::Core;
        config.active_subscription_id = Some(subscription.id);
        config.mixed_port = 17890;
        config.clash_api_port = 9090;
    });

    // 调用方快照（与磁盘不同）：mixed 18765 / API 18766。
    let snapshot = {
        let mut config = crate::request_settings::load_request_config(temporary.path()).unwrap();
        config.mixed_port = 18765;
        config.clash_api_port = 18766;
        config
    };
    let preview = composer
        .compose_with_config(&snapshot)
        .await
        .expect("compose with snapshot");
    let value = parsed_json(&preview);
    assert_eq!(value["inbounds"][0]["listen_port"], 18765);
    assert_eq!(
        value["experimental"]["clash_api"]["external_controller"],
        "127.0.0.1:18766"
    );
    // 磁盘配置不被改写。
    let on_disk = crate::request_settings::load_request_config(temporary.path()).unwrap();
    assert_eq!(on_disk.mixed_port, 17890);
    assert_eq!(on_disk.clash_api_port, 9090);
}
