//! Resource URL, response status, and modification-time behavior.
use super::{
    apply_github_proxy_prefix, fetch_resource_last_modified, fetch_resource_text, is_github_url,
    normalize_resource_url, parse_http_date,
};

/// HTTP-date（IMF-fixdate）解析：已知 RFC 示例 → epoch；`GMT` 命名时区可解析；
/// 非法输入报错。
#[test]
fn parse_http_date_handles_imf_fixdate() {
    assert_eq!(
        parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT").unwrap(),
        784_111_777
    );
    assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT").unwrap(), 0);
    assert!(parse_http_date("not a date").is_err());
}

/// HEAD 智能跳过基础：有 Last-Modified 返回其 epoch；无头返回 None；
/// 非 2xx 返回错误。
#[tokio::test]
async fn fetch_resource_last_modified_reads_header_and_handles_absent() {
    use axum::http::header;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = axum::Router::new()
        .route(
            "/dated",
            axum::routing::head(|| async {
                (
                    [(header::LAST_MODIFIED, "Sun, 06 Nov 1994 08:49:37 GMT")],
                    "",
                )
            }),
        )
        .route("/plain", axum::routing::head(|| async { "" }))
        .route(
            "/missing",
            axum::routing::head(|| async { axum::http::StatusCode::NOT_FOUND }),
        );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let dir = tempfile::tempdir().unwrap();
    let timeout = std::time::Duration::from_secs(5);

    let dated = fetch_resource_last_modified(dir.path(), &format!("http://{addr}/dated"), timeout)
        .await
        .unwrap();
    assert_eq!(dated, Some(784_111_777));

    let plain = fetch_resource_last_modified(dir.path(), &format!("http://{addr}/plain"), timeout)
        .await
        .unwrap();
    assert_eq!(plain, None, "无 Last-Modified 头应为 None");

    let err = fetch_resource_last_modified(dir.path(), &format!("http://{addr}/missing"), timeout)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("HTTP 404"), "{err}");
}

#[test]
fn is_github_url_matches_known_github_hosts() {
    assert!(is_github_url(
        "https://raw.githubusercontent.com/owner/repo/main/x.js"
    ));
    assert!(is_github_url("https://github.com/owner/repo"));
    assert!(is_github_url(
        "https://api.github.com/repos/owner/repo/releases"
    ));
    assert!(is_github_url("http://www.github.com/owner/repo"));
    assert!(is_github_url("https://gist.github.com/owner/abc123"));
    assert!(is_github_url(
        "https://gist.githubusercontent.com/owner/abc123/raw/x.js"
    ));
    assert!(is_github_url(
        "https://codeload.github.com/owner/repo/zip/main"
    ));
    assert!(is_github_url(
        "https://objects.githubusercontent.com/github-production/…"
    ));
    // 带端口 / query 仍识别 host。
    assert!(is_github_url(
        "https://raw.githubusercontent.com:443/o/r/main/x.js"
    ));
    // 非 GitHub 域名 / 无 scheme 判定为 false。
    assert!(!is_github_url("https://example.com/raw/x.js"));
    assert!(!is_github_url("https://github.com.evil.com/x"));
    assert!(!is_github_url("github.com/owner/repo"));
    assert!(!is_github_url(""));
}

#[test]
fn apply_github_proxy_prefix_only_wraps_github_urls() {
    let prefix = "https://gh-proxy.com";
    assert_eq!(
        apply_github_proxy_prefix("https://raw.githubusercontent.com/o/r/main/x.js", prefix),
        "https://gh-proxy.com/https://raw.githubusercontent.com/o/r/main/x.js"
    );
    // prefix 尾部斜杠去重。
    assert_eq!(
        apply_github_proxy_prefix("https://github.com/o/r", "https://gh-proxy.com/"),
        "https://gh-proxy.com/https://github.com/o/r"
    );
    // api.github.com 同样走代理前缀（核心版本查询/下载共用此策略）。
    assert_eq!(
        apply_github_proxy_prefix("https://api.github.com/repos/o/r/releases", prefix),
        "https://gh-proxy.com/https://api.github.com/repos/o/r/releases"
    );
    // 非 GitHub URL 原样返回。
    assert_eq!(
        apply_github_proxy_prefix("https://example.com/x.js", prefix),
        "https://example.com/x.js"
    );
    // 空 prefix 原样返回。
    assert_eq!(
        apply_github_proxy_prefix("https://github.com/o/r", ""),
        "https://github.com/o/r"
    );
    assert_eq!(
        apply_github_proxy_prefix("https://github.com/o/r", "   "),
        "https://github.com/o/r"
    );
}

/// 本地 axum 服务测试 fetch_resource_text：200 成功 / 404 失败（非 GitHub URL，
/// 不拼接代理前缀；默认配置直连）。
#[tokio::test]
async fn fetch_resource_text_succeeds_on_local_200_and_fails_on_404() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = axum::Router::new()
        .route("/ok", axum::routing::get(|| async { "hello" }))
        .route(
            "/missing",
            axum::routing::get(|| async { axum::http::StatusCode::NOT_FOUND }),
        );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let dir = tempfile::tempdir().unwrap();
    let timeout = std::time::Duration::from_secs(5);

    let ok = fetch_resource_text(dir.path(), &format!("http://{addr}/ok"), timeout)
        .await
        .unwrap();
    assert_eq!(ok, "hello");

    let err = fetch_resource_text(dir.path(), &format!("http://{addr}/missing"), timeout)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("HTTP 404"), "{err}");
}

#[test]
fn normalizes_github_blob_and_raw_to_raw_githubusercontent() {
    assert_eq!(
        normalize_resource_url(
            "https://github.com/SagerNet/sing-box/blob/main/transport/splithttp/client.go"
        ),
        "https://raw.githubusercontent.com/SagerNet/sing-box/main/transport/splithttp/client.go"
    );
    assert_eq!(
        normalize_resource_url("https://github.com/owner/repo/raw/main/sub/module.sgmodule"),
        "https://raw.githubusercontent.com/owner/repo/main/sub/module.sgmodule"
    );
}

#[test]
fn leaves_already_raw_and_non_github_urls_unchanged() {
    assert_eq!(
        normalize_resource_url("https://raw.githubusercontent.com/o/r/main/x.js"),
        "https://raw.githubusercontent.com/o/r/main/x.js"
    );
    assert_eq!(
        normalize_resource_url("https://example.com/not/github/blob/main/x.js"),
        "https://example.com/not/github/blob/main/x.js"
    );
    assert_eq!(
        normalize_resource_url("http://github.com/o/r/tree/main/x.js"),
        "http://github.com/o/r/tree/main/x.js"
    );
    // 结构不完整（缺 branch / path）时原样返回。
    assert_eq!(
        normalize_resource_url("https://github.com/owner/repo"),
        "https://github.com/owner/repo"
    );
}
