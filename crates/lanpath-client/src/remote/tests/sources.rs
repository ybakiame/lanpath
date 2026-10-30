use super::*;
use crate::import::parse_import;

#[tokio::test]
async fn provenance_uses_only_valid_active_remote_and_registered_local_collections() {
    let base =
        spawn_remote_server(|_| "[mitm]\nhostname=remote.invalid, shared.invalid".into()).await;
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().to_path_buf());
    assert!(collect_hostname_sources(root.path()).is_empty());
    let remote = RemoteResource {
        name: "remote".into(),
        url: format!("{base}/snippet"),
        kind: RemoteKind::Snippet,
        dialect: ScriptDialect::QuantumultX,
        ..Default::default()
    };
    manager.save(std::slice::from_ref(&remote)).unwrap();
    assert_eq!(manager.fetch_all(&[remote]).await.fetched, 1);
    let imported = parse_import(
        "[MITM]\nhostname=local.invalid,shared.invalid",
        ScriptDialect::Surge,
    )
    .unwrap();
    manager.merge_imported(&imported).unwrap();
    let view = collect_hostname_sources(root.path());
    assert_eq!(view.len(), 3);
    assert_eq!(view[0].hostname, "local.invalid");
    assert_eq!(view[0].sources, ["本地导入"]);
    assert_eq!(view[1].hostname, "remote.invalid");
    assert_eq!(view[1].sources, ["remote"]);
    assert_eq!(view[2].hostname, "shared.invalid");
    assert_eq!(view[2].sources, ["remote", "本地导入"]);
    // An orphan is not a new source; disk enumeration cannot grant activation.
    std::fs::write(
        root.path().join("remote_cache/orphan.json"),
        r#"{"version":2,"module_id":"orphan","hostnames":["orphan.invalid"]}"#,
    )
    .unwrap();
    assert_eq!(collect_hostname_sources(root.path()), view);
    std::fs::write(
        root.path().join("remote_cache/local_imports.json"),
        "broken registry",
    )
    .unwrap();
    let merged = manager.load_cached().unwrap();
    assert_eq!(merged.hostnames, ["remote.invalid", "shared.invalid"]);
    assert!(
        merged
            .warnings
            .iter()
            .any(|warning| warning.contains("registry is unreadable"))
    );
    assert_eq!(collect_hostname_sources(root.path()).len(), 2);
}
