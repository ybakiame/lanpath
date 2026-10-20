use lanpath_client::{CachedSubscriptionContent, RequestProxyMode, SubFormat, SubscriptionSource};
use serde_json::json;

use super::BusinessService;
use crate::configuration::Bindings;

fn service(root: &std::path::Path) -> BusinessService {
    BusinessService::new(root, None, Bindings::new(17899, 19090))
}

#[test]
fn client_state_persists_selection_but_forces_experimental_network_boundaries() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path());
    let subscription = service
        .subscription_store()
        .add("fixture", "https://example.invalid/sub", true, None)
        .unwrap();
    service
        .set_active_subscription(Some(subscription.id))
        .unwrap();

    let mut tampered = service.client_config().unwrap();
    tampered.tun_enabled = true;
    tampered.system_proxy_enabled = true;
    tampered.request_proxy_mode = RequestProxyMode::System;
    tampered.mixed_port = 1;
    tampered.clash_api_port = 2;
    tampered.save().unwrap();

    let normalized = service.client_config().unwrap();
    assert_eq!(normalized.active_subscription_id, Some(subscription.id));
    assert_eq!(normalized.mixed_port, 17899);
    assert_eq!(normalized.clash_api_port, 19090);
    assert!(!normalized.tun_enabled);
    assert!(!normalized.system_proxy_enabled);
    assert_eq!(normalized.request_proxy_mode, RequestProxyMode::None);
}

#[test]
fn linked_profile_cannot_be_deleted_and_invalid_overrides_never_persist() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path());
    let profile = service
        .save_profile(None, "default", String::new(), String::new(), None, None)
        .unwrap();
    assert!(
        service
            .save_profile(
                Some(profile.id),
                "default",
                "- invalid".into(),
                String::new(),
                None,
                None,
            )
            .is_err()
    );
    assert!(service.profiles().unwrap()[0].yaml_override.is_empty());

    let subscription = service
        .subscription_store()
        .add("fixture", "https://example.invalid/sub", true, None)
        .unwrap();
    service
        .subscription_store()
        .set_profile_id(subscription.id, Some(profile.id))
        .unwrap();
    assert!(service.remove_profile(profile.id).is_err());
    service
        .subscription_store()
        .set_profile_id(subscription.id, None)
        .unwrap();
    service.remove_profile(profile.id).unwrap();
}

#[tokio::test]
async fn selected_subscription_and_linked_profile_generate_safe_runtime_config() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path());
    let profile = service
        .save_profile(
            None,
            "fixture override",
            "log:\n  level: debug\n".into(),
            String::new(),
            None,
            None,
        )
        .unwrap();
    let subscription = service
        .subscription_store()
        .add("fixture", "https://example.invalid/sub", true, None)
        .unwrap();
    service
        .subscription_store()
        .set_profile_id(subscription.id, Some(profile.id))
        .unwrap();
    service
        .subscription_store()
        .write_cached_content(
            subscription.id,
            &CachedSubscriptionContent {
                format: SubFormat::SingBoxJson,
                singbox_nodes: vec![json!({
                    "type": "shadowsocks",
                    "tag": "node-a",
                    "server": "127.0.0.1",
                    "server_port": 8388,
                    "method": "2022-blake3-aes-128-gcm",
                    "password": "fixture"
                })],
            },
        )
        .unwrap();
    service
        .set_active_subscription(Some(subscription.id))
        .unwrap();

    let reopened = BusinessService::new(root.path(), None, Bindings::new(17899, 19090));
    assert_eq!(reopened.profiles().unwrap()[0].id, profile.id);
    assert_eq!(
        reopened.active_subscription_id().unwrap(),
        Some(subscription.id)
    );
    let generated = reopened.build_config(false).await.unwrap();
    assert_eq!(generated.pointer("/log/level"), Some(&json!("debug")));
    assert_eq!(
        generated.pointer("/inbounds/0/listen"),
        Some(&json!("127.0.0.1"))
    );
    assert_eq!(
        generated.pointer("/inbounds/0/listen_port"),
        Some(&json!(17899))
    );
    assert_eq!(
        generated.pointer("/experimental/clash_api/external_controller"),
        Some(&json!("127.0.0.1:19090"))
    );
    assert_eq!(
        generated.pointer("/experimental/cache_file/path"),
        Some(&json!(
            root.path()
                .join("cache")
                .join("cache.db")
                .to_string_lossy()
                .into_owned()
        ))
    );
    assert!(
        generated
            .pointer("/experimental/clash_api/external_ui")
            .is_none()
    );
    assert!(
        generated["inbounds"]
            .as_array()
            .unwrap()
            .iter()
            .all(|inbound| inbound["type"] != "tun")
    );
    assert!(
        generated["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|outbound| outbound.get("tag") == Some(&json!("node-a")))
    );
}

#[tokio::test]
async fn local_http_subscription_is_fetched_parsed_and_cached() {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let body = r#"{"outbounds":[{"type":"shadowsocks","tag":"fetched-node","server":"127.0.0.1","server_port":8388,"method":"aes-128-gcm","password":"fixture"}]}"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 2048];
        let _ = stream.read(&mut request).await.unwrap();
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path());
    let subscription = service
        .add_subscription(
            "local fixture",
            &format!("http://{address}/subscription"),
            None,
            None,
        )
        .await
        .unwrap();
    server.await.unwrap();

    assert_eq!(subscription.node_count, 1);
    assert!(subscription.error.is_none());
    let store = service.subscription_store();
    let source = store.load_source(subscription.id).unwrap().unwrap();
    assert_eq!(source.content, body);
    assert_eq!(
        store
            .load_cached_content(subscription.id)
            .unwrap()
            .singbox_nodes[0]["tag"],
        json!("fetched-node")
    );
}

#[tokio::test]
async fn refresh_failure_persists_error_and_keeps_last_successful_source_and_nodes() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let subscription = service
        .subscription_store()
        .add(
            "fixture",
            &format!("http://{address}/subscription"),
            true,
            None,
        )
        .unwrap();
    let cached = CachedSubscriptionContent {
        format: SubFormat::SingBoxJson,
        singbox_nodes: vec![json!({
            "type": "shadowsocks",
            "tag": "cached-node",
            "server": "127.0.0.1",
            "server_port": 8388,
            "method": "aes-128-gcm",
            "password": "fixture"
        })],
    };
    let source = SubscriptionSource {
        format: SubFormat::SingBoxJson,
        content: "{\"outbounds\":[]}".into(),
    };
    service
        .subscription_store()
        .write_cached_content(subscription.id, &cached)
        .unwrap();
    service
        .subscription_store()
        .write_source(subscription.id, &source)
        .unwrap();

    let refreshed = service.refresh_subscription(subscription.id).await.unwrap();
    assert!(refreshed.error.is_some());
    let store = service.subscription_store();
    assert_eq!(
        store
            .load_cached_content(subscription.id)
            .unwrap()
            .singbox_nodes,
        cached.singbox_nodes
    );
    assert_eq!(
        store.load_source(subscription.id).unwrap().unwrap().content,
        source.content
    );
}

#[test]
fn update_rejects_missing_profile_reference_without_changing_subscription() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path());
    let subscription = service
        .subscription_store()
        .add("fixture", "https://example.invalid/original", true, None)
        .unwrap();
    let result = service.update_subscription(
        subscription.id,
        "changed",
        "https://example.invalid/changed",
        None,
        Some(uuid::Uuid::nil()),
    );
    assert!(result.is_err());
    let saved = service.subscriptions().unwrap().remove(0);
    assert_eq!(saved.name, "fixture");
    assert_eq!(saved.url, "https://example.invalid/original");
}

#[test]
fn stale_refresh_result_cannot_restore_an_edited_subscription() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path());
    let store = service.subscription_store();
    let mut stale = store
        .add("fixture", "https://example.invalid/original", true, None)
        .unwrap();
    store
        .update(stale.id, "changed", "https://example.invalid/changed", None)
        .unwrap();
    stale.node_count = 99;
    stale.error = None;

    let current = super::subscriptions::write_refresh_result(&store, &stale).unwrap();
    assert_eq!(current.name, "changed");
    assert_eq!(current.url, "https://example.invalid/changed");
    assert_eq!(current.node_count, 0);
}

#[tokio::test]
#[ignore = "requires LANPATH_TEST_SING_BOX pointing to a real sing-box executable"]
async fn generated_subscription_pipeline_starts_real_core() {
    use lanpath_engine::{CoreManager, SingBoxProcessManager};
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let binary = std::env::var_os("LANPATH_TEST_SING_BOX")
        .map(std::path::PathBuf::from)
        .expect("set LANPATH_TEST_SING_BOX to a real sing-box executable");
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    std::fs::create_dir(root_path.join("cache")).unwrap();
    let service = BusinessService::new(
        &root_path,
        Some(binary.clone()),
        Bindings::new(17899, 19090),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let body = r#"{"outbounds":[{"type":"shadowsocks","tag":"fixture-node","server":"127.0.0.1","server_port":8388,"method":"aes-128-gcm","password":"fixture"}]}"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 2048];
        let _ = stream.read(&mut request).await.unwrap();
        stream.write_all(response.as_bytes()).await.unwrap();
    });
    let subscription = service
        .add_subscription(
            "real core fixture",
            &format!("http://{address}/subscription"),
            None,
            None,
        )
        .await
        .unwrap();
    server.await.unwrap();
    assert_eq!(subscription.node_count, 1);
    service
        .set_active_subscription(Some(subscription.id))
        .unwrap();

    let generated = service.build_config(false).await.unwrap();
    service
        .validator()
        .validate_value(&generated)
        .await
        .unwrap();
    let manager = SingBoxProcessManager::new(&binary, &root_path).unwrap();
    manager.start(&generated).await.unwrap();
    assert!(manager.is_running().await);
    manager.stop().await.unwrap();
    assert!(!manager.is_running().await);
}
