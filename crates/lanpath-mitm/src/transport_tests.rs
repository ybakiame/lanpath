//! Consumer-visible transport regressions; all endpoints and parents are loopback fixtures.
use crate::{
    CaStore, FileCaStore, HostnameMatcher, MemoryRecorder, MitmConfig, MitmProxy, Phase,
    RewriteEngine, RewriteKind, RewriteRule, RunningProxy, TrafficRecorder, UpstreamProxy,
    transport_body::{self, Inspected},
};
use bytes::Bytes;
use futures::{SinkExt, StreamExt, stream};
use http::{
    HeaderMap, HeaderValue,
    header::{CONTENT_ENCODING, CONTENT_LENGTH, TRANSFER_ENCODING},
};
use http_body::Frame;
use http_body_util::{BodyExt, StreamBody};
use hudsucker::{
    Body,
    tokio_tungstenite::{
        self,
        tungstenite::{Message, client::IntoClientRequest},
    },
};
use std::{
    future::Future,
    io::Write,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};

async fn bounded<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(10), future)
        .await
        .expect("loopback fixture timed out")
}
async fn read_head(socket: &mut (impl tokio::io::AsyncRead + Unpin)) -> Vec<u8> {
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        head.push(socket.read_u8().await.unwrap());
        assert!(head.len() < 65536);
    }
    head
}
async fn start(config: MitmConfig, rules: Vec<RewriteRule>) -> (RunningProxy, Arc<MemoryRecorder>) {
    let dir = tempfile::tempdir().unwrap();
    let ca = FileCaStore::new(dir.path()).load_or_generate().unwrap();
    let recorder = Arc::new(MemoryRecorder::new(8));
    let running = MitmProxy::new(config, RewriteEngine { rules }, None, recorder.clone(), ca)
        .start()
        .await
        .unwrap();
    (running, recorder)
}
fn rule(kind: RewriteKind) -> RewriteRule {
    RewriteRule {
        kind,
        pattern: crate::Pattern::new(".*").unwrap(),
    }
}
fn client(proxy: &RunningProxy) -> reqwest::Client {
    reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(format!("http://{}", proxy.addr)).unwrap())
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}
fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}
async fn response_origin(
    body: Vec<u8>,
    extra: &'static str,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_head(&mut socket).await;
        socket
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        socket.write_all(&body).await.unwrap();
    });
    (addr, task)
}

#[tokio::test]
async fn unmatched_binary_request_and_response_are_exactly_eight_bytes() {
    bounded(async {
        let bytes = [0, 0xff, 0x80, 0xfe, b'A', b'\n', 0, 0x81];
        let origin = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = origin.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = origin.accept().await.unwrap();
            let head = read_head(&mut socket).await;
            assert!(head.windows(7).any(|v| v == b"x-raw: "));
            assert!(head.contains(&0x80));
            let mut received = [0u8; 8];
            socket.read_exact(&mut received).await.unwrap();
            assert_eq!(received, bytes);
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\nX-Raw: \x80\r\nConnection: close\r\n\r\n").await.unwrap();
            socket.write_all(&bytes).await.unwrap();
        });
        let (mut proxy, recorder) = start(MitmConfig::default(), vec![]).await;
        let response = client(&proxy).post(format!("http://{addr}/bytes"))
            .header("x-raw", HeaderValue::from_bytes(b"\x80").unwrap()).body(bytes.to_vec()).send().await.unwrap();
        assert_eq!(response.headers().get_all("set-cookie").iter().count(), 2);
        assert_eq!(response.headers()["x-raw"].as_bytes(), b"\x80");
        assert_eq!(response.bytes().await.unwrap().as_ref(), &bytes);
        server.await.unwrap();
        let records = recorder.list();
        assert_eq!(records.len(), 1);
        assert!(records[0].request_body.is_none());
        assert!(records[0].response_body.is_none());
        proxy.shutdown().await.unwrap();
    }).await;
}

#[tokio::test]
async fn selected_compressed_noop_keeps_wire_bytes_but_changed_body_reframes() {
    bounded(async {
        for (pattern, expected) in [
            ("absent", b"original".as_slice()),
            ("original", b"rewritten".as_slice()),
        ] {
            let compressed = gzip(b"original");
            let (origin, server) = response_origin(
                compressed.clone(),
                "Content-Encoding: gzip\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\n",
            )
            .await;
            let (mut proxy, recorder) = start(
                MitmConfig::default(),
                vec![rule(RewriteKind::BodyRewrite {
                    phase: Phase::Response,
                    body_pattern: Some(crate::Pattern::new(pattern).unwrap()),
                    replacement: "rewritten".into(),
                })],
            )
            .await;
            let response = client(&proxy)
                .get(format!("http://{origin}/body"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.headers().get_all("set-cookie").iter().count(), 2);
            if pattern == "absent" {
                assert_eq!(response.headers()[CONTENT_ENCODING], "gzip");
                assert_eq!(
                    response.bytes().await.unwrap().as_ref(),
                    compressed.as_slice()
                );
            } else {
                assert!(!response.headers().contains_key(CONTENT_ENCODING));
                assert_eq!(response.bytes().await.unwrap().as_ref(), expected);
            }
            server.await.unwrap();
            assert_eq!(
                recorder.list()[0].response_body.as_deref(),
                std::str::from_utf8(expected).ok()
            );
            proxy.shutdown().await.unwrap();
        }
    })
    .await;
}

#[tokio::test]
async fn decoded_bomb_is_not_rewritten_or_recorded_as_complete_text() {
    bounded(async {
        let compressed = gzip(&vec![b'x'; 4096]);
        assert!(compressed.len() < 128);
        let (origin, server) =
            response_origin(compressed.clone(), "Content-Encoding: gzip\r\n").await;
        let config = MitmConfig {
            max_body_size: 128,
            ..MitmConfig::default()
        };
        let (mut proxy, recorder) = start(
            config,
            vec![rule(RewriteKind::BodyRewrite {
                phase: Phase::Response,
                body_pattern: Some(crate::Pattern::new("x").unwrap()),
                replacement: "y".into(),
            })],
        )
        .await;
        let response = client(&proxy)
            .get(format!("http://{origin}/bomb"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.headers()[CONTENT_ENCODING], "gzip");
        assert_eq!(
            response.bytes().await.unwrap().as_ref(),
            compressed.as_slice()
        );
        server.await.unwrap();
        assert!(recorder.list()[0].response_body.is_none());
        proxy.shutdown().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn header_only_rewrite_streams_first_chunk_before_origin_finishes() {
    bounded(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (release, released) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            read_head(&mut socket).await;
            socket.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nfirst\r\n").await.unwrap();
            released.await.unwrap();
            socket.write_all(b"6\r\nsecond\r\n0\r\n\r\n").await.unwrap();
        });
        let (mut proxy, recorder) = start(MitmConfig::default(), vec![rule(RewriteKind::HeaderAppend {
            phase: Phase::Response, name: "X-Stream".into(), value: "yes".into(),
        })]).await;
        let mut response = client(&proxy).get(format!("http://{addr}/stream")).send().await.unwrap();
        assert_eq!(response.headers()["x-stream"], "yes");
        assert_eq!(response.chunk().await.unwrap().unwrap().as_ref(), b"first");
        release.send(()).unwrap();
        assert_eq!(response.bytes().await.unwrap().as_ref(), b"second");
        server.await.unwrap();
        assert_eq!(recorder.list()[0].response_body.as_deref(), Some("firstsecond"));
        proxy.shutdown().await.unwrap();
    }).await;
}

#[tokio::test]
async fn selected_frames_preserve_errors_trailers_and_oversized_remainder() {
    let mut trailers = HeaderMap::new();
    trailers.append("x-checksum", HeaderValue::from_static("one"));
    trailers.append("x-checksum", HeaderValue::from_static("two"));
    let frames = vec![
        Ok(Frame::data(Bytes::from_static(b"prefix"))),
        Err(hudsucker::Error::Io(std::io::Error::other("origin failed"))),
        Ok(Frame::trailers(trailers.clone())),
    ];
    let body = Body::from(StreamBody::new(stream::iter(frames)));
    let inspected = Inspected::read(body, 100, &HeaderMap::new()).await;
    assert!(inspected.bytes.is_none());
    let capture = Arc::new(Mutex::new(Default::default()));
    let mut outgoing = transport_body::observe(
        inspected.finish(None, &mut HeaderMap::new()),
        capture.clone(),
        100,
        None,
    );
    assert_eq!(
        outgoing
            .frame()
            .await
            .unwrap()
            .unwrap()
            .into_data()
            .unwrap(),
        "prefix"
    );
    assert!(
        matches!(outgoing.frame().await.unwrap(), Err(hudsucker::Error::Io(e)) if e.to_string() == "origin failed")
    );
    assert_eq!(
        outgoing
            .frame()
            .await
            .unwrap()
            .unwrap()
            .into_trailers()
            .unwrap(),
        trailers
    );
    assert!(outgoing.frame().await.is_none());
    assert!(transport_body::recorded(&capture, &HeaderMap::new(), 100).is_none());

    let body = Body::from(StreamBody::new(stream::iter(vec![
        Ok::<_, hudsucker::Error>(Frame::data(Bytes::from_static(b"123"))),
        Ok(Frame::data(Bytes::from_static(b"456789"))),
        Ok(Frame::trailers(trailers.clone())),
    ])));
    let inspected = Inspected::read(body, 4, &HeaderMap::new()).await;
    assert!(inspected.bytes.is_none());
    let collected = inspected
        .finish(None, &mut HeaderMap::new())
        .collect()
        .await
        .unwrap();
    assert_eq!(collected.trailers(), Some(&trailers));
    assert_eq!(collected.to_bytes(), "123456789");
}

#[tokio::test]
async fn replacement_removes_stale_framing_and_encoding_but_keeps_trailers() {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_ENCODING, HeaderValue::from_static("gzip"));
    headers.insert(CONTENT_LENGTH, HeaderValue::from_static("999"));
    headers.insert(TRANSFER_ENCODING, HeaderValue::from_static("chunked"));
    let mut trailers = HeaderMap::new();
    trailers.insert("x-done", HeaderValue::from_static("yes"));
    let frames = vec![
        Ok::<_, hudsucker::Error>(Frame::data(Bytes::from(gzip(b"before")))),
        Ok(Frame::trailers(trailers.clone())),
    ];
    let inspected = Inspected::read(
        Body::from(StreamBody::new(stream::iter(frames))),
        1024,
        &headers,
    )
    .await;
    assert_eq!(inspected.bytes.as_deref(), Some(b"before".as_slice()));
    let body = inspected
        .finish(Some(b"after and longer".to_vec()), &mut headers)
        .collect()
        .await
        .unwrap();
    assert!(!headers.contains_key(CONTENT_ENCODING));
    assert!(!headers.contains_key(CONTENT_LENGTH));
    assert!(!headers.contains_key(TRANSFER_ENCODING));
    assert_eq!(body.trailers(), Some(&trailers));
    assert_eq!(body.to_bytes(), "after and longer");
}

// A parent maps an intentionally unresolvable target to a loopback origin. Direct bypass
// cannot pass these tests even on machines that happen to have a working default route.
async fn parent(origin: SocketAddr) -> (SocketAddr, tokio::task::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut client, _) = listener.accept().await.unwrap();
        let head = String::from_utf8(read_head(&mut client).await).unwrap();
        assert!(head.starts_with("CONNECT "));
        let mut server = TcpStream::connect(origin).await.unwrap();
        client
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .await
            .unwrap();
        tokio::io::copy_bidirectional(&mut client, &mut server)
            .await
            .unwrap();
        head.lines().next().unwrap().to_owned()
    });
    (addr, task)
}
async fn tunnel(proxy: &RunningProxy, target: &str) -> TcpStream {
    let mut stream = TcpStream::connect(proxy.addr).await.unwrap();
    stream
        .write_all(format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let head = read_head(&mut stream).await;
    assert!(head.starts_with(b"HTTP/1.1 200"));
    stream
}

#[tokio::test]
async fn excluded_connect_uses_parent_and_allows_server_first_binary() {
    bounded(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            stream.write_all(b"\0\xff\x80hello").await.unwrap();
        });
        let (parent_addr, parent_task) = parent(addr).await;
        let target = "excluded.invalid:8443";
        let config = MitmConfig {
            upstream: UpstreamProxy::Http { addr: parent_addr },
            excluded_hostnames: vec![HostnameMatcher::from_pattern(target)],
            ..MitmConfig::default()
        };
        let (mut proxy, _) = start(config, vec![]).await;
        let mut stream = tunnel(&proxy, target).await;
        let mut bytes = [0u8; 8];
        stream.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes, b"\0\xff\x80hello");
        drop(stream);
        server.await.unwrap();
        assert_eq!(
            parent_task.await.unwrap(),
            format!("CONNECT {target} HTTP/1.1")
        );
        proxy.shutdown().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn websocket_binary_frames_use_the_configured_parent_tcp() {
    bounded(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let message = socket.next().await.unwrap().unwrap();
            assert_eq!(
                message,
                Message::Binary(Bytes::from_static(b"\0\xff\x80hello"))
            );
            socket.send(message).await.unwrap();
            socket.close(None).await.unwrap();
        });
        let (parent_addr, parent_task) = parent(addr).await;
        let config = MitmConfig {
            upstream: UpstreamProxy::Http { addr: parent_addr },
            ..MitmConfig::default()
        };
        let (mut proxy, _) = start(config, vec![]).await;
        let request = "ws://websocket.invalid:8123/echo"
            .into_client_request()
            .unwrap();
        let stream = tunnel(&proxy, "websocket.invalid:8123").await;
        let (mut socket, _) = tokio_tungstenite::client_async(request, stream)
            .await
            .unwrap();
        socket
            .send(Message::Binary(Bytes::from_static(b"\0\xff\x80hello")))
            .await
            .unwrap();
        assert_eq!(
            socket.next().await.unwrap().unwrap(),
            Message::Binary(Bytes::from_static(b"\0\xff\x80hello"))
        );
        let _ = socket.close(None).await;
        drop(socket);
        server.await.unwrap();
        assert_eq!(
            parent_task.await.unwrap(),
            "CONNECT websocket.invalid:8123 HTTP/1.1"
        );
        proxy.shutdown().await.unwrap();
    })
    .await;
}

fn private_tls_client(
    cert: rustls::pki_types::CertificateDer<'static>,
) -> tokio_rustls::TlsConnector {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert).unwrap();
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    tokio_rustls::TlsConnector::from(Arc::new(config))
}

#[tokio::test]
async fn ip_no_sni_tls_intercepts_exact_connect_authority_with_private_ca() {
    bounded(async {
        let pattern =
            crate::Pattern::new(r"^https://(127[.]0[.]0[.]1|\[::1\]):8443/no-sni[?]q=%2F$")
                .unwrap();
        for (allowed, target, address) in [
            ("127.0.0.1", "127.0.0.1:8443", "127.0.0.1"),
            ("127.0.0.1:8443", "127.0.0.1:8443", "127.0.0.1"),
            ("[::1]", "[::1]:8443", "::1"),
            ("[::1]:8443", "[::1]:8443", "::1"),
        ] {
            let parent = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let parent_addr = parent.local_addr().unwrap();
            let parent_task = tokio::spawn(async move {
                let (mut stream, _) = parent.accept().await.unwrap();
                read_head(&mut stream).await;
                stream
                    .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                    .await
                    .unwrap();
            });
            let dir = tempfile::tempdir().unwrap();
            let ca = FileCaStore::new(dir.path()).load_or_generate().unwrap();
            let cert = rustls_pemfile::certs(&mut ca.cert_pem.as_bytes())
                .next()
                .unwrap()
                .unwrap();
            let recorder = Arc::new(MemoryRecorder::new(8));
            let mut proxy = MitmProxy::new(
                MitmConfig {
                    upstream: UpstreamProxy::Http { addr: parent_addr },
                    hostnames: vec![HostnameMatcher::from_pattern(allowed)],
                    ..MitmConfig::default()
                },
                RewriteEngine {
                    rules: vec![RewriteRule {
                        pattern: pattern.clone(),
                        kind: RewriteKind::Mock {
                            status: 200,
                            body: b"ip-no-sni-intercepted".to_vec(),
                            headers: vec![("X-Intercepted".into(), "yes".into())],
                        },
                    }],
                },
                None,
                recorder.clone(),
                ca,
            )
            .start()
            .await
            .unwrap();
            let stream = tunnel(&proxy, target).await;
            // Rustls sends no SNI for an IP ServerName, but still verifies the IP SAN.
            let name =
                rustls::pki_types::ServerName::from(address.parse::<std::net::IpAddr>().unwrap());
            assert!(matches!(name, rustls::pki_types::ServerName::IpAddress(_)));
            let mut stream = private_tls_client(cert)
                .connect(name, stream)
                .await
                .unwrap();
            stream
                .write_all(
                    format!(
                        "GET /no-sni?q=%2F HTTP/1.1\r\nHost: {target}\r\nConnection: close\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            let head = String::from_utf8(read_head(&mut stream).await).unwrap();
            assert!(head.starts_with("HTTP/1.1 200"), "{allowed}: {head}");
            assert!(head.to_ascii_lowercase().contains("x-intercepted: yes"));
            let mut body = [0; b"ip-no-sni-intercepted".len()];
            stream.read_exact(&mut body).await.unwrap();
            assert_eq!(&body, b"ip-no-sni-intercepted", "{allowed}");
            drop(stream);
            let records = recorder.list();
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].url, format!("https://{target}/no-sni?q=%2F"));
            assert_eq!(records[0].response_status, 200);
            proxy.shutdown().await.unwrap();
            parent_task.abort();
        }
    })
    .await;
}

#[tokio::test]
async fn ip_connect_tls_respects_port_ip_exclusion_and_actual_sni() {
    bounded(async {
        for case in ["wrong-port", "excluded-ip", "excluded-sni", "unlisted-sni"] {
            let sni = match case {
                "excluded-sni" => "excluded.invalid",
                "unlisted-sni" => "other.invalid",
                _ => "127.0.0.1",
            };
            let identity = rcgen::generate_simple_self_signed(vec![sni.into()]).unwrap();
            let cert = identity.cert.der().clone();
            let key = rustls::pki_types::PrivatePkcs8KeyDer::from(
                identity.signing_key.serialize_der(),
            );
            let config = rustls::ServerConfig::builder_with_provider(Arc::new(
                rustls::crypto::aws_lc_rs::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![cert.clone()], key.into())
            .unwrap();
            let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let origin = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut stream = acceptor.accept(stream).await.unwrap();
                let head = String::from_utf8(read_head(&mut stream).await).unwrap();
                assert!(head.starts_with("GET /no-sni HTTP/1.1"));
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\noriginal")
                    .await
                    .unwrap();
                stream.shutdown().await.unwrap();
            });
            let (parent_addr, parent_task) = parent(origin).await;
            let target = "127.0.0.1:8443";
            let allowed = if case == "wrong-port" {
                "127.0.0.1:9443"
            } else {
                target
            };
            let excluded_hostnames = match case {
                "excluded-ip" => vec![HostnameMatcher::from_pattern(target)],
                "excluded-sni" => vec![HostnameMatcher::from_pattern("excluded.invalid:8443")],
                _ => vec![],
            };
            let (mut proxy, recorder) = start(
                MitmConfig {
                    upstream: UpstreamProxy::Http { addr: parent_addr },
                    hostnames: vec![HostnameMatcher::from_pattern(allowed)],
                    excluded_hostnames,
                    ..MitmConfig::default()
                },
                vec![rule(RewriteKind::Mock {
                    status: 200,
                    body: b"must-not-intercept".to_vec(),
                    headers: vec![],
                })],
            )
            .await;
            let stream = tunnel(&proxy, target).await;
            let name = rustls::pki_types::ServerName::try_from(sni).unwrap();
            let mut stream = private_tls_client(cert).connect(name, stream).await.unwrap();
            stream
                .write_all(b"GET /no-sni HTTP/1.1\r\nHost: 127.0.0.1:8443\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
            let head = read_head(&mut stream).await;
            assert!(head.starts_with(b"HTTP/1.1 200"), "{case}");
            let mut body = [0; 8];
            stream.read_exact(&mut body).await.unwrap();
            assert_eq!(&body, b"original", "{case}");
            stream.shutdown().await.unwrap();
            drop(stream);
            server.await.unwrap();
            assert_eq!(parent_task.await.unwrap(), format!("CONNECT {target} HTTP/1.1"));
            assert!(recorder.list().is_empty(), "{case}");
            proxy.shutdown().await.unwrap();
        }
    })
    .await;
}

#[tokio::test]
async fn sni_exclusion_replays_client_hello_through_parent_with_real_tls_validation() {
    bounded(async {
        let identity = rcgen::generate_simple_self_signed(vec!["excluded.invalid".into()]).unwrap();
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let cert = identity.cert.der().clone();
        let key = rustls::pki_types::PrivatePkcs8KeyDer::from(identity.signing_key.serialize_der());
        let config = rustls::ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![cert.clone()], key.into())
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = acceptor.accept(stream).await.unwrap();
            stream.write_all(b"verified").await.unwrap();
            stream.shutdown().await.unwrap();
        });
        let (parent_addr, parent_task) = parent(addr).await;
        let config = MitmConfig {
            upstream: UpstreamProxy::Http { addr: parent_addr },
            excluded_hostnames: vec![HostnameMatcher::from_pattern("excluded.invalid:8443")],
            ..MitmConfig::default()
        };
        let (mut proxy, _) = start(config, vec![]).await;
        // CONNECT authority is allowed; only the actual ClientHello SNI excludes it.
        let stream = tunnel(&proxy, "allowed.invalid:8443").await;
        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert).unwrap();
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        let mut stream = connector
            .connect(
                rustls::pki_types::ServerName::try_from("excluded.invalid").unwrap(),
                stream,
            )
            .await
            .unwrap();
        let mut received = [0u8; 8];
        stream.read_exact(&mut received).await.unwrap();
        assert_eq!(&received, b"verified");
        stream.shutdown().await.unwrap();
        drop(stream);
        server.await.unwrap();
        assert_eq!(
            parent_task.await.unwrap(),
            "CONNECT allowed.invalid:8443 HTTP/1.1"
        );
        proxy.shutdown().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn truncated_origin_body_is_a_client_error_not_an_empty_success() {
    bounded(async {
        let body_pattern = crate::Pattern::new("short").unwrap();
        for selected in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                read_head(&mut stream).await;
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 99\r\nConnection: close\r\n\r\nshort",
                    )
                    .await
                    .unwrap();
            });
            let rules = if selected {
                vec![rule(RewriteKind::BodyRewrite {
                    phase: Phase::Response,
                    body_pattern: Some(body_pattern.clone()),
                    replacement: "must not hide an error".into(),
                })]
            } else {
                vec![]
            };
            let (mut proxy, recorder) = start(MitmConfig::default(), rules).await;
            if let Ok(response) = client(&proxy)
                .get(format!("http://{addr}/broken"))
                .send()
                .await
            {
                assert!(
                    response.bytes().await.is_err(),
                    "truncated upstream became successful body"
                );
            }
            server.await.unwrap();
            assert!(
                recorder
                    .list()
                    .iter()
                    .all(|record| record.response_body.is_none())
            );
            proxy.shutdown().await.unwrap();
        }
    })
    .await;
}

#[tokio::test]
async fn selected_noop_forwards_real_http_trailers() {
    bounded(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_head(&mut stream).await;
            stream.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nTrailer: x-checksum\r\nConnection: close\r\n\r\n4\r\ndata\r\n0\r\nx-checksum: final\r\n\r\n").await.unwrap();
        });
        let (mut proxy, _) = start(MitmConfig::default(), vec![rule(RewriteKind::BodyRewrite {
            phase: Phase::Response, body_pattern: Some(crate::Pattern::new("absent").unwrap()),
            replacement: "unchanged".into(),
        })]).await;
        let mut stream = TcpStream::connect(proxy.addr).await.unwrap();
        stream.write_all(format!("GET http://{addr}/trailers HTTP/1.1\r\nHost: {addr}\r\nTE: trailers\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response = String::from_utf8(response).unwrap().to_ascii_lowercase();
        assert!(response.starts_with("http/1.1 200"));
        assert!(response.contains("4\r\ndata\r\n0\r\nx-checksum: final\r\n\r\n"), "{response:?}");
        server.await.unwrap();
        proxy.shutdown().await.unwrap();
    }).await;
}

#[tokio::test]
async fn mock_binary_response_keeps_duplicate_headers_and_discards_stale_length() {
    bounded(async {
        let bytes = b"\0\xff\x80hello";
        let (mut proxy, _) = start(
            MitmConfig::default(),
            vec![rule(RewriteKind::Mock {
                status: 201,
                body: bytes.to_vec(),
                headers: vec![
                    ("Set-Cookie".into(), "a=1".into()),
                    ("Set-Cookie".into(), "b=2".into()),
                    ("Content-Length".into(), "999".into()),
                    ("Transfer-Encoding".into(), "chunked".into()),
                ],
            })],
        )
        .await;
        // No origin or parent exists: mock completion must not establish a transport.
        let response = client(&proxy)
            .get("http://mock.invalid/body")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 201);
        assert_eq!(response.headers().get_all("set-cookie").iter().count(), 2);
        assert_eq!(response.content_length(), Some(8));
        assert_eq!(response.bytes().await.unwrap().as_ref(), bytes);
        proxy.shutdown().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn shutdown_closes_an_active_excluded_connect_and_its_origin_socket() {
    bounded(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (active, ready) = oneshot::channel();
        let origin = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4];
            socket.read_exact(&mut request).await.unwrap();
            assert_eq!(&request, b"open");
            active.send(()).unwrap();
            let mut byte = [0];
            socket.read(&mut byte).await
        });
        let (mut proxy, _) = start(MitmConfig::default(), vec![]).await;
        let proxy_addr = proxy.addr;
        let mut client = TcpStream::connect(proxy_addr).await.unwrap();
        client
            .write_all(format!("CONNECT {addr} HTTP/1.1\r\nHost: {addr}\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let head = read_head(&mut client).await;
        assert!(head.starts_with(b"HTTP/1.1 200"));
        client.write_all(b"open").await.unwrap();
        ready.await.unwrap();
        proxy.shutdown().await.unwrap();
        let mut byte = [0];
        let result = tokio::time::timeout(Duration::from_secs(2), client.read(&mut byte))
            .await
            .expect("stopped proxy retained an active CONNECT tunnel");
        assert_closed(result);
        assert_closed(origin.await.unwrap());
        assert!(TcpStream::connect(proxy_addr).await.is_err());
    })
    .await;
}

fn assert_closed(result: std::io::Result<usize>) {
    match result {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
            ) => {}
        other => panic!("expected a closed owned socket, got {other:?}"),
    }
}

#[path = "script_transport_tests.rs"]
mod script_contracts;
