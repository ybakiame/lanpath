use super::*;
use std::{fs::File, time::SystemTime};

const PASSWORD: &str = "correct horse battery staple";

fn configured(token: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    crate::ClientConfig::new(dir.path().into(), "", token, Default::default())
        .save()
        .unwrap();
    dir
}

fn appearance(accent: &str) -> Appearance {
    Appearance {
        theme: "dark".into(),
        accent: accent.into(),
        pure_black: true,
    }
}

#[test]
fn service_export_restores_through_existing_archive_api() {
    let source = configured("service-export-token");
    for name in ["certs", "cores", "logs"] {
        std::fs::create_dir(source.path().join(name)).unwrap();
        std::fs::write(source.path().join(name).join("private"), b"excluded").unwrap();
    }
    std::fs::write(
        source.path().join("appearance.json"),
        br#"{"theme":"dark","accent":"blue","pure_black":true,"scale_percent":150}"#,
    )
    .unwrap();
    let service = BackupService::new(source.path().into());
    let encrypted = service.export(PASSWORD, Some(appearance("blue"))).unwrap();
    let snapshot = decode(&encrypted, PASSWORD).unwrap();
    assert_eq!(snapshot.summary.version, 2);
    assert_eq!(snapshot.files.len(), 1);
    assert!(snapshot.files.contains_key("client.json"));
    let target = configured("old-target-token");
    prepare_restore(target.path(), snapshot, PASSWORD, None).unwrap();
    apply_pending_at_startup(target.path()).unwrap();
    let restored = crate::ClientConfig::load(target.path()).unwrap();
    assert_eq!(restored.sub_token, "service-export-token");
    assert_eq!(restored.data_dir, target.path());
    assert!(!target.path().join("appearance.json").exists());
}

#[test]
fn legacy_pending_restore_converts_to_app_toml_and_preserves_appearance() {
    let source = configured("toml-restore-token");
    let snapshot = capture(source.path(), None).unwrap();
    let target = configured("current-target-token");
    prepare_restore(target.path(), snapshot, PASSWORD, None).unwrap();

    // The migration session validates first, then converts current settings,
    // and only then applies the pending archive.
    crate::settings::ensure_app_settings(target.path()).unwrap();
    crate::settings::persist_app_section(
        target.path(),
        "appearance",
        &serde_json::json!({"scale_percent": 117, "theme": "dark"}),
    )
    .unwrap();
    validate_pending(target.path()).unwrap();
    apply_validated_pending(target.path()).unwrap();

    let restored = crate::ClientConfig::load(target.path()).unwrap();
    assert_eq!(restored.sub_token, "toml-restore-token");
    let appearance: serde_json::Value =
        crate::settings::load_app_section(target.path(), "appearance")
            .unwrap()
            .unwrap();
    assert_eq!(appearance["scale_percent"], 117);
    assert_eq!(appearance["theme"], "dark");
    assert!(!target.path().join(".backup-pending").exists());
}

#[test]
fn legacy_import_is_one_use_and_encrypted_history_can_restore_previous_configuration() {
    let source = configured("legacy-import-token");
    let mut legacy = capture(source.path(), Some(appearance("amber"))).unwrap();
    legacy.summary.version = 1;
    let encrypted = encode(&legacy, PASSWORD).unwrap();
    let target = configured("previous-target-token");
    let service = BackupService::new(target.path().into());
    assert!(service.previous().unwrap().is_none());
    assert!(service.restored().unwrap().is_none());
    let preview = service.inspect(&encrypted, PASSWORD.into()).unwrap();
    assert_eq!(preview.summary.version, 1);
    // Unsupported shell accents remain portable metadata, not an archive error.
    assert_eq!(preview.summary.appearance.unwrap().accent, "amber");
    assert!(service.take_preview("wrong-token").is_err());
    service
        .take_preview(&preview.preview_id)
        .unwrap()
        .prepare(target.path(), Some(appearance("rose")))
        .unwrap();
    assert!(service.take_preview(&preview.preview_id).is_err());
    assert!(service.is_pending());
    assert_eq!(
        crate::ClientConfig::load(target.path()).unwrap().sub_token,
        "previous-target-token"
    );
    let rollback = service.previous().unwrap().unwrap();
    let saved = decode(&rollback, PASSWORD).unwrap();
    let saved_config: crate::ClientConfig =
        serde_json::from_slice(&saved.files["client.json"]).unwrap();
    assert_eq!(saved_config.sub_token, "previous-target-token");
    assert_eq!(saved.summary.appearance.unwrap().accent, "rose");
    apply_pending_at_startup(target.path()).unwrap();
    assert!(!service.is_pending());
    assert_eq!(
        crate::ClientConfig::load(target.path()).unwrap().sub_token,
        "legacy-import-token"
    );
    assert_eq!(service.restored().unwrap().unwrap().version, 1);
    service.acknowledge().unwrap();
    service.acknowledge().unwrap();
    assert!(service.restored().unwrap().is_none());
    let preview = service.inspect(&rollback, PASSWORD.into()).unwrap();
    service
        .take_preview(&preview.preview_id)
        .unwrap()
        .prepare(target.path(), None)
        .unwrap();
    apply_pending_at_startup(target.path()).unwrap();
    assert_eq!(
        crate::ClientConfig::load(target.path()).unwrap().sub_token,
        "previous-target-token"
    );
}

#[test]
fn preview_replacement_and_timed_discard_are_bound_to_the_current_token() {
    let source = configured("preview-token");
    let service = BackupService::new(source.path().into());
    let encrypted = service.export(PASSWORD, None).unwrap();
    let first = service.inspect(&encrypted, PASSWORD.into()).unwrap();
    let second = service.inspect(&encrypted, PASSWORD.into()).unwrap();
    assert!(service.take_preview(&first.preview_id).is_err());
    // A scheduled expiry for an older preview must not discard its replacement.
    service.discard(&first.preview_id);
    assert!(service.take_preview(&second.preview_id).is_ok());
    assert!(service.take_preview(&second.preview_id).is_err());
    let third = service.inspect(&encrypted, PASSWORD.into()).unwrap();
    service.discard(&third.preview_id);
    assert!(service.take_preview(&third.preview_id).is_err());
}

#[test]
fn failed_inspections_revoke_preview_without_touching_saved_configuration() {
    let source = configured("incoming-token");
    let encrypted = BackupService::new(source.path().into())
        .export(PASSWORD, None)
        .unwrap();
    let target = configured("unchanged-target-token");
    let service = BackupService::new(target.path().into());
    let preview = service.inspect(&encrypted, PASSWORD.into()).unwrap();
    assert!(
        service
            .inspect(&encrypted, "wrong password".into())
            .is_err()
    );
    assert!(service.take_preview(&preview.preview_id).is_err());
    let mut damaged = encrypted;
    let last = damaged.len() - 1;
    damaged[last] ^= 1;
    assert!(service.inspect(&damaged, PASSWORD.into()).is_err());
    assert!(service.inspect(b"not an archive", PASSWORD.into()).is_err());
    assert_eq!(
        crate::ClientConfig::load(target.path()).unwrap().sub_token,
        "unchanged-target-token"
    );
    assert!(!service.is_pending());
    assert!(!target.path().join("backup-history").exists());
    assert!(service.restored().unwrap().is_none());
}

#[test]
fn history_selects_newest_ciphertext_and_rejects_oversized_files() {
    let dir = tempfile::tempdir().unwrap();
    let history = dir.path().join("backup-history");
    std::fs::create_dir(&history).unwrap();
    let service = BackupService::new(dir.path().into());
    assert!(service.previous().unwrap().is_none());
    let old = history.join("old.zip.age");
    let new = history.join("new.zip.age");
    std::fs::write(&old, b"older ciphertext").unwrap();
    std::fs::write(&new, b"newer ciphertext").unwrap();
    File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1))
        .unwrap();
    File::options()
        .write(true)
        .open(&new)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(2))
        .unwrap();
    std::fs::write(history.join("ignored.txt"), b"not a backup").unwrap();
    std::fs::create_dir(history.join("ignored.zip.age")).unwrap();
    assert_eq!(service.previous().unwrap().unwrap(), b"newer ciphertext");
    File::options()
        .write(true)
        .open(new)
        .unwrap()
        .set_len((MAX_ARCHIVE + 1) as u64)
        .unwrap();
    assert!(service.previous().is_err());
}

#[test]
fn unreadable_history_and_invalid_startup_results_are_errors_not_absence() {
    let dir = configured("original-token");
    let service = BackupService::new(dir.path().into());
    std::fs::write(dir.path().join("backup-history"), b"not a directory").unwrap();
    assert!(service.previous().is_err());
    std::fs::write(dir.path().join(".backup-applied.json"), b"invalid JSON").unwrap();
    assert!(service.restored().is_err());
    service.acknowledge().unwrap();
    std::fs::create_dir(dir.path().join(".backup-pending")).unwrap();
    std::fs::write(dir.path().join(".backup-pending/manifest.json"), b"invalid").unwrap();
    apply_pending_at_startup(dir.path()).unwrap();
    assert!(service.restored().is_err());
    assert_eq!(
        crate::ClientConfig::load(dir.path()).unwrap().sub_token,
        "original-token"
    );
    service.acknowledge().unwrap();
    assert!(service.restored().unwrap().is_none());
}

#[test]
fn inspection_errors_never_disclose_decrypted_configuration_values() {
    let source = configured("private-source");
    let mut snapshot = capture(source.path(), None).unwrap();
    let secret = "credential-private-value-must-not-reach-error-reports";
    snapshot.files.insert(
        "client.json".into(),
        serde_json::to_vec(&serde_json::json!({"request_proxy_mode": secret})).unwrap(),
    );
    let encrypted = encode(&snapshot, PASSWORD).unwrap();
    let target = configured("unchanged-target");
    let service = BackupService::new(target.path().into());
    let error = service
        .inspect(&encrypted, PASSWORD.into())
        .err()
        .unwrap()
        .to_string();
    assert!(!error.contains(secret));
    assert_eq!(
        crate::ClientConfig::load(target.path()).unwrap().sub_token,
        "unchanged-target"
    );
    assert!(!service.is_pending());
}

#[test]
fn successful_retry_replaces_previous_failure_without_acknowledgement() {
    let target = configured("previous-token");
    std::fs::create_dir(target.path().join(".backup-pending")).unwrap();
    std::fs::write(
        target.path().join(".backup-pending/manifest.json"),
        b"invalid",
    )
    .unwrap();
    apply_pending_at_startup(target.path()).unwrap();
    let service = BackupService::new(target.path().into());
    assert!(service.restored().is_err());
    let source = configured("retried-token");
    let encrypted = BackupService::new(source.path().into())
        .export(PASSWORD, Some(appearance("rose")))
        .unwrap();
    let preview = service.inspect(&encrypted, PASSWORD.into()).unwrap();
    service
        .take_preview(&preview.preview_id)
        .unwrap()
        .prepare(target.path(), None)
        .unwrap();
    apply_pending_at_startup(target.path()).unwrap();
    assert_eq!(
        crate::ClientConfig::load(target.path()).unwrap().sub_token,
        "retried-token"
    );
    assert_eq!(
        service
            .restored()
            .unwrap()
            .unwrap()
            .appearance
            .unwrap()
            .accent,
        "rose"
    );
    assert!(!target.path().join(".backup-failed").exists());
}
