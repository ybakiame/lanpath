use super::*;

fn service(root: &Path) -> MitmService {
    MitmService::new(
        root,
        Arc::new(lanpath_script::MockHttpExecutor::with_responses(vec![])),
    )
}
fn config(root: &Path) -> ClientConfig {
    let mut config = ClientConfig::new(root.to_path_buf(), "", "", PathBuf::new());
    config.mitm_enabled = true;
    config.mitm.hostnames = vec!["api.example.com".into()];
    config
}

#[tokio::test]
async fn mismatched_ca_or_data_root_is_rejected_before_generating_any_identity() {
    let root = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let service = service(root.path());
    let mut config = config(root.path());
    config.mitm.ca_dir = other.path().join("certs");
    let notifier = Arc::new(lanpath_script::RecordingNotifier::new());
    assert!(
        service
            .start(&config, notifier.clone())
            .await
            .err()
            .unwrap()
            .to_string()
            .contains("CA 目录")
    );
    assert!(!root.path().join("certs").exists());
    assert!(!other.path().join("certs").exists());
    config.mitm.ca_dir = root.path().join("certs");
    config.data_dir = other.path().to_path_buf();
    assert!(service.start(&config, notifier).await.is_err());
    assert!(!root.path().join("certs").exists());
    assert!(!other.path().join("certs").exists());
}

#[test]
fn stale_display_authorization_does_not_install_or_regenerate_a_replaced_ca() {
    let root = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let service = service(root.path());
    let displayed = service.ca().unwrap();
    let replacement = FileCaStore::new(other.path()).load_or_generate().unwrap();
    std::fs::write(service.ca_path(), &replacement.cert_pem).unwrap();
    std::fs::write(root.path().join("certs/ca.key"), &replacement.key_pem).unwrap();
    let error = service
        .install_ca(&displayed.sha256)
        .unwrap_err()
        .to_string();
    assert!(error.contains("changed since authorization"), "{error}");
    assert_eq!(
        std::fs::read_to_string(service.ca_path()).unwrap(),
        replacement.cert_pem
    );
    assert!(
        std::fs::read_to_string(root.path().join("certs/ca.key")).unwrap() == replacement.key_pem
    );
    assert!(!root.path().join("certs/ca-trust-receipt.json").exists());
}

#[tokio::test]
async fn running_proxy_identity_cannot_be_reauthorized_as_a_replaced_disk_ca() {
    let root = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let service = service(root.path());
    let displayed = service.ca().unwrap();
    let mut session = service
        .start(
            &config(root.path()),
            Arc::new(lanpath_script::RecordingNotifier::new()),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(service.ca().unwrap().sha256, displayed.sha256);
    let replacement = FileCaStore::new(other.path()).load_or_generate().unwrap();
    std::fs::write(service.ca_path(), &replacement.cert_pem).unwrap();
    std::fs::write(root.path().join("certs/ca.key"), &replacement.key_pem).unwrap();
    let replacement_fingerprint = fingerprint(&replacement).unwrap();
    assert!(
        service
            .ca()
            .unwrap_err()
            .to_string()
            .contains("running MITM identity")
    );
    assert!(
        service
            .install_ca(&replacement_fingerprint)
            .unwrap_err()
            .to_string()
            .contains("running MITM identity")
    );
    assert!(service.install_ca(&displayed.sha256).is_err());
    // Missing both files during a session must not mint a replacement before detecting the mismatch.
    std::fs::remove_file(service.ca_path()).unwrap();
    std::fs::remove_file(root.path().join("certs/ca.key")).unwrap();
    assert!(service.ca().is_err());
    assert!(!service.ca_path().exists());
    std::fs::write(service.ca_path(), &replacement.cert_pem).unwrap();
    std::fs::write(root.path().join("certs/ca.key"), &replacement.key_pem).unwrap();
    session.stop().await.unwrap();
    assert_eq!(service.ca().unwrap().sha256, replacement_fingerprint);
}

#[test]
fn absent_ca_install_and_unowned_removal_never_create_an_identity() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path());
    assert!(service.install_ca("stale authorization").is_err());
    assert!(service.remove_ca("no receipt").is_err());
    assert!(!root.path().join("certs").exists());
}
