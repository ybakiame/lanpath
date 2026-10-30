use super::*;
use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Response, StatusCode},
    routing::get,
};
use std::sync::atomic::AtomicUsize;

fn executor(mode: RequestProxyMode) -> (tempfile::TempDir, Arc<ReqwestHttpExecutor>) {
    let root = tempfile::tempdir().unwrap();
    let mut config = ClientConfig::new(
        root.path().to_owned(),
        String::new(),
        String::new(),
        PathBuf::new(),
    );
    config.request_proxy_mode = mode;
    config.save().unwrap();
    let executor = Arc::new(ReqwestHttpExecutor::for_data_dir(root.path()));
    (root, executor)
}

fn request(url: String, auto_cookie: bool) -> HttpRequestSpec {
    HttpRequestSpec {
        url,
        method: "GET".into(),
        auto_cookie,
        ..Default::default()
    }
}

async fn serve(app: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (url, task)
}

#[tokio::test]
async fn binary_and_duplicate_headers_survive_with_run_scoped_cookie_privacy() {
    let binary = vec![137, 80, 78, 71, 0, 255, 128, 10];
    let content = binary.clone();
    let app = Router::new()
        .route(
            "/binary",
            get(move || {
                let content = content.clone();
                async move {
                    Response::builder()
                        .header("set-cookie", "one=1; Path=/")
                        .header("set-cookie", "two=2; Path=/")
                        .header(
                            "x-raw",
                            axum::http::HeaderValue::from_bytes(b"\x80").unwrap(),
                        )
                        .body(Body::from(content))
                        .unwrap()
                }
            }),
        )
        .route(
            "/cookie",
            get(|headers: HeaderMap| async move {
                headers
                    .get("cookie")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("")
                    .to_owned()
            }),
        )
        .route(
            "/header",
            get(|headers: HeaderMap| async move {
                if headers
                    .get("x-roundtrip")
                    .is_some_and(|value| value.as_bytes() == b"\x80")
                {
                    (StatusCode::OK, "valid octet")
                } else {
                    (StatusCode::BAD_REQUEST, "incorrect octet")
                }
            }),
        )
        .route(
            "/poison",
            get(|| async { ([("set-cookie", "one=poison; Path=/")], "ignored") }),
        );
    let (url, server) = serve(app).await;
    let (_root, root) = executor(RequestProxyMode::None);
    let host = Arc::new(lanpath_script::ScriptHost::new(
        root,
        Arc::new(lanpath_script::MemoryPersistentStore::new()),
        Arc::new(lanpath_script::RecordingNotifier::new()),
    ));
    let worker = lanpath_script::ScriptWorker::new(host, lanpath_script::ScriptLimits::default());
    let source = r#"
        const get = options => new Promise((resolve, reject) =>
            $httpClient.get(options, (error, response, body) =>
                error ? reject(new Error(error)) : resolve({response, body})));
        (async () => {
            const binary = await get({url: $argument + '/binary',
                'binary-mode': true, 'full-header-mode': true});
            const raw = binary.response.headers.find(h => h.field.toLowerCase() === 'x-raw').value;
            const checked = await get({url: $argument + '/header', headers: {'X-Roundtrip': raw}});
            const before = await get($argument + '/cookie');
            const bypass = await get({url: $argument + '/cookie', 'auto-cookie': false});
            await get({url: $argument + '/poison', 'auto-cookie': false});
            const after = await get($argument + '/cookie');
            $done({bytes: Array.from(binary.body),
                cookieFields: binary.response.headers.filter(h => h.field.toLowerCase() === 'set-cookie').length,
                rawHeader: raw.charCodeAt(0), headerStatus: checked.response.status,
                before: before.body, bypass: bypass.body, after: after.body});
        })();
    "#;
    let result = worker
        .run_script(
            source,
            lanpath_script::ScriptKind::Generic,
            None,
            Some(&url),
            lanpath_script::ScriptDialect::Surge,
            "cookie-boundary",
        )
        .await
        .unwrap();
    assert_eq!(result.0["bytes"], serde_json::json!(binary));
    assert_eq!(result.0["cookieFields"], 2);
    assert_eq!(result.0["rawHeader"], 128);
    assert_eq!(result.0["headerStatus"], 200);
    let before = result.0["before"].as_str().unwrap();
    assert!(before.contains("one=1") && before.contains("two=2"));
    assert_eq!(result.0["bypass"], "");
    assert_eq!(result.0["after"], before);
    let next = worker.run_script(
        "$httpClient.get($argument + '/cookie', (e,r,body) => { if(e) throw Error(e); $done(body); });",
        lanpath_script::ScriptKind::Generic, None, Some(&url), lanpath_script::ScriptDialect::Surge,
        "cookie-boundary").await.unwrap();
    assert_eq!(next.0, "");
    server.abort();
}

#[tokio::test]
async fn explicit_redirect_disable_does_not_contact_the_destination() {
    let reached = Arc::new(AtomicUsize::new(0));
    let seen = reached.clone();
    let app = Router::new()
        .route(
            "/redirect",
            get(|| async { (StatusCode::FOUND, [("location", "/private")]) }),
        )
        .route(
            "/private",
            get(move || {
                seen.fetch_add(1, Ordering::SeqCst);
                async { "private destination" }
            }),
        );
    let (url, server) = serve(app).await;
    let (_root, root) = executor(RequestProxyMode::None);
    let session = root.session();
    let mut no_redirect = request(format!("{url}/redirect"), false);
    no_redirect.auto_redirect = false;
    assert_eq!(session.execute(no_redirect).await.unwrap().status, 302);
    assert_eq!(reached.load(Ordering::SeqCst), 0);
    assert_eq!(
        session
            .execute(request(format!("{url}/redirect"), false))
            .await
            .unwrap()
            .status,
        200
    );
    assert_eq!(reached.load(Ordering::SeqCst), 1);
    server.abort();
}

#[tokio::test]
async fn unavailable_core_policy_never_falls_back_to_a_reachable_origin() {
    let reached = Arc::new(AtomicUsize::new(0));
    let seen = reached.clone();
    let (url, server) = serve(Router::new().route(
        "/private",
        get(move || {
            seen.fetch_add(1, Ordering::SeqCst);
            async { "must not be contacted directly" }
        }),
    ))
    .await;
    let (_root, root) = executor(RequestProxyMode::Core);
    assert!(
        root.clone()
            .session()
            .execute(request(format!("{url}/private"), false))
            .await
            .is_err()
    );
    let unused = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = unused.local_addr().unwrap().port();
    drop(unused);
    root.set_core_port(Some(port));
    let error = root
        .session()
        .execute(request(
            format!("{url}/private?token=private-marker"),
            false,
        ))
        .await
        .unwrap_err();
    assert!(!error.to_string().contains("private-marker"));
    assert_eq!(reached.load(Ordering::SeqCst), 0);
    server.abort();
}

#[tokio::test]
async fn loon_base64_uploads_decode_binary_without_changing_response_mode() {
    let received = Arc::new(AtomicUsize::new(0));
    let seen = received.clone();
    let app = Router::new().route(
        "/upload",
        axum::routing::post(move |body: bytes::Bytes| {
            let index = seen.fetch_add(1, Ordering::SeqCst);
            async move {
                let expected: &[&[u8]] = &[&[0, 255, 128], &[255], &[251, 255], &[0], &[], b"AP+A"];
                assert_eq!(body.as_ref(), expected[index]);
                (StatusCode::CREATED, "response stays text")
            }
        }),
    );
    let (url, server) = serve(app).await;
    let (_root, root) = executor(RequestProxyMode::None);
    let worker = script_worker(root);
    let result = worker
        .run_script(
            r#"
        const post = options => new Promise((resolve, reject) =>
            $httpClient.post(options, (error, response, data) => {
                if (error) return reject(Error(error));
                if (response.status !== 201 || data !== 'response stays text')
                    return reject(Error('response was decoded or not uploaded'));
                resolve();
            }));
        (async () => {
            for (const body of ['AP+A', '/w==', '+/8=', 'AA==', '']) {
                await post({url:$argument + '/upload', body, 'body-base64':true});
            }
            await post({url:$argument + '/upload', body:'AP+A', 'body-base64':false});
            $done('uploaded');
        })();
    "#,
            lanpath_script::ScriptKind::Generic,
            None,
            Some(&url),
            lanpath_script::ScriptDialect::Loon,
            "loon-base64",
        )
        .await
        .unwrap();
    assert_eq!(result.0, "uploaded");
    assert_eq!(received.load(Ordering::SeqCst), 6);
    worker.shutdown().await;
    server.abort();
}

fn script_worker(root: Arc<ReqwestHttpExecutor>) -> lanpath_script::ScriptWorker {
    let host = Arc::new(lanpath_script::ScriptHost::new(
        root,
        Arc::new(lanpath_script::MemoryPersistentStore::new()),
        Arc::new(lanpath_script::RecordingNotifier::new()),
    ));
    lanpath_script::ScriptWorker::new(host, lanpath_script::ScriptLimits::default())
}

#[tokio::test]
async fn loon_invalid_base64_conflicting_bodies_and_unknown_protocols_never_reach_transport() {
    let received = Arc::new(AtomicUsize::new(0));
    let seen = received.clone();
    let (url, server) = serve(Router::new().route(
        "/",
        axum::routing::post(move || {
            seen.fetch_add(1, Ordering::SeqCst);
            async { "accepted" }
        }),
    ))
    .await;
    let (_root, root) = executor(RequestProxyMode::None);
    let worker = script_worker(root);
    worker
        .run_script(
            r#"
        const post = options => new Promise(resolve =>
            $httpClient.post(Object.assign({url:$argument + '/'}, options),
                (error, response, data) => resolve({error, response, data})));
        (async () => {
            const invalid = [
                {'body-base64':true}, {'body-base64':true,body:null},
                {'body-base64':true,body:new Uint8Array([255])},
                {'body-base64':true,bodyBytes:new Uint8Array([255])},
                {'body-base64':true,body:'AA==',bodyBytes:new Uint8Array([0])},
                {'body-base64':'true',body:'AA=='}, {'body-base64':1,body:'AA=='},
                {'body-base64':null,body:'AA=='},
                ...['%', 'A', 'AA=', 'AA===', 'A===', '=AAA', 'AB==', 'AAB=',
                    'AA==AA==', '_w==', 'AA==\n'].map(body => ({body,'body-base64':true})),
                ...['h3', 'http/1.1', '', 2, null, ['h2']].map(alpn => ({alpn})),
                {'http-alpn':'h2'}, {node:'DIRECT'}, {policy:'DIRECT'}
            ];
            for (const options of invalid) {
                const result = await post(options);
                if (typeof result.error !== 'string' || !result.error ||
                    result.response !== null || result.data !== null)
                    throw Error('invalid request did not use callback error boundary');
            }
            const valid = await post({body:'still live'});
            if (valid.error !== null || valid.response.status !== 200 || valid.data !== 'accepted')
                throw Error('error poisoned subsequent request');
            $done();
        })();
    "#,
            lanpath_script::ScriptKind::Generic,
            None,
            Some(&url),
            lanpath_script::ScriptDialect::Loon,
            "loon-errors",
        )
        .await
        .unwrap();
    assert_eq!(received.load(Ordering::SeqCst), 1);
    worker.shutdown().await;
    server.abort();
}

struct TlsListener {
    tcp: tokio::net::TcpListener,
    acceptor: tokio_rustls::TlsAcceptor,
    negotiated: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl axum::serve::Listener for TlsListener {
    type Io = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;
    type Addr = std::net::SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (tcp, address) = self.tcp.accept().await.unwrap();
            if let Ok(Ok(tls)) =
                tokio::time::timeout(Duration::from_secs(2), self.acceptor.accept(tcp)).await
            {
                self.negotiated
                    .lock()
                    .unwrap()
                    .push(tls.get_ref().1.alpn_protocol().unwrap_or_default().to_vec());
                return (tls, address);
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.tcp.local_addr()
    }
}

async fn serve_tls(
    protocols: &[&[u8]],
    app: Router,
) -> (
    String,
    Arc<Mutex<Vec<Vec<u8>>>>,
    tokio::task::JoinHandle<()>,
) {
    use base64::Engine as _;
    use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
    // Public, test-only self-signed identity. Never installed in an OS trust store.
    const CERT: &str = "MIIBmTCCAT+gAwIBAgIUcfDAdluOynWnvZwqzUrdmZddMBEwCgYIKoZIzj0EAwIwFDESMBAGA1UEAwwJbG9jYWxob3N0MB4XDTI2MTAyNTE2MjI0OFoXDTM2MTAyMjE2MjI0OFowFDESMBAGA1UEAwwJbG9jYWxob3N0MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE44k7xXOcKA5A+XAJ6YDkgvQhfAv4jzYqVG2AV7qB/CqSazxi1FeUWR2SIKPRd2rEVmcqZzt97TdLSCBtQ4fTIaNvMG0wHQYDVR0OBBYEFDKOSdYGSoNm+I+AAaNfwoUNLHVeMB8GA1UdIwQYMBaAFDKOSdYGSoNm+I+AAaNfwoUNLHVeMA8GA1UdEwEB/wQFMAMBAf8wGgYDVR0RBBMwEYIJbG9jYWxob3N0hwR/AAABMAoGCCqGSM49BAMCA0gAMEUCIQCq2heN6DWEkewRHEoebqCySOYBHWgUhpa5heLeDEXurgIgKf3ciLfpHOhpckpJtRs99/VHsioL/OHbPW9wAoVHOfo=";
    const KEY: &str = "MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgpH/OjFKABZsycillG5dXXHlLHvQe82Lql/iKsioe0rShRANCAATjiTvFc5woDkD5cAnpgOSC9CF8C/iPNipUbYBXuoH8KpJrPGLUV5RZHZIgo9F3asRWZypnO33tN0tIIG1Dh9Mh";
    let cert = CertificateDer::from(
        base64::engine::general_purpose::STANDARD
            .decode(CERT)
            .unwrap(),
    );
    let key = PrivatePkcs8KeyDer::from(
        base64::engine::general_purpose::STANDARD
            .decode(KEY)
            .unwrap(),
    );
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![cert], key.into())
    .unwrap();
    config.alpn_protocols = protocols.iter().map(|protocol| protocol.to_vec()).collect();
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("https://{}", tcp.local_addr().unwrap());
    let negotiated = Arc::new(Mutex::new(Vec::new()));
    let listener = TlsListener {
        tcp,
        acceptor: tokio_rustls::TlsAcceptor::from(Arc::new(config)),
        negotiated: negotiated.clone(),
    };
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (url, negotiated, task)
}

#[tokio::test]
async fn loon_worker_tls_alpn_selects_real_h1_h2_and_isolates_cached_tls_policy() {
    let app = Router::new()
        .route(
            "/protocol",
            get(
                |version: axum::http::Version, headers: HeaderMap| async move {
                    let cookie = headers
                        .get("cookie")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("");
                    (
                        [("set-cookie", "shared=yes; Path=/")],
                        format!("{version:?}|{cookie}"),
                    )
                },
            ),
        )
        .route(
            "/redirect",
            get(|| async { (StatusCode::FOUND, [("location", "/protocol")]) }),
        );
    let (url, negotiated, server) = serve_tls(&[b"h2", b"http/1.1"], app).await;
    let (_root, root) = executor(RequestProxyMode::None);
    let worker = script_worker(root);
    let result = worker.run_script(r#"
        const get = options => new Promise(resolve =>
            $httpClient.get(Object.assign({url:$argument + '/protocol'}, options),
                (error, response, body) => resolve({error, response, body})));
        (async () => {
            const h1 = await get({});
            const h2 = await get({alpn:'h2'});
            const bypass = await get({alpn:'h2','auto-cookie':false});
            const h1Again = await get({alpn:'h1'});
            const noRedirect = await get({url:$argument + '/redirect',alpn:'h2','auto-redirect':false});
            const redirect = await get({url:$argument + '/redirect',alpn:'h2'});
            const secureH2 = await get({alpn:'h2',insecure:false});
            const secureH1 = await get({alpn:'h1',insecure:false});
            const recovered = await get({alpn:'h2'});
            for (const result of [h1,h2,bypass,h1Again,noRedirect,redirect,recovered])
                if (result.error) throw Error(result.error);
            if (!secureH2.error || !secureH1.error) throw Error('insecure client reused for trusted TLS request');
            $done({h1:h1.body,h2:h2.body,bypass:bypass.body,h1Again:h1Again.body,
                noRedirect:noRedirect.response.status,redirect:redirect.body,recovered:recovered.body});
        })();
    "#, lanpath_script::ScriptKind::Generic, None, Some(&url),
        lanpath_script::ScriptDialect::Loon, "loon-tls").await.unwrap();
    assert_eq!(
        result.0,
        serde_json::json!({
            "h1":"HTTP/1.1|", "h2":"HTTP/2.0|shared=yes", "bypass":"HTTP/2.0|",
            "h1Again":"HTTP/1.1|shared=yes", "noRedirect":302,
            "redirect":"HTTP/2.0|shared=yes", "recovered":"HTTP/2.0|shared=yes"
        })
    );
    {
        let protocols = negotiated.lock().unwrap();
        assert!(protocols.iter().any(|protocol| protocol == b"h2"));
        assert!(protocols.iter().any(|protocol| protocol == b"http/1.1"));
    }
    worker.shutdown().await;
    server.abort();
}

#[tokio::test]
async fn explicit_tls_protocol_never_downgrades_to_the_other_server_protocol() {
    for (offered, selected) in [
        (b"http/1.1".as_slice(), HttpAlpn::Http2),
        (b"h2".as_slice(), HttpAlpn::Http1),
    ] {
        let contacted = Arc::new(AtomicUsize::new(0));
        let seen = contacted.clone();
        let (url, _, server) = serve_tls(
            &[offered],
            Router::new().route(
                "/",
                get(move || {
                    seen.fetch_add(1, Ordering::SeqCst);
                    async { "wrong protocol" }
                }),
            ),
        )
        .await;
        let (_root, root) = executor(RequestProxyMode::None);
        let mut req = request(format!("{url}/"), false);
        req.insecure = true;
        req.alpn = selected;
        assert!(root.session().execute(req).await.is_err());
        assert_eq!(contacted.load(Ordering::SeqCst), 0);
        server.abort();
    }
}

#[tokio::test]
async fn other_dialects_keep_automatic_tls_negotiation_and_reject_loon_only_options() {
    let (url, negotiated, server) = serve_tls(
        &[b"h2", b"http/1.1"],
        Router::new().route(
            "/",
            get(|version: axum::http::Version| async move { format!("{version:?}") }),
        ),
    )
    .await;
    let callback = r#"
        const get = options => new Promise(resolve =>
            $httpClient.get(Object.assign({url:$argument + '/',insecure:true}, options),
                (error,response,body) => resolve({error,response,body})));
        (async () => {
            for (const options of [{alpn:'h1'},{'body-base64':false}]) {
                const rejected = await get(options);
                if (!rejected.error || rejected.response !== null || rejected.body !== null)
                    throw Error('Loon-only option accepted');
            }
            const result = await get({});
            if (result.error) throw Error(result.error);
            $done(result.body);
        })();
    "#;
    let qx = r#"
        (async () => {
            for (const options of [{alpn:'h1'},{'body-base64':false}]) {
                let rejected = false;
                try { await $task.fetch(Object.assign({url:$argument + '/'},options)); }
                catch (error) { rejected = typeof error.error === 'string'; }
                if (!rejected) throw Error('Loon-only option accepted');
            }
            const result = await $task.fetch({url:$argument + '/',opts:{'skip-cert-verify':true}});
            $done(result.body);
        })();
    "#;
    for dialect in [
        lanpath_script::ScriptDialect::Surge,
        lanpath_script::ScriptDialect::Stash,
        lanpath_script::ScriptDialect::QuantumultX,
    ] {
        let (_root, root) = executor(RequestProxyMode::None);
        let worker = script_worker(root);
        let source = if dialect == lanpath_script::ScriptDialect::QuantumultX {
            qx
        } else {
            callback
        };
        let result = worker
            .run_script(
                source,
                lanpath_script::ScriptKind::Generic,
                None,
                Some(&url),
                dialect,
                "automatic-tls",
            )
            .await
            .unwrap();
        assert_eq!(result.0, "HTTP/2.0");
        worker.shutdown().await;
    }
    assert!(
        negotiated
            .lock()
            .unwrap()
            .iter()
            .all(|protocol| protocol == b"h2")
    );
    server.abort();
}
