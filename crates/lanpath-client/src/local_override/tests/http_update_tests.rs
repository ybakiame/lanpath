//! Real local HTTP requests verify origin-bound validators and atomic cache behavior.
use crate::local_override::*;
use axum::{
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn remote(url: String) -> CustomRuleSet {
    CustomRuleSet {
        id: "http".into(),
        name: "demo".into(),
        tag: "demo".into(),
        category: "测试".into(),
        source: CustomRuleSetSource::Remote {
            url,
            format: RuleSetFormat::Binary,
        },
        remote_state: Default::default(),
        last_updated: 0,
        remote_updated_at: 0,
        builtin: false,
    }
}

#[tokio::test]
async fn conditional_etag_304_and_missing_cache_force_full_download() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let bodies = Arc::new(AtomicUsize::new(0));
    let seen = bodies.clone();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new().route(
                "/rules",
                axum::routing::get(move |headers: HeaderMap| {
                    let seen = seen.clone();
                    async move {
                        if headers.get("if-none-match").is_some_and(|h| h == "\"v1\"") {
                            return StatusCode::NOT_MODIFIED.into_response();
                        }
                        seen.fetch_add(1, Ordering::SeqCst);
                        ([("etag", "\"v1\"")], "content-v1").into_response()
                    }
                }),
            ),
        )
        .await
        .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let manager = RuleSetManager::new(dir.path().into());
    let mut rs = remote(format!("http://{addr}/rules"));
    assert_eq!(
        manager.update_custom_rule_set(&mut rs).await,
        RuleSetUpdateStatus::Updated
    );
    assert_eq!(rs.remote_updated_at, 0);
    let written = rs.last_updated;
    assert!(rs.remote_state.checked_at > 0);
    assert_eq!(
        manager.update_custom_rule_set(&mut rs).await,
        RuleSetUpdateStatus::Skipped
    );
    assert_eq!(rs.last_updated, written);
    assert_eq!(bodies.load(Ordering::SeqCst), 1);
    std::fs::remove_file(manager.custom_rule_set_file_path(&rs.id, rs.file_format())).unwrap();
    assert_eq!(
        manager.update_custom_rule_set(&mut rs).await,
        RuleSetUpdateStatus::Updated
    );
    assert_eq!(bodies.load(Ordering::SeqCst), 2);
    std::fs::write(
        manager.custom_rule_set_file_path(&rs.id, rs.file_format()),
        b"damaged-cache",
    )
    .unwrap();
    assert_eq!(
        manager.update_custom_rule_set(&mut rs).await,
        RuleSetUpdateStatus::Updated
    );
    assert_eq!(bodies.load(Ordering::SeqCst), 3);
    // Changing the effective origin discards the old validator.
    rs.remote_state.validator_url = "https://example.invalid/other".into();
    assert_eq!(
        manager.update_custom_rule_set(&mut rs).await,
        RuleSetUpdateStatus::Skipped
    );
    assert_eq!(bodies.load(Ordering::SeqCst), 4);
    server.abort();
}

#[tokio::test]
async fn no_headers_compares_content_and_failure_preserves_cache_and_metadata() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            axum::Router::new().route(
                "/rules",
                axum::routing::get(move || {
                    let n = count.fetch_add(1, Ordering::SeqCst);
                    async move {
                        match n {
                            0 | 1 => (StatusCode::OK, "one"),
                            2 => (StatusCode::OK, "two"),
                            3 => (StatusCode::OK, ""),
                            _ => (StatusCode::SERVICE_UNAVAILABLE, "error"),
                        }
                    }
                }),
            ),
        )
        .await
        .unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let manager = RuleSetManager::new(dir.path().into());
    let mut rs = remote(format!("http://{addr}/rules"));
    assert_eq!(
        manager.update_custom_rule_set(&mut rs).await,
        RuleSetUpdateStatus::Updated
    );
    assert_eq!(
        manager.update_custom_rule_set(&mut rs).await,
        RuleSetUpdateStatus::Skipped
    );
    assert_eq!(
        manager.update_custom_rule_set(&mut rs).await,
        RuleSetUpdateStatus::Updated
    );
    let saved = rs.clone();
    for _ in 0..2 {
        assert_eq!(
            manager.update_custom_rule_set(&mut rs).await,
            RuleSetUpdateStatus::Failed
        );
        assert_eq!(rs, saved);
    }
    assert_eq!(
        std::fs::read(manager.custom_rule_set_file_path(&rs.id, rs.file_format())).unwrap(),
        b"two"
    );
    server.abort();
}
