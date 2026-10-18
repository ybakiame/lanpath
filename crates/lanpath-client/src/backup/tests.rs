use super::*;
use std::io::{Cursor, Write};

fn fixture() -> (tempfile::TempDir, Snapshot) {
    let dir = tempfile::tempdir().unwrap();
    let cfg = crate::ClientConfig::new(
        dir.path().into(),
        "https://example.invalid",
        "private-token",
        Default::default(),
    );
    cfg.save().unwrap();
    std::fs::create_dir(dir.path().join("certs")).unwrap();
    std::fs::write(dir.path().join("certs/ca.key"), "DO NOT COPY").unwrap();
    let source = crate::SubscriptionSource {
        format: crate::SubFormat::ClashYaml,
        content: "# 原文\r\nproxies: []\r\n".into(),
    };
    crate::SubscriptionStore::new(dir.path().into())
        .write_source(uuid::Uuid::nil(), &source)
        .unwrap();
    let snapshot = capture(dir.path(), None).unwrap();
    (dir, snapshot)
}

#[test]
fn encrypted_roundtrip_wrong_password_and_tampering() {
    let (_dir, snapshot) = fixture();
    let bytes = encode(&snapshot, "correct horse battery staple").unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("private-token"));
    assert!(!String::from_utf8_lossy(&bytes).contains("client.json"));
    let restored = decode(&bytes, "correct horse battery staple").unwrap();
    assert_eq!(snapshot.files, restored.files);
    let source = restored
        .files
        .iter()
        .find(|(name, _)| name.ends_with(".source.json"))
        .unwrap()
        .1;
    assert_eq!(
        serde_json::from_slice::<crate::SubscriptionSource>(source)
            .unwrap()
            .content,
        "# 原文\r\nproxies: []\r\n"
    );
    assert!(decode(&bytes, "wrong password").is_err());
    let mut damaged = bytes;
    let end = damaged.len() - 10;
    damaged[end] ^= 1;
    assert!(decode(&damaged, "correct horse battery staple").is_err());
}

#[test]
fn unsafe_paths_and_future_version_are_rejected() {
    for path in [
        "../client.json",
        "/client.json",
        "scripts/../../secret",
        "scripts\\evil",
        "certs/ca.key",
        "scripts/.. /evil",
        "scripts/CON.txt",
        "scripts/a?b.js",
        "scripts/a/b/c/d/e/f/g/h.js",
        "scripts/name.",
        "scripts",
    ] {
        assert!(!allowed(path), "{path}");
    }
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("../client.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"{}").unwrap();
    assert!(archive::unpack(zip.finish().unwrap().into_inner()).is_err());
}

#[test]
fn snapshot_excludes_machine_data_and_rebases_managed_paths() {
    let (source, snapshot) = fixture();
    assert!(!snapshot.files.keys().any(|s| s.starts_with("certs")));
    let text = String::from_utf8(snapshot.files["client.json"].clone()).unwrap();
    assert!(!text.contains(source.path().to_str().unwrap()));
    let target = tempfile::tempdir().unwrap();
    let mut snapshot = snapshot;
    snapshot::relocate(&mut snapshot, target.path()).unwrap();
    let cfg: crate::ClientConfig = serde_json::from_slice(&snapshot.files["client.json"]).unwrap();
    assert_eq!(cfg.data_dir, target.path());
    assert!(!cfg.tun_enabled && !cfg.system_proxy_enabled && !cfg.mitm_enabled);
    assert!(cfg.core_binary.as_os_str().is_empty());
}

#[test]
fn startup_transaction_replaces_supported_roots_and_is_idempotent() {
    let (source, snapshot) = fixture();
    let target = tempfile::tempdir().unwrap();
    std::fs::write(target.path().join("subscriptions.json"), b"[]").unwrap();
    std::fs::write(target.path().join("untouched.log"), "keep").unwrap();
    // Stage directly to isolate startup recovery from password KDF cost.
    let pending = target.path().join(".backup-pending");
    std::fs::create_dir(&pending).unwrap();
    let mut snapshot = snapshot;
    snapshot::relocate(&mut snapshot, target.path()).unwrap();
    for (name, bytes) in snapshot.files {
        let path = pending.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    std::fs::write(
        pending.join("manifest.json"),
        serde_json::to_vec(&snapshot.summary).unwrap(),
    )
    .unwrap();
    apply_pending(target.path()).unwrap();
    assert!(!target.path().join("subscriptions.json").exists());
    assert_eq!(
        crate::ClientConfig::load(target.path()).unwrap().sub_token,
        "private-token"
    );
    assert!(source.path().join("client.json").exists());
    assert_eq!(
        std::fs::read_to_string(target.path().join("untouched.log")).unwrap(),
        "keep"
    );
    apply_pending(target.path()).unwrap();
}

#[test]
fn interrupted_transaction_restores_original_before_retry() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".backup-transaction/original")).unwrap();
    std::fs::write(
        dir.path().join(".backup-transaction/original/client.json"),
        b"{\"sub_token\":\"old\"}",
    )
    .unwrap();
    std::fs::write(dir.path().join("client.json"), b"{\"sub_token\":\"new\"}").unwrap();
    std::fs::write(
        dir.path().join(".backup-journal.json"),
        br#"{"originals":["client.json"],"committed":false}"#,
    )
    .unwrap();
    apply_pending(dir.path()).unwrap();
    assert_eq!(
        crate::ClientConfig::load(dir.path()).unwrap().sub_token,
        "old"
    );
    assert!(!dir.path().join(".backup-journal.json").exists());
}

#[cfg(unix)]
#[test]
fn symlink_capture_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("/etc", dir.path().join("scripts")).unwrap();
    assert!(capture(dir.path(), None).is_err());
}

#[test]
fn prepare_keeps_current_files_until_restart_and_saves_encrypted_rollback() {
    let (_source, snapshot) = fixture();
    let target = tempfile::tempdir().unwrap();
    let mut cfg = crate::ClientConfig::new(
        target.path().into(),
        "",
        "previous-token",
        Default::default(),
    );
    cfg.core_binary = target.path().join("cores/target-core");
    cfg.save().unwrap();
    let rollback = prepare_restore(
        target.path(),
        snapshot,
        "correct horse battery staple",
        None,
    )
    .unwrap();
    assert_eq!(
        crate::ClientConfig::load(target.path()).unwrap().sub_token,
        "previous-token"
    );
    let bytes = std::fs::read(rollback).unwrap();
    let previous = decode(&bytes, "correct horse battery staple").unwrap();
    let saved: crate::ClientConfig =
        serde_json::from_slice(&previous.files["client.json"]).unwrap();
    assert_eq!(saved.sub_token, "previous-token");
    apply_pending(target.path()).unwrap();
    let restored = crate::ClientConfig::load(target.path()).unwrap();
    assert_eq!(restored.sub_token, "private-token");
    assert_eq!(restored.core_binary, cfg.core_binary);
}

#[test]
fn invalid_pending_keeps_original_client_usable() {
    let (dir, _) = fixture();
    std::fs::create_dir(dir.path().join(".backup-pending")).unwrap();
    std::fs::write(dir.path().join(".backup-pending/manifest.json"), "not JSON").unwrap();
    apply_pending_at_startup(dir.path()).unwrap();
    assert_eq!(
        crate::ClientConfig::load(dir.path()).unwrap().sub_token,
        "private-token"
    );
    assert!(dir.path().join(".backup-failed").exists());
    assert!(!dir.path().join(".backup-pending").exists());
}

#[test]
fn archive_enforces_version_duplicates_and_expanded_limit() {
    let (_dir, mut snapshot) = fixture();
    snapshot
        .files
        .insert("scripts/Node.js".into(), b"a".to_vec());
    snapshot
        .files
        .insert("scripts/node.js".into(), b"b".to_vec());
    assert!(snapshot::validate(&mut snapshot).is_err());
    snapshot.files.remove("scripts/Node.js");
    snapshot.files.remove("scripts/node.js");
    snapshot.summary.version = 3;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(&serde_json::to_vec(&snapshot.summary).unwrap())
        .unwrap();
    zip.start_file("client.json", options).unwrap();
    zip.write_all(&snapshot.files["client.json"]).unwrap();
    assert!(archive::unpack(zip.finish().unwrap().into_inner()).is_err());

    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(
        "scripts/large.js",
        options.compression_method(zip::CompressionMethod::Deflated),
    )
    .unwrap();
    let chunk = vec![0u8; 1024 * 1024];
    for _ in 0..65 {
        zip.write_all(&chunk).unwrap();
    }
    assert!(archive::unpack(zip.finish().unwrap().into_inner()).is_err());
}

#[test]
fn ruleset_files_roundtrip_and_restore_without_changing_source_content() {
    let (source, _) = fixture();
    let binary = b"SRS\0\x01binary-rules";
    let json = b"{\n  \"version\": 3, \"rules\": []\n}\n";
    std::fs::create_dir_all(source.path().join("rulesets/builtin")).unwrap();
    std::fs::create_dir_all(source.path().join("rulesets/custom")).unwrap();
    std::fs::write(
        source.path().join("rulesets/builtin/geosite-private.srs"),
        binary,
    )
    .unwrap();
    std::fs::write(source.path().join("rulesets/custom/manual.json"), json).unwrap();
    let snapshot = capture(source.path(), None).unwrap();
    assert_eq!(snapshot.summary.version, 2);
    let encrypted = encode(&snapshot, "correct horse battery staple").unwrap();
    let snapshot = decode(&encrypted, "correct horse battery staple").unwrap();
    let target = tempfile::tempdir().unwrap();
    prepare_restore(
        target.path(),
        snapshot,
        "correct horse battery staple",
        None,
    )
    .unwrap();
    apply_pending(target.path()).unwrap();
    assert_eq!(
        std::fs::read(target.path().join("rulesets/builtin/geosite-private.srs")).unwrap(),
        binary
    );
    assert_eq!(
        std::fs::read(target.path().join("rulesets/custom/manual.json")).unwrap(),
        json
    );
}

#[test]
fn legacy_backup_decodes_and_old_journal_does_not_remove_rulesets() {
    let (_source, mut snapshot) = fixture();
    snapshot.summary.version = 1;
    let bytes = encode(&snapshot, "correct horse battery staple").unwrap();
    assert_eq!(
        decode(&bytes, "correct horse battery staple")
            .unwrap()
            .summary
            .version,
        1
    );
    let target = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(target.path().join("rulesets/custom")).unwrap();
    std::fs::write(target.path().join("rulesets/custom/keep.srs"), b"keep").unwrap();
    std::fs::create_dir_all(target.path().join(".backup-transaction/original")).unwrap();
    std::fs::write(
        target.path().join(".backup-journal.json"),
        br#"{"originals":[],"committed":false}"#,
    )
    .unwrap();
    apply_pending(target.path()).unwrap();
    assert_eq!(
        std::fs::read(target.path().join("rulesets/custom/keep.srs")).unwrap(),
        b"keep"
    );
}

#[test]
fn interrupted_v2_transaction_rolls_back_ruleset_files() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir
        .path()
        .join(".backup-transaction/original/rulesets/custom");
    std::fs::create_dir_all(&original).unwrap();
    std::fs::write(original.join("rules.srs"), b"original").unwrap();
    std::fs::create_dir_all(dir.path().join("rulesets/custom")).unwrap();
    std::fs::write(dir.path().join("rulesets/custom/rules.srs"), b"incoming").unwrap();
    std::fs::write(
        dir.path().join(".backup-journal.json"),
        br#"{"roots":["rulesets"],"originals":["rulesets"],"committed":false}"#,
    )
    .unwrap();
    apply_pending(dir.path()).unwrap();
    assert_eq!(
        std::fs::read(dir.path().join("rulesets/custom/rules.srs")).unwrap(),
        b"original"
    );
}
