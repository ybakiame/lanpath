use super::*;

struct Fixture {
    root: DataRoot,
    _target: tempfile::TempDir,
    source: tempfile::TempDir,
    original: Vec<u8>,
    source_bytes: Vec<u8>,
}
impl Fixture {
    fn new() -> Self {
        let target = tempfile::tempdir().unwrap();
        let root = DataRoot::open(target.path()).unwrap();
        let source = tempfile::tempdir().unwrap();
        let target_config = lanpath_client::ClientConfig::new(
            root.path().into(),
            String::new(),
            String::new(),
            PathBuf::new(),
        );
        let original = serde_json::to_vec(&target_config).unwrap();
        std::fs::write(root.path().join("client.json"), &original).unwrap();
        let mut incoming = lanpath_client::ClientConfig::new(
            source.path().into(),
            String::new(),
            String::new(),
            PathBuf::new(),
        );
        incoming.mixed_port = 18881;
        incoming.system_proxy_enabled = true;
        incoming.tun_enabled = true;
        incoming.mitm_enabled = true;
        // A secret can look like a local path without being a path reference.
        incoming.clash_api_secret = source
            .path()
            .join("literal-secret")
            .to_string_lossy()
            .into_owned();
        let source_bytes = serde_json::to_vec(&incoming).unwrap();
        std::fs::write(source.path().join("client.json"), &source_bytes).unwrap();
        Self {
            root,
            _target: target,
            source,
            original,
            source_bytes,
        }
    }
    fn preview(&self) -> MigrationPreview {
        MigrationService::new(&self.root)
            .preview(
                self.source.path(),
                SourceKind::Tauri,
                MigrationOptions {
                    configuration: true,
                    history: false,
                    appearance: false,
                    ca: CaScope::Exclude,
                },
            )
            .unwrap()
    }
}

#[test]
fn import_and_rollback_replace_stores_only_at_startup_and_preserve_the_source() {
    let fixture = Fixture::new();
    let service = MigrationService::new(&fixture.root);
    service.prepare(fixture.preview()).unwrap();
    assert_eq!(
        std::fs::read(fixture.root.path().join("client.json")).unwrap(),
        fixture.original
    );
    assert_eq!(service.startup().unwrap(), MigrationRecovery::Applied);
    let imported = std::fs::read(fixture.root.path().join("client.json")).unwrap();
    let config: lanpath_client::ClientConfig = serde_json::from_slice(&imported).unwrap();
    let source: lanpath_client::ClientConfig =
        serde_json::from_slice(&fixture.source_bytes).unwrap();
    assert_eq!(config.mixed_port, 18881);
    assert_eq!(config.data_dir, fixture.root.path());
    assert_eq!(config.clash_api_secret, source.clash_api_secret);
    assert!(!config.system_proxy_enabled && !config.tun_enabled && !config.mitm_enabled);
    service.request_rollback().unwrap();
    assert_eq!(
        service.status().unwrap(),
        MigrationRecovery::RollbackPending
    );
    assert_eq!(
        std::fs::read(fixture.root.path().join("client.json")).unwrap(),
        imported
    );
    assert!(service.acknowledge().is_err());
    assert_eq!(service.startup().unwrap(), MigrationRecovery::RolledBack);
    assert_eq!(
        std::fs::read(fixture.root.path().join("client.json")).unwrap(),
        fixture.original
    );
    assert_eq!(
        std::fs::read(fixture.source.path().join("client.json")).unwrap(),
        fixture.source_bytes
    );
    service.acknowledge().unwrap();
    assert_eq!(service.status().unwrap(), MigrationRecovery::None);
}

#[test]
fn stale_target_preview_cannot_overwrite_a_newer_local_configuration() {
    let fixture = Fixture::new();
    let service = MigrationService::new(&fixture.root);
    let preview = fixture.preview();
    let mut changed: lanpath_client::ClientConfig =
        serde_json::from_slice(&fixture.original).unwrap();
    changed.mixed_port = 19991;
    let changed = serde_json::to_vec(&changed).unwrap();
    std::fs::write(fixture.root.path().join("client.json"), &changed).unwrap();
    assert!(service.prepare(preview).is_err());
    assert_eq!(
        std::fs::read(fixture.root.path().join("client.json")).unwrap(),
        changed
    );
    assert_eq!(
        std::fs::read(fixture.source.path().join("client.json")).unwrap(),
        fixture.source_bytes
    );
}

#[test]
fn history_snapshot_includes_committed_wal_rows_without_checkpointing_the_source() {
    let fixture = Fixture::new();
    let database = rusqlite::Connection::open(fixture.source.path().join("stats.db")).unwrap();
    database.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE captured (bytes INTEGER NOT NULL); INSERT INTO captured VALUES (7341);").unwrap();
    let names = ["stats.db", "stats.db-wal", "stats.db-shm"];
    let original: Vec<_> = names
        .iter()
        .map(|name| std::fs::read(fixture.source.path().join(name)).unwrap())
        .collect();
    let service = MigrationService::new(&fixture.root);
    let preview = service
        .preview(
            fixture.source.path(),
            SourceKind::Tauri,
            MigrationOptions {
                configuration: false,
                history: true,
                appearance: false,
                ca: CaScope::Exclude,
            },
        )
        .unwrap();
    service.prepare(preview).unwrap();
    assert_eq!(service.startup().unwrap(), MigrationRecovery::Applied);
    let snapshot = rusqlite::Connection::open(fixture.root.path().join("stats.db")).unwrap();
    let bytes: i64 = snapshot
        .query_row("SELECT bytes FROM captured", [], |row| row.get(0))
        .unwrap();
    assert_eq!(bytes, 7341);
    for (name, expected) in names.iter().zip(original) {
        assert_eq!(
            std::fs::read(fixture.source.path().join(name)).unwrap(),
            expected
        );
    }
}
