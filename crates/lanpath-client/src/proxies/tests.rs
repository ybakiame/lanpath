use super::*;

#[test]
fn parse_proxies_response_builds_groups_and_nodes() {
    let json = serde_json::json!({
        "proxies": {
            "GLOBAL": {
                "name": "GLOBAL",
                "type": "Selector",
                "now": "Auto",
                "all": ["Auto", "DIRECT"]
            },
            "Auto": {
                "name": "Auto",
                "type": "URLTest",
                "now": "Node-A",
                "all": ["Node-A", "Node-B", "DIRECT"],
                "history": [{"delay": 120}]
            },
            "Node-A": {
                "name": "Node-A",
                "type": "Shadowsocks",
                "history": [{"delay": 150}],
                "udp": true
            },
            "Node-B": {
                "name": "Node-B",
                "type": "Vmess",
                "history": [],
                "udp": false
            },
            "DIRECT": {
                "name": "DIRECT",
                "type": "Direct",
                "history": []
            },
            "REJECT": {
                "name": "REJECT",
                "type": "Reject",
                "history": []
            }
        }
    });

    let list = parse_proxies_response(&json).unwrap();

    // GLOBAL skipped.
    assert!(!list.groups.iter().any(|g| g.name == "GLOBAL"));

    // Auto group present.
    let auto = list.groups.iter().find(|g| g.name == "Auto").unwrap();
    assert_eq!(auto.group_type, "URLTest");
    assert_eq!(auto.now, "Node-A");
    assert_eq!(auto.members, vec!["Node-A", "Node-B", "DIRECT"]);

    // Nodes: DIRECT / REJECT filtered out.
    let node_names: Vec<&str> = list.nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(node_names.contains(&"Node-A"));
    assert!(node_names.contains(&"Node-B"));
    assert!(!node_names.contains(&"DIRECT"));
    assert!(!node_names.contains(&"REJECT"));

    // Node details.
    let node_a = list.nodes.iter().find(|n| n.name == "Node-A").unwrap();
    assert_eq!(node_a.node_type, "Shadowsocks");
    assert_eq!(node_a.delay_ms, Some(150));
    assert!(node_a.udp);

    let node_b = list.nodes.iter().find(|n| n.name == "Node-B").unwrap();
    assert_eq!(node_b.node_type, "Vmess");
    assert_eq!(node_b.delay_ms, None);
    assert!(!node_b.udp);
}

#[test]
fn parse_proxies_response_skips_compatible_pseudo_nodes() {
    let json = serde_json::json!({
        "proxies": {
            "COMPATIBLE": {
                "name": "COMPATIBLE",
                "type": "Compatible",
                "history": []
            },
            "RealNode": {
                "name": "RealNode",
                "type": "Trojan",
                "history": [{"delay": 200}],
                "udp": true
            }
        }
    });
    let list = parse_proxies_response(&json).unwrap();
    assert_eq!(list.nodes.len(), 1);
    assert_eq!(list.nodes[0].name, "RealNode");
}

#[test]
fn group_selections_serde_backward_compat() {
    // Old client.json without group_selections should load with default empty map.
    let json = r#"{
        "data_dir": "/tmp/lanpath-client-test",
        "hub_url": "http://127.0.0.1:50052",
        "sub_token": "tok",
        "core_type": "singbox",
        "core_binary": "/usr/local/bin/sing-box",
        "mixed_port": 17890,
        "mitm_enabled": true,
        "mitm": { "ca_dir": "/tmp/lanpath-client-test/certs", "hostnames": [], "script_dialect": "Surge" },
        "system_proxy_enabled": false
    }"#;
    let cfg: crate::config::ClientConfig = serde_json::from_str(json).unwrap();
    assert!(cfg.group_selections.is_empty());
}

#[test]
fn group_selections_roundtrip() {
    let mut cfg = crate::config::ClientConfig::new(
        std::path::PathBuf::from("/tmp/lanpath-client-test"),
        "http://127.0.0.1:50052",
        "tok",
        std::path::PathBuf::from("/usr/local/bin/sing-box"),
    );
    cfg.group_selections.insert("Auto".into(), "Node-A".into());
    cfg.group_selections.insert("Proxy".into(), "Node-B".into());

    let json = serde_json::to_string(&cfg).unwrap();
    let back: crate::config::ClientConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back.group_selections.len(), 2);
    assert_eq!(
        back.group_selections.get("Auto"),
        Some(&"Node-A".to_string())
    );
    assert_eq!(
        back.group_selections.get("Proxy"),
        Some(&"Node-B".to_string())
    );
}

#[tokio::test]
async fn clash_get_proxies_hits_local_endpoint() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = axum::Router::new().route(
        "/proxies",
        axum::routing::get(|| async {
            axum::Json(serde_json::json!({
                "proxies": {
                    "Proxy": {
                        "name": "Proxy",
                        "type": "Selector",
                        "now": "Node1",
                        "all": ["Node1", "Node2"]
                    },
                    "Node1": {
                        "name": "Node1",
                        "type": "Shadowsocks",
                        "history": [{"delay": 100}],
                        "udp": true
                    },
                    "Node2": {
                        "name": "Node2",
                        "type": "Vmess",
                        "history": [],
                        "udp": false
                    }
                }
            }))
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let list = clash_get_proxies(addr.port(), "").await.unwrap();
    assert_eq!(list.groups.len(), 1);
    assert_eq!(list.groups[0].name, "Proxy");
    // Proxy groups themselves are also listed as nodes if they are not pseudo-nodes.
    // Node1 + Node2 = 2 nodes (Proxy is a Selector, not a pseudo-node, so it also appears).
    assert_eq!(list.nodes.len(), 3);
}

#[tokio::test]
async fn clash_select_proxy_puts_with_body() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let captured = std::sync::Arc::new(std::sync::Mutex::new(None));
    let cap_ref = std::sync::Arc::clone(&captured);
    let app = axum::Router::new().route(
        "/proxies/{group}",
        axum::routing::put(
            move |req: axum::http::Request<axum::body::Body>| async move {
                let bytes = axum::body::to_bytes(req.into_body(), 1024).await.unwrap();
                *cap_ref.lock().unwrap() = Some(bytes.to_vec());
                axum::http::StatusCode::NO_CONTENT
            },
        ),
    );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    clash_select_proxy(addr.port(), "", "Proxy", "Node1")
        .await
        .unwrap();
    assert_eq!(
        captured.lock().unwrap().as_ref().unwrap(),
        &br#"{"name":"Node1"}"#.to_vec()
    );
}

#[tokio::test]
async fn clash_test_delay_parses_delay_field() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = axum::Router::new().route(
        "/proxies/{name}/delay",
        axum::routing::get(|| async { axum::Json(serde_json::json!({ "delay": 233 })) }),
    );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let delay = clash_test_delay(addr.port(), "", "Node1", None, 5000)
        .await
        .unwrap();
    assert_eq!(delay, Some(233));
}

#[tokio::test]
async fn clash_test_delay_returns_none_on_non_2xx() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = axum::Router::new().route(
        "/proxies/{name}/delay",
        axum::routing::get(|| async { axum::http::StatusCode::BAD_REQUEST }),
    );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let delay = clash_test_delay(addr.port(), "", "Node1", None, 5000)
        .await
        .unwrap();
    assert_eq!(delay, None);
}

#[tokio::test]
async fn clash_api_uses_bearer_auth_when_secret_set() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let captured = std::sync::Arc::new(std::sync::Mutex::new(None));
    let cap_ref = std::sync::Arc::clone(&captured);
    let app = axum::Router::new().route(
        "/proxies",
        axum::routing::get(
            move |req: axum::http::Request<axum::body::Body>| async move {
                let auth = req
                    .headers()
                    .get("authorization")
                    .cloned()
                    .map(|h| h.to_str().unwrap_or("").to_string());
                *cap_ref.lock().unwrap() = auth;
                axum::Json(serde_json::json!({ "proxies": {} }))
            },
        ),
    );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    clash_get_proxies(addr.port(), "mysecret").await.unwrap();
    assert_eq!(
        captured.lock().unwrap().as_ref().unwrap(),
        "Bearer mysecret"
    );
}
