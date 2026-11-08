use super::BusinessService;
use crate::configuration::Bindings;
use lanpath_client::{
    ScriptDialect,
    mitm::MitmService,
    remote::{RemoteKind, RemoteManager, RemoteResource, TracingNotifier},
};
use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    task::JoinHandle,
};

/// Exactly two loopback requests: the declared module and its associated JS.
/// Shutting this server down before activation proves the cache is sufficient.
pub(super) async fn module_server(hosts: &str) -> (String, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let snippet = format!(
        "[Script]\nresponse=type=http-response,pattern=^http://127\\.0\\.0\\.1:,script-path=http://{addr}/response.js,requires-body=true,max-size=-1\n[MITM]\nhostname=%APPEND% {hosts}\n"
    );
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 2048];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            let body = if request.starts_with("GET /module.sgmodule ") {
                snippet.as_str()
            } else if request.starts_with("GET /response.js ") {
                "$done({body: $response.body + '-script-ok'});"
            } else {
                panic!("unexpected fixture request")
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    (format!("http://{addr}/module.sgmodule"), server)
}

pub(super) async fn fetch_module(business: &BusinessService, hosts: &str) {
    let (url, server) = module_server(hosts).await;
    business
        .add_remote_resource(RemoteResource {
            name: "response-module".into(),
            url,
            kind: RemoteKind::Snippet,
            dialect: ScriptDialect::Surge,
            update_interval_secs: 0,
            ..Default::default()
        })
        .await
        .unwrap();
    let report = business.fetch_remote_resources().await.unwrap();
    assert_eq!(report.fetched, 1);
    assert_eq!(report.scripts, 1);
    assert_eq!(report.warnings.len(), 1);
    assert!(report.warnings[0].contains("32 MiB"));
    server.await.unwrap();
}

#[tokio::test]
async fn updated_response_module_hosts_reach_saved_view_and_startup_chain() {
    let root = tempfile::tempdir().unwrap();
    let business = BusinessService::new(root.path(), None, Bindings::new(17899, 19090));
    let initial = business
        .save_mitm_settings(true, vec![], ScriptDialect::Surge)
        .unwrap();
    assert!(initial.whitelist.derived.is_empty());
    fetch_module(&business, "127.0.0.1, -excluded.example").await;
    let saved = business.mitm_settings().unwrap();
    assert!(saved.hostnames.is_empty());
    assert!(saved.whitelist.has_inclusions());
    let merged = RemoteManager::new(root.path().to_owned())
        .load_cached()
        .unwrap();
    assert_eq!(merged.scripts.len(), 1);
    assert!(merged.scripts[0].source.contains("-script-ok"));
    for entry in &saved.whitelist.derived {
        assert!(merged.hostnames.contains(&entry.hostname));
        assert_eq!(entry.sources, ["response-module"]);
    }
    assert_eq!(saved.whitelist.derived.len(), 2);
    let service = MitmService::new(
        root.path(),
        Arc::new(lanpath_script::MockHttpExecutor::with_responses(vec![])),
    );
    let mut session = service
        .start(
            &business.client_config().unwrap(),
            Arc::new(TracingNotifier::new()),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(session.chain().hostnames, merged.hostnames);
    session.stop().await.unwrap();
    business
        .set_remote_resource_enabled("response-module", false)
        .await
        .unwrap();
    assert!(
        business
            .mitm_settings()
            .unwrap()
            .whitelist
            .derived
            .is_empty()
    );
    assert!(
        service
            .start(
                &business.client_config().unwrap(),
                Arc::new(TracingNotifier::new())
            )
            .await
            .unwrap()
            .is_none()
    );
    business
        .set_remote_resource_enabled("response-module", true)
        .await
        .unwrap();
    assert_eq!(business.mitm_settings().unwrap().whitelist.derived.len(), 2);
}

#[tokio::test]
async fn response_pattern_does_not_invent_hosts_and_exclusions_do_not_start_mitm() {
    let root = tempfile::tempdir().unwrap();
    let business = BusinessService::new(root.path(), None, Bindings::new(17899, 19090));
    business
        .save_mitm_settings(true, vec![], ScriptDialect::Surge)
        .unwrap();
    fetch_module(&business, "-excluded.example").await;
    let saved = business.mitm_settings().unwrap();
    assert_eq!(saved.whitelist.derived.len(), 1);
    assert!(!saved.whitelist.has_inclusions());
    let service = MitmService::new(
        root.path(),
        Arc::new(lanpath_script::MockHttpExecutor::with_responses(vec![])),
    );
    assert!(
        service
            .start(
                &business.client_config().unwrap(),
                Arc::new(TracingNotifier::new())
            )
            .await
            .unwrap()
            .is_none()
    );
    assert!(!root.path().join("certs").exists());
}
