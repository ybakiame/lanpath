use lanpath_client::{ClientConfig, RequestProxyMode, ScriptDialect};
use serde_json::Value;

use super::{BusinessService, RequestSettingsDocument};
use crate::configuration::Bindings;

fn service(root: &std::path::Path) -> BusinessService {
    BusinessService::new(root, None, Bindings::new(17899, 19090))
}

fn unrelated(config: &ClientConfig) -> Value {
    let mut value = serde_json::to_value(config).unwrap();
    let fields = value.as_object_mut().unwrap();
    for key in [
        "request_proxy_mode",
        "request_proxy_url",
        "github_proxy_prefix",
    ] {
        fields.remove(key);
    }
    value
}

#[test]
fn request_modes_reopen_without_losing_credentials_or_mirror_semantics() {
    let root = tempfile::tempdir().unwrap();
    let business = service(root.path());
    assert_eq!(
        business.request_settings().unwrap().mode(),
        RequestProxyMode::None
    );
    let url = "socks5h://fixture-user:fixture-password@127.0.0.1:1080";
    let prefix = "  https://mirror.example/  ";
    for mode in [
        RequestProxyMode::None,
        RequestProxyMode::System,
        RequestProxyMode::Core,
        RequestProxyMode::Custom,
    ] {
        let settings = RequestSettingsDocument::new(mode, url.into(), prefix.into());
        business.save_request_settings(&settings).unwrap();
        let reopened = service(root.path());
        let document = reopened.request_settings().unwrap();
        assert_eq!(document.mode(), mode);
        assert!(document.custom_url() == url);
        assert_eq!(document.github_prefix(), prefix);
        // Exercise the shared consumer too, not merely the page's load method.
        let consumer = ClientConfig::load_for_requests(root.path()).unwrap();
        assert_eq!(consumer.request_proxy_mode, mode);
        assert!(consumer.request_proxy_url == url);
        assert_eq!(
            lanpath_client::apply_github_proxy_prefix(
                "https://api.github.com/zen",
                &consumer.github_proxy_prefix,
            ),
            "https://mirror.example/https://api.github.com/zen",
        );
    }
}

#[test]
fn request_save_merges_latest_unrelated_settings_without_clobbering_system_configuration() {
    let root = tempfile::tempdir().unwrap();
    let business = service(root.path());
    let stale_draft = RequestSettingsDocument::new(
        RequestProxyMode::Custom,
        "http://fixture-user:fixture-password@127.0.0.1:8080".into(),
        "https://mirror.example".into(),
    );
    business.request_settings().unwrap();
    // These edits occur after the request editor was loaded.
    let mut latest = business.client_config().unwrap();
    latest.active_subscription_id = Some(uuid::Uuid::new_v4());
    latest.rule_mode = "global".into();
    latest.mitm_enabled = true;
    latest.mitm.hostnames = vec!["api.example.com".into()];
    latest.mitm.script_dialect = ScriptDialect::Loon;
    latest.system_proxy_enabled = true;
    latest.tun_enabled = true;
    latest.tun_auto_route = false;
    latest.mixed_port = 23456;
    latest.clash_api_enabled = false;
    latest.clash_api_port = 23457;
    latest.clash_api_secret = "fixture-api-secret".into();
    latest.save().unwrap();
    let expected_unrelated = unrelated(&latest);
    business.save_request_settings(&stale_draft).unwrap();
    let saved = business.client_config().unwrap();
    assert!(unrelated(&saved) == expected_unrelated);
}

#[test]
fn invalid_custom_proxy_redacts_credentials_and_keeps_prior_saved_document() {
    let root = tempfile::tempdir().unwrap();
    let business = service(root.path());
    business
        .save_request_settings(&RequestSettingsDocument::new(
            RequestProxyMode::System,
            String::new(),
            "https://mirror.example".into(),
        ))
        .unwrap();
    let mut config = business.client_config().unwrap();
    config.rule_mode = "direct".into();
    config.mitm_enabled = true;
    config.save().unwrap();
    let file = root.path().join("app.toml");
    let prior = std::fs::read(&file).unwrap();
    for url in [
        "http://secret-user:secret-password@127.0.0.1:8080/private",
        "ftp://secret-user:secret-password@127.0.0.1:8080",
        "http://secret-user:secret-password@127.0.0.1:0",
        "http://secret-user:secret-password@127.0.0.1:8080?secret-token",
    ] {
        let error = business
            .save_request_settings(&RequestSettingsDocument::new(
                RequestProxyMode::Custom,
                url.into(),
                "changed-mirror".into(),
            ))
            .err()
            .expect("invalid custom settings must fail")
            .to_string();
        for secret in ["secret-user", "secret-password", "secret-token", url] {
            assert!(!error.contains(secret));
        }
        assert!(std::fs::read(&file).unwrap() == prior);
        let saved = business.request_settings().unwrap();
        assert_eq!(saved.mode(), RequestProxyMode::System);
        assert!(saved.custom_url().is_empty());
        assert_eq!(saved.github_prefix(), "https://mirror.example");
    }
}

#[test]
fn normalized_request_reads_leave_existing_file_bytes_untouched() {
    let root = tempfile::tempdir().unwrap();
    let business = service(root.path());
    business
        .save_request_settings(&RequestSettingsDocument::new(
            RequestProxyMode::Core,
            String::new(),
            "https://mirror.example".into(),
        ))
        .unwrap();
    let file = root.path().join("app.toml");
    let original = std::fs::read(&file).unwrap();
    business.request_settings().unwrap();
    let (snapshot, policy) = business.saved_request_policy().unwrap();
    assert_eq!(snapshot.mode(), RequestProxyMode::Core);
    assert_eq!(policy.request_proxy_mode, RequestProxyMode::Core);
    assert!(std::fs::read(&file).unwrap() == original);
}

#[test]
fn request_read_persists_path_only_normalization_without_clearing_saved_policy() {
    let root = tempfile::tempdir().unwrap();
    let business = service(root.path());
    business
        .save_request_settings(&RequestSettingsDocument::new(
            RequestProxyMode::System,
            String::new(),
            "https://mirror.example".into(),
        ))
        .unwrap();
    let mut config = business.client_config().unwrap();
    config.data_dir = root.path().join("outside-data");
    config.mitm.ca_dir = root.path().join("outside-ca");
    let file = root.path().join("app.toml");
    super::state::persist(&file, &config).unwrap();
    assert_eq!(
        business.request_settings().unwrap().mode(),
        RequestProxyMode::System
    );
    let repaired: Value = lanpath_client::settings::load_client(&file).unwrap();
    assert_eq!(
        repaired["data_dir"],
        serde_json::to_value(root.path()).unwrap()
    );
    assert_eq!(
        repaired["mitm"]["ca_dir"],
        serde_json::to_value(root.path().join("certs")).unwrap()
    );
    assert_eq!(repaired["github_proxy_prefix"], "https://mirror.example");
}

#[tokio::test]
async fn accepted_github_probe_uses_saved_snapshot_even_after_settings_change() {
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

    let root = tempfile::tempdir().unwrap();
    let business = service(root.path());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mirror = format!("http://{address}");
    business
        .save_request_settings(&RequestSettingsDocument::new(
            RequestProxyMode::None,
            String::new(),
            mirror,
        ))
        .unwrap();
    let (tested, snapshot) = business.saved_request_policy().unwrap();
    // A later save must not change an already accepted test's route or label.
    business
        .save_request_settings(&RequestSettingsDocument::new(
            RequestProxyMode::Custom,
            "http://127.0.0.1:1".into(),
            String::new(),
        ))
        .unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut reader = BufReader::new(stream);
        let mut request_line = String::new();
        reader.read_line(&mut request_line).await.unwrap();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
        }
        reader
            .get_mut()
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nzen")
            .await
            .unwrap();
        request_line
    });
    lanpath_client::test_github_connectivity(&snapshot)
        .await
        .unwrap();
    assert_eq!(
        server.await.unwrap(),
        "GET /https://api.github.com/zen HTTP/1.1\r\n",
    );
    assert_eq!(tested.mode(), RequestProxyMode::None);
    assert_eq!(
        business.request_settings().unwrap().mode(),
        RequestProxyMode::Custom
    );
    let current = business.client_config().unwrap();
    assert!(!current.system_proxy_enabled);
    assert!(!current.tun_enabled);
}
