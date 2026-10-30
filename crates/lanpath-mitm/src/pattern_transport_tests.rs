use crate::{
    CaStore, FileCaStore, MemoryRecorder, MitmConfig, MitmProxy, Pattern, Phase, RewriteEngine,
    RewriteKind, RewriteRule, pattern::MAX_FALLBACK_INPUT_BYTES,
};
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn body_rule(phase: Phase) -> RewriteEngine {
    RewriteEngine {
        rules: vec![RewriteRule {
            pattern: Pattern::new(r"^http://127\.0\.0\.1:").unwrap(),
            kind: RewriteKind::BodyRewrite {
                phase,
                body_pattern: Some(Pattern::new(r"a(?=a)").unwrap()),
                replacement: "b".into(),
            },
        }],
    }
}

#[tokio::test]
async fn request_regex_budget_failure_never_reaches_upstream() {
    let origin = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let destination = format!("http://{}/owned", origin.local_addr().unwrap());
    let dir = tempfile::tempdir().unwrap();
    let ca = FileCaStore::new(dir.path()).load_or_generate().unwrap();
    let config = MitmConfig {
        listen_addr: "127.0.0.1:0".parse().unwrap(),
        max_body_size: MAX_FALLBACK_INPUT_BYTES + 1024,
        record_enabled: false,
        ..MitmConfig::default()
    };
    let mut proxy = MitmProxy::new(
        config,
        body_rule(Phase::Request),
        None,
        Arc::new(MemoryRecorder::new(1)),
        ca,
    )
    .start()
    .await
    .unwrap();
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::http(format!("http://{}", proxy.addr)).unwrap())
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let result = client
        .post(destination)
        .body("a".repeat(MAX_FALLBACK_INPUT_BYTES + 1))
        .send()
        .await;
    assert!(
        result.is_err(),
        "a rejected exchange must not return a synthetic success"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), origin.accept())
            .await
            .is_err()
    );
    proxy.shutdown().await.unwrap();
}

#[tokio::test]
async fn response_regex_budget_failure_is_not_an_empty_success() {
    let origin = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let destination = format!("http://{}/owned", origin.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = origin.accept().await.unwrap();
        let mut request = Vec::new();
        let mut bytes = [0; 512];
        while !request.windows(4).any(|tail| tail == b"\r\n\r\n") {
            let count = socket.read(&mut bytes).await.unwrap();
            assert_ne!(count, 0);
            request.extend_from_slice(&bytes[..count]);
            assert!(request.len() < 8192);
        }
        let body = "a".repeat(MAX_FALLBACK_INPUT_BYTES + 1);
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        socket.write_all(headers.as_bytes()).await.unwrap();
        socket.write_all(body.as_bytes()).await.unwrap();
        socket.shutdown().await.unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let ca = FileCaStore::new(dir.path()).load_or_generate().unwrap();
    let config = MitmConfig {
        listen_addr: "127.0.0.1:0".parse().unwrap(),
        max_body_size: MAX_FALLBACK_INPUT_BYTES + 1024,
        record_enabled: false,
        ..MitmConfig::default()
    };
    let mut proxy = MitmProxy::new(
        config,
        body_rule(Phase::Response),
        None,
        Arc::new(MemoryRecorder::new(1)),
        ca,
    )
    .start()
    .await
    .unwrap();
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::http(format!("http://{}", proxy.addr)).unwrap())
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    assert!(
        client.get(destination).send().await.is_err(),
        "the handler abort flag must stop response delivery"
    );
    server.await.unwrap();
    proxy.shutdown().await.unwrap();
}
