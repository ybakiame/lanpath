use std::{sync::Arc, time::Duration};

use lanpath_client::{
    CachedSubscriptionContent, ScriptDialect, SubFormat, core_config::wait_clash_api_ready,
    mitm::MitmService, remote::TracingNotifier,
};
use lanpath_engine::{CoreManager, SingBoxProcessManager};
use serde_json::json;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use super::BusinessService;
use crate::configuration::Bindings;

#[tokio::test]
#[ignore = "requires LANPATH_TEST_SING_BOX pointing to a real sing-box executable"]
async fn generated_pipeline_records_real_mitm_traffic() {
    let binary = std::env::var_os("LANPATH_TEST_SING_BOX")
        .map(std::path::PathBuf::from)
        .expect("set LANPATH_TEST_SING_BOX to a real sing-box executable");
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    std::fs::create_dir(root_path.join("cache")).unwrap();
    let business = BusinessService::new(
        &root_path,
        Some(binary.clone()),
        Bindings::new(17899, 19090),
    );

    let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_port = target.local_addr().unwrap().port();
    let target_task = tokio::spawn(async move {
        let (mut stream, _) = target.accept().await.unwrap();
        let mut request = [0; 2048];
        let _ = stream.read(&mut request).await.unwrap();
        let body = "mitm-target-ok";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let store = business.subscription_store();
    let subscription = store
        .add("MITM fixture", "https://example.invalid/sub", true, None)
        .unwrap();
    store
        .write_cached_content(
            subscription.id,
            &CachedSubscriptionContent {
                format: SubFormat::SingBoxJson,
                singbox_nodes: vec![json!({
                    "type": "shadowsocks",
                    "tag": "fixture-node",
                    "server": "127.0.0.1",
                    "server_port": 8388,
                    "method": "aes-128-gcm",
                    "password": "fixture"
                })],
            },
        )
        .unwrap();
    business
        .set_active_subscription(Some(subscription.id))
        .unwrap();
    let mut slices = business.config_slices().unwrap().slices;
    slices.route.final_tag = "direct".into();
    business.save_config_slices(&slices).unwrap();
    business
        .save_mitm_settings(true, vec!["127.0.0.1".into()], ScriptDialect::Surge)
        .unwrap();

    let client_config = business.client_config().unwrap();
    let mitm_service = MitmService::new(
        &root_path,
        Arc::new(lanpath_client::http_exec::ReqwestHttpExecutor::for_data_dir(&root_path)),
    );
    let mut session = mitm_service
        .start(&client_config, Arc::new(TracingNotifier::new()))
        .await
        .unwrap()
        .expect("saved whitelist should start MITM");
    let (generated, _) = business
        .build_config_report(
            false,
            Some(session.chain()),
            business.client_config().unwrap(),
        )
        .await
        .unwrap();
    business
        .validator(&business.client_config().unwrap())
        .validate_value(&generated)
        .await
        .unwrap();
    assert_eq!(generated["inbounds"].as_array().unwrap().len(), 2);
    assert!(
        generated["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|outbound| {
                outbound["tag"] == "lanpath-mitm"
                    && outbound["server_port"] == json!(session.addr().port())
            })
    );

    let manager = SingBoxProcessManager::new(&binary, &root_path).unwrap();
    manager.start(&generated).await.unwrap_or_else(|error| {
        panic!(
            "{error}\n{}",
            serde_json::to_string_pretty(&generated).unwrap()
        )
    });
    wait_clash_api_ready(19090, "", Duration::from_secs(15))
        .await
        .unwrap();
    let mut proxied = tokio::net::TcpStream::connect("127.0.0.1:17899")
        .await
        .unwrap();
    proxied
        .write_all(
            format!(
                "GET http://127.0.0.1:{target_port}/mitm HTTP/1.1\r\nHost: 127.0.0.1:{target_port}\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    let mut response = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        match proxied.read(&mut chunk).await {
            Ok(0) => break,
            Ok(read) => response.extend_from_slice(&chunk[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => break,
            Err(error) => panic!("proxy response read failed: {error}"),
        }
    }
    let response = String::from_utf8(response).unwrap();
    let core_error = manager.last_error().await.unwrap();
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "unexpected proxy response: {response:?}; core output: {core_error}"
    );
    assert!(
        response.ends_with("mitm-target-ok"),
        "target body missing: {response:?}"
    );
    assert!(
        mitm_service
            .traffic()
            .iter()
            .any(|record| record.url.contains("127.0.0.1") && record.response_status == 200)
    );

    manager.stop().await.unwrap();
    session.stop().await.unwrap();
    target_task.await.unwrap();
    assert!(!manager.is_running().await);
}
