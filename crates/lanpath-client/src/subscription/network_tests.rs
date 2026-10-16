use super::*;
use axum::{
    Router,
    http::{HeaderMap, StatusCode, Uri},
    response::IntoResponse,
    routing::get,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

async fn server(app: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, handle)
}

fn session() -> SubscriptionFetcher {
    SubscriptionFetcher::with_client(
        reqwest::Client::builder()
            .no_proxy()
            .cookie_store(true)
            .build()
            .unwrap(),
    )
}

#[tokio::test]
async fn retries_cookie_landing_once_and_keeps_original_subscription() {
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    let app = Router::new().route(
        "/subscription",
        get(move |headers: HeaderMap| {
            let calls = calls.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                assert_eq!(headers["user-agent"], "custom-subscription-client");
                if headers.get("cookie").is_some_and(|v| v == "session=ready") {
                    (
                        [
                            ("content-type", "text/yaml"),
                            ("subscription-userinfo", "total=4096; download=12"),
                        ],
                        "# original\r\nproxies: []\r\n",
                    )
                        .into_response()
                } else {
                    (
                        [
                            ("set-cookie", "session=ready; Path=/; HttpOnly"),
                            ("content-type", "text/html"),
                        ],
                        "<!doctype html><html>session required</html>",
                    )
                        .into_response()
                }
            }
        }),
    );
    let (url, task) = server(app).await;
    let result = session()
        .fetch_with_user_agent(
            &format!("{url}/subscription"),
            Some("custom-subscription-client"),
        )
        .await
        .unwrap();
    assert_eq!(result.source, "# original\r\nproxies: []\r\n");
    assert_eq!(result.format, SubFormat::ClashYaml);
    assert_eq!(result.userinfo.unwrap().total, Some(4096));
    assert_eq!(count.load(Ordering::SeqCst), 2);
    task.abort();
}

#[tokio::test]
async fn repeated_html_challenge_is_bounded_and_not_reported_as_yaml() {
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    let app = Router::new().route(
        "/",
        get(move || {
            calls.fetch_add(1, Ordering::SeqCst);
            async {
                (
                    [("set-cookie", "session=blocked; Path=/")],
                    "<html>still blocked</html>",
                )
            }
        }),
    );
    let (url, task) = server(app).await;
    let error = session().fetch(&url).await.unwrap_err().to_string();
    assert!(error.contains("HTML page"), "{error}");
    assert_eq!(count.load(Ordering::SeqCst), 2);
    task.abort();
}

#[tokio::test]
async fn authorization_error_is_not_retried() {
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    let app = Router::new().route(
        "/",
        get(move || {
            calls.fetch_add(1, Ordering::SeqCst);
            async { (StatusCode::UNAUTHORIZED, "unauthorized") }
        }),
    );
    let (url, task) = server(app).await;
    let error = session().fetch(&url).await.unwrap_err().to_string();
    assert!(error.contains("401"));
    assert_eq!(count.load(Ordering::SeqCst), 1);
    task.abort();
}

#[tokio::test]
async fn redirect_carries_session_cookie() {
    let app = Router::new()
        .route(
            "/start",
            get(|| async {
                (
                    StatusCode::FOUND,
                    [
                        ("location", "/finish"),
                        ("set-cookie", "session=ready; Path=/"),
                    ],
                )
            }),
        )
        .route(
            "/finish",
            get(|headers: HeaderMap| async move {
                assert_eq!(headers["cookie"], "session=ready");
                "{\n  \"outbounds\": []\n}\n"
            }),
        );
    let (url, task) = server(app).await;
    let result = session().fetch(&format!("{url}/start")).await.unwrap();
    assert_eq!(result.source, "{\n  \"outbounds\": []\n}\n");
    assert_eq!(result.format, SubFormat::SingBoxJson);
    task.abort();
}

#[tokio::test]
async fn transport_errors_expose_cause_without_private_url() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let error = session()
        .fetch(&format!(
            "http://127.0.0.1:{port}/private-token?token=private-token"
        ))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("subscription request failed"));
    assert!(!error.contains("private-token"));
    assert!(!error.contains("http://"));
}

#[tokio::test]
async fn configured_proxy_receives_request_and_gzip_response_is_decoded() {
    const COMPRESSED: &[u8] = &[
        31, 139, 8, 0, 0, 0, 0, 0, 2, 255, 83, 86, 200, 47, 202, 76, 207, 204, 75, 204, 225, 229,
        42, 40, 202, 175, 200, 76, 45, 182, 82, 136, 142, 229, 229, 2, 0, 74, 201, 189, 150, 25, 0,
        0, 0,
    ];
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    let app = Router::new().fallback(get(move |headers: HeaderMap, uri: Uri| {
        calls.fetch_add(1, Ordering::SeqCst);
        async move {
            if uri.path().ends_with("/releases") {
                return "[]".into_response();
            }
            assert_eq!(headers["host"], "subscription.invalid");
            ([("content-encoding", "gzip")], COMPRESSED).into_response()
        }
    }));
    let (url, task) = server(app).await;
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = crate::ClientConfig::new(
        dir.path().into(),
        String::new(),
        String::new(),
        std::path::PathBuf::new(),
    );
    cfg.mixed_port = reqwest::Url::parse(&url).unwrap().port().unwrap();
    cfg.request_proxy_url = url;
    for mode in [
        crate::RequestProxyMode::Core,
        crate::RequestProxyMode::Custom,
    ] {
        cfg.request_proxy_mode = mode;
        cfg.save().unwrap();
        let result = fetch_subscription_for_config(
            &cfg,
            "http://subscription.invalid/sub",
            Some("fixture-client"),
        )
        .await
        .unwrap();
        assert_eq!(result.source, "# original\r\nproxies: []\r\n");
        let resource = crate::fetch_resource_bytes(
            dir.path(),
            "http://subscription.invalid/resource",
            std::time::Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert_eq!(resource.as_ref(), result.source.as_bytes());
        let inventory = crate::ClientCoreInventory::with_api_base(
            dir.path().into(),
            "http://subscription.invalid",
        );
        assert!(inventory.list_remote_channels().await.unwrap().is_empty());
    }
    assert_eq!(count.load(Ordering::SeqCst), 6);
    task.abort();
}

#[tokio::test]
async fn none_policy_ignores_custom_proxy_address() {
    let (url, task) = server(Router::new().route("/", get(|| async { "proxies: []" }))).await;
    let cfg = crate::ClientConfig {
        request_proxy_mode: crate::RequestProxyMode::None,
        request_proxy_url: "http://unreachable.invalid:1".into(),
        ..Default::default()
    };
    fetch_subscription_for_config(&cfg, &url, None)
        .await
        .unwrap();
    task.abort();
}
