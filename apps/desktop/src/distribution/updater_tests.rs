use super::*;
use crate::distribution::signature::Artifact;

#[test]
fn update_gates_ignore_build_metadata_and_order_rc_identifiers_by_semver_precedence() {
    use std::cmp::Ordering::{Equal, Greater, Less};

    for (proposed, current, expected) in [
        ("0.2.0-rc.1", "0.1.0", Greater),
        ("0.2.0-rc.10", "0.2.0-rc.2", Greater),
        ("0.2.0", "0.2.0-rc.10", Greater),
        ("0.2.0-rc.1", "0.2.0", Less),
        ("0.2.0+new", "0.2.0+old", Equal),
        ("0.2.0+old", "0.2.0+new", Equal),
        ("0.2.0", "0.2.0+build.1", Equal),
        ("0.2.0-rc.1+new", "0.2.0-rc.1+old", Equal),
    ] {
        assert_eq!(version_precedence(proposed, current).unwrap(), expected);
    }
    assert!(version_precedence("not-semver", "0.1.0").is_err());
    assert!(version_precedence("0.2.0-rc.1", "not-semver").is_err());
}

#[test]
fn only_the_zip_can_be_offered_as_an_update() {
    let mut value = Manifest {
        schema: 1,
        product: crate::product::APP_ID.into(),
        channel: crate::product::UPDATE_CHANNEL.into(),
        version: "2.3.4".into(),
        arch: "x86_64".into(),
        artifact: Artifact {
            url: String::new(),
            size: 7,
            sha256: "a".repeat(64),
        },
        files: vec![],
    };
    value.artifact.url = format!(
        "{}/releases/download/gpui-v2.3.4/LanpathGPUI_2.3.4_windows_x86_64-portable.zip",
        crate::product::REPOSITORY
    );
    assert!(validate_kind(&value).is_ok());
    value.artifact.url = value.artifact.url.replace("-portable.zip", "-setup.exe");
    assert!(validate_kind(&value).is_err());
    value.artifact.url = format!(
        "{}/releases/download/gpui-v2.3.4/{}",
        crate::product::REPOSITORY,
        crate::product::BINARY_NAME
    );
    assert!(validate_kind(&value).is_err());
}

#[tokio::test]
async fn request_errors_never_disclose_private_redirect_urls() {
    let client = reqwest::Client::builder().https_only(true).build().unwrap();
    // HTTPS-only rejects this before DNS or a socket can be used.
    let error = client
        .get("http://private.invalid/account-secret?token=private-token")
        .send()
        .await
        .unwrap_err();
    let exposed = http_error(error).to_string();
    for secret in ["private.invalid", "account-secret", "private-token"] {
        assert!(!exposed.contains(secret));
    }
}

#[tokio::test]
async fn saved_explicit_proxy_snapshot_covers_metadata_signature_and_artifact_without_direct_fallback()
 {
    use crate::{
        business::{BusinessService, RequestSettingsDocument},
        configuration::Bindings,
    };
    use lanpath_client::RequestProxyMode;
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

    for mode in [RequestProxyMode::Core, RequestProxyMode::Custom] {
        let temporary = tempfile::tempdir().unwrap();
        let root = DataRoot::open(&temporary.path().join("data")).unwrap();
        let proxy = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_address = proxy.local_addr().unwrap();
        let origin = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin_address = origin.local_addr().unwrap();
        let business = BusinessService::new(
            root.path(),
            None,
            Bindings::new(proxy_address.port(), 19090),
        );
        business
            .save_request_settings(&RequestSettingsDocument::new(
                mode,
                format!("http://{proxy_address}"),
                String::new(),
            ))
            .unwrap();
        let (_, snapshot) = business.saved_request_policy().unwrap();
        let service = UpdateService::new(&root, &snapshot).unwrap();
        // Saving a new route after acceptance cannot alter the in-flight snapshot.
        business
            .save_request_settings(&RequestSettingsDocument::new(
                RequestProxyMode::None,
                String::new(),
                String::new(),
            ))
            .unwrap();
        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let (stream, _) = proxy.accept().await.unwrap();
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                assert_eq!(line, format!("CONNECT {origin_address} HTTP/1.1\r\n"));
                loop {
                    line.clear();
                    reader.read_line(&mut line).await.unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                }
                reader.get_mut().write_all(
                    b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                ).await.unwrap();
            }
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            for suffix in ["update.json", "update.json.sig"] {
                assert!(
                    service
                        .small_get(
                            &format!("https://{origin_address}/{suffix}"),
                            DESCRIPTOR_LIMIT
                        )
                        .await
                        .is_err()
                );
            }
            // The artifact path shares this exact transport, after signed-manifest validation.
            assert!(
                service
                    .get(
                        &format!("https://{origin_address}/update-portable.zip"),
                        Duration::from_secs(600)
                    )
                    .await
                    .is_err()
            );
            server.await.unwrap();
        })
        .await
        .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), origin.accept())
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn updater_direct_policy_reaches_origin_and_still_requires_verified_https() {
    use lanpath_client::RequestProxyMode;
    use tokio::io::AsyncWriteExt as _;

    let temporary = tempfile::tempdir().unwrap();
    let root = DataRoot::open(&temporary.path().join("data")).unwrap();
    let origin = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = origin.local_addr().unwrap();
    let config = ClientConfig {
        request_proxy_mode: RequestProxyMode::None,
        ..ClientConfig::default()
    };
    let service = UpdateService::new(&root, &config).unwrap();
    assert!(
        service
            .small_get(&format!("http://{address}/update.json"), DESCRIPTOR_LIMIT)
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), origin.accept())
            .await
            .is_err()
    );
    let server = tokio::spawn(async move {
        let (mut stream, _) = origin.accept().await.unwrap();
        // A reachable origin speaking plaintext cannot weaken the updater's TLS requirement.
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .await
            .unwrap();
    });
    tokio::time::timeout(Duration::from_secs(5), async {
        assert!(
            service
                .small_get(&format!("https://{address}/update.json"), DESCRIPTOR_LIMIT)
                .await
                .is_err()
        );
        server.await.unwrap();
    })
    .await
    .unwrap();
}
