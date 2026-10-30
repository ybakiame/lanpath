//! Real QuickJS rules exercised through the loopback HTTP proxy, without TLS bypass.
use super::*;
use crate::{HookMode, ScriptHookEngine, ScriptRule};
use lanpath_script::{
    MemoryPersistentStore, MockHttpExecutor, RecordingNotifier, ScriptDialect, ScriptHost,
    ScriptKind, ScriptLimits,
};
use std::sync::atomic::{AtomicUsize, Ordering};

fn hook(kind: ScriptKind, source: &str, limit: usize, binary: bool) -> ScriptRule {
    ScriptRule {
        name: match kind {
            ScriptKind::HttpRequest => "declared-request",
            _ => "declared-response",
        }
        .into(),
        kind,
        pattern: crate::Pattern::new(r"^http://127\.0\.0\.1:").unwrap(),
        requires_body: true,
        max_size: limit,
        source: source.into(),
        argument: None,
        dialect: ScriptDialect::Surge,
        mode: HookMode::Modify,
        enabled: true,
        timeout_ms: None,
        binary,
        full_headers: false,
        source_id: "https://assets.example/real-source.js".into(),
        module_id: Some("fixture-module".into()),
    }
}

async fn start_hooks(rules: Vec<ScriptRule>) -> (RunningProxy, Arc<MemoryRecorder>) {
    let dir = tempfile::tempdir().unwrap();
    let ca = FileCaStore::new(dir.path()).load_or_generate().unwrap();
    let recorder = Arc::new(MemoryRecorder::new(8));
    let host = Arc::new(ScriptHost::new(
        Arc::new(MockHttpExecutor::with_responses(vec![])),
        Arc::new(MemoryPersistentStore::new()),
        Arc::new(RecordingNotifier::new()),
    ));
    let hooks = ScriptHookEngine::new(host, ScriptLimits::default(), rules);
    let proxy = MitmProxy::new(
        MitmConfig::default(),
        RewriteEngine::default(),
        Some(hooks),
        recorder.clone(),
        ca,
    )
    .start()
    .await
    .unwrap();
    (proxy, recorder)
}

fn request_header<'a>(head: &'a [u8], name: &str) -> Option<&'a str> {
    std::str::from_utf8(head)
        .unwrap()
        .split("\r\n")
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .find_map(|(field, value)| field.eq_ignore_ascii_case(name).then_some(value.trim()))
}

#[tokio::test]
async fn surge_ninth_request_byte_is_rejected_before_any_origin_connection() {
    bounded(async {
        // Also select a larger response-body context: it must not hide request overflow.
        let rules = vec![
            hook(
                ScriptKind::HttpRequest,
                "$done({headers:{'x-executed':'yes'}})",
                8,
                false,
            ),
            hook(ScriptKind::HttpResponse, "$done({})", 64, false),
        ];
        let (mut proxy, _) = start_hooks(rules).await;
        for length in [8, 9] {
            let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = origin.local_addr().unwrap();
            let connections = Arc::new(AtomicUsize::new(0));
            let observed = connections.clone();
            let server = tokio::spawn(async move {
                let (mut socket, _) = origin.accept().await.unwrap();
                observed.fetch_add(1, Ordering::SeqCst);
                let head = read_head(&mut socket).await;
                assert_eq!(
                    request_header(&head, "x-executed"),
                    Some("yes"),
                    "head: {:?}",
                    String::from_utf8_lossy(&head)
                );
                let mut bytes = vec![0; length];
                socket.read_exact(&mut bytes).await.unwrap();
                assert_eq!(bytes, vec![b'x'; length]);
                socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    )
                    .await
                    .unwrap();
            });
            let result = client(&proxy)
                .post(format!("http://{address}/limit"))
                .body(vec![b'x'; length])
                .send()
                .await;
            if length == 8 {
                assert_eq!(result.unwrap().bytes().await.unwrap().as_ref(), b"ok");
                server.await.unwrap();
                assert_eq!(connections.load(Ordering::SeqCst), 1);
            } else {
                assert!(result.is_err(), "overflow reached the origin");
                assert_eq!(
                    connections.load(Ordering::SeqCst),
                    0,
                    "origin connected before rejection"
                );
                server.abort();
                let _ = server.await;
            }
        }
        proxy.shutdown().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn surge_response_overflow_skips_quickjs_and_replays_original_wire_body() {
    bounded(async {
        for (compressed, size, limit) in [(false, 9, 8), (true, 9, 8), (true, 4096, 128)] {
            let payload = vec![b'x'; size];
            let bytes = if compressed { gzip(&payload) } else { payload };
            if size == 4096 {
                assert!(bytes.len() < limit);
            }
            let extra = if compressed {
                "Content-Encoding: gzip\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\n"
            } else {
                "Set-Cookie: a=1\r\nSet-Cookie: b=2\r\n"
            };
            let (origin, server) = response_origin(bytes.clone(), extra).await;
            let rule = hook(
                ScriptKind::HttpResponse,
                "$done({status:418,headers:{'x-executed':'yes'},body:'changed'})",
                limit,
                false,
            );
            let (mut proxy, _) = start_hooks(vec![rule]).await;
            let response = client(&proxy)
                .get(format!("http://{origin}/overflow"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            assert!(!response.headers().contains_key("x-executed"));
            assert_eq!(response.headers().get_all("set-cookie").iter().count(), 2);
            if compressed {
                assert_eq!(response.headers()[CONTENT_ENCODING], "gzip");
            }
            assert_eq!(response.bytes().await.unwrap().as_ref(), bytes.as_slice());
            server.await.unwrap();
            proxy.shutdown().await.unwrap();
        }
    })
    .await;
}

#[tokio::test]
async fn surge_selected_limits_above_record_cap_execute_with_binary_policy_and_paired_ids() {
    bounded(async {
        // Cover the imported default and a smaller explicit limit in both body modes.
        for (limit, binary) in [(10 * 1024 * 1024, false), (256 * 1024, true)] {
            let size = 160 * 1024;
            let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = origin.local_addr().unwrap();
            let server = tokio::spawn(async move {
                for _ in 0..2 {
                    let (mut socket, _) = origin.accept().await.unwrap();
                    let head = read_head(&mut socket).await;
                    assert_eq!(request_header(&head, "x-hook-length"), Some("163840"), "head: {:?}", String::from_utf8_lossy(&head));
                    assert_eq!(request_header(&head, "x-hook-name"), Some("declared-request"));
                    let mut body = vec![0; size];
                    socket.read_exact(&mut body).await.unwrap();
                    assert_eq!(body, vec![b'x'; size]);
                    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
                    socket.write_all(&body).await.unwrap();
                }
            });
            let request = hook(ScriptKind::HttpRequest, r#"
                if ($script.binaryBodyMode !== ($request.body instanceof Uint8Array)) throw Error('binary metadata mismatch');
                const h = $request.headers;
                h['x-hook-length'] = String($request.body.length);
                h['x-hook-name'] = $script.name;
                h['x-hook-id'] = $request.id;
                $done({headers:h});
            "#, limit, binary);
            let response = hook(ScriptKind::HttpResponse, r#"
                if ($script.binaryBodyMode !== ($response.body instanceof Uint8Array)) throw Error('binary metadata mismatch');
                const h = $response.headers;
                h['x-request-id'] = $request.id;
                h['x-paired-id'] = $request.headers['x-hook-id'];
                h['x-response-length'] = String($response.body.length);
                h['x-response-name'] = $script.name;
                h['x-binary-mode'] = String($script.binaryBodyMode);
                $done({status:201,headers:h});
            "#, limit, binary);
            let (mut proxy, recorder) = start_hooks(vec![request, response]).await;
            let mut identities = Vec::new();
            for _ in 0..2 {
                let response = client(&proxy).post(format!("http://{address}/large"))
                    .body(vec![b'x'; size]).send().await.unwrap();
                assert_eq!(response.status(), 201);
                assert_eq!(response.headers()["x-response-length"], "163840");
                assert_eq!(response.headers()["x-response-name"], "declared-response");
                assert_eq!(response.headers()["x-binary-mode"], binary.to_string());
                let id = response.headers()["x-request-id"].to_str().unwrap().to_owned();
                assert_eq!(response.headers()["x-paired-id"], id);
                assert_ne!(uuid::Uuid::parse_str(&id).unwrap(), uuid::Uuid::nil());
                identities.push(id);
                assert_eq!(response.bytes().await.unwrap().as_ref(), vec![b'x'; size]);
            }
            assert_ne!(identities[0], identities[1]);
            server.await.unwrap();
            let records = recorder.list();
            assert_eq!(records.len(), 2);
            for (record, id) in records.iter().zip(identities) {
                assert_eq!(record.id.to_string(), id);
                assert!(record.request_body.is_none());
                assert!(record.response_body.is_none());
            }
            proxy.shutdown().await.unwrap();
        }
    }).await;
}

#[tokio::test]
async fn surge_decoded_request_overflow_rejects_without_forwarding_compressed_bytes() {
    bounded(async {
        let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = origin.local_addr().unwrap();
        let rule = hook(ScriptKind::HttpRequest, "$done({})", 128, false);
        let (mut proxy, _) = start_hooks(vec![rule]).await;
        let compressed = gzip(&vec![b'x'; 4096]);
        assert!(compressed.len() < 128);
        assert!(
            client(&proxy)
                .post(format!("http://{address}/decoded-overflow"))
                .header(CONTENT_ENCODING, "gzip")
                .body(compressed)
                .send()
                .await
                .is_err()
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(20), origin.accept())
                .await
                .is_err()
        );
        proxy.shutdown().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn https_default_port_matches_public_script_urls_in_all_dialects() {
    bounded(async {
        let parent = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let parent_addr = parent.local_addr().unwrap();
        let connections = Arc::new(AtomicUsize::new(0));
        let observed = connections.clone();
        let parent_task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = parent.accept().await.unwrap();
                observed.fetch_add(1, Ordering::SeqCst);
                read_head(&mut socket).await;
                socket
                    .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                    .await
                    .unwrap();
            }
        });
        let pattern =
            crate::Pattern::new(r"^https://script[.]probe[.]invalid(?::8443)?/native[?]q=%2F$")
                .unwrap();
        for dialect in [
            ScriptDialect::Surge,
            ScriptDialect::Loon,
            ScriptDialect::QuantumultX,
            ScriptDialect::Stash,
        ] {
            let source = if dialect == ScriptDialect::QuantumultX {
                "$done({status:'HTTP/1.1 200 OK',body:$argument+':'+$request.url});"
            } else {
                "$done({response:{status:200,body:$argument+':'+$request.url}});"
            };
            let rule = ScriptRule {
                name: "declared-request".into(),
                kind: ScriptKind::HttpRequest,
                pattern: pattern.clone(),
                requires_body: false,
                max_size: 65536,
                source: source.into(),
                argument: Some(format!("{dialect:?}")),
                dialect,
                mode: if dialect == ScriptDialect::QuantumultX {
                    HookMode::Echo
                } else {
                    HookMode::Modify
                },
                enabled: true,
                timeout_ms: None,
                binary: false,
                full_headers: false,
                source_id: "https://assets.example/real-source.js".into(),
                module_id: Some("fixture-module".into()),
            };
            let dir = tempfile::tempdir().unwrap();
            let ca = FileCaStore::new(dir.path()).load_or_generate().unwrap();
            let certificate = reqwest::Certificate::from_pem(ca.cert_pem.as_bytes()).unwrap();
            let recorder = Arc::new(MemoryRecorder::new(8));
            let host = Arc::new(ScriptHost::new(
                Arc::new(MockHttpExecutor::with_responses(vec![])),
                Arc::new(MemoryPersistentStore::new()),
                Arc::new(RecordingNotifier::new()),
            ));
            let hooks = ScriptHookEngine::new(host, ScriptLimits::default(), vec![rule]);
            let mut proxy = MitmProxy::new(
                MitmConfig {
                    upstream: UpstreamProxy::Http { addr: parent_addr },
                    ..Default::default()
                },
                RewriteEngine::default(),
                Some(hooks),
                recorder.clone(),
                ca,
            )
            .start()
            .await
            .unwrap();
            let client = reqwest::Client::builder()
                .no_proxy()
                .proxy(reqwest::Proxy::all(format!("http://{}", proxy.addr)).unwrap())
                .tls_built_in_root_certs(false)
                .add_root_certificate(certificate)
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();
            for authority in ["script.probe.invalid", "script.probe.invalid:8443"] {
                let url = format!("https://{authority}/native?q=%2F");
                let response = client.get(&url).send().await.unwrap();
                assert_eq!(response.status(), 200, "{dialect:?}: {url}");
                assert_eq!(response.text().await.unwrap(), format!("{dialect:?}:{url}"));
            }
            assert_eq!(connections.load(Ordering::SeqCst), 0);
            let recorded = recorder.list();
            assert_eq!(recorded[0].url, "https://script.probe.invalid/native?q=%2F");
            assert_eq!(
                recorded[1].url,
                "https://script.probe.invalid:8443/native?q=%2F"
            );
            let untrusted = reqwest::Client::builder()
                .no_proxy()
                .proxy(reqwest::Proxy::all(format!("http://{}", proxy.addr)).unwrap())
                .tls_built_in_root_certs(false)
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();
            assert!(
                untrusted
                    .get("https://script.probe.invalid/native?q=%2F")
                    .send()
                    .await
                    .is_err()
            );
            proxy.shutdown().await.unwrap();
        }
        parent_task.abort();
    })
    .await;
}
