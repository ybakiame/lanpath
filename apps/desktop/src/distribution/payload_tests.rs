use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use roots::DataMode;

const MODES: [DataMode; 2] = [DataMode::Standard, DataMode::Portable];

// Synthetic, never executable bytes and an in-memory fixture signing identity.
// These tests launch nothing, inspect no processes, and touch only TempDir roots.
fn bundle(root: &Path, version: &str) -> String {
    std::fs::create_dir_all(root).unwrap();
    let key = SigningKey::from_bytes(&[47; 32]);
    let mut entries = Vec::new();
    for name in crate::product::TRUSTED_BUNDLE_FILES {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("never-launched fixture {version} {name}\n")).unwrap();
        let (size, sha256) = signature::digest(&path).unwrap();
        entries.push(signature::BundleFile {
            path: (*name).into(),
            size,
            sha256,
        });
    }
    let app = entries
        .iter()
        .find(|entry| entry.path == crate::product::BINARY_NAME)
        .unwrap();
    let manifest = Manifest {
        schema: 1,
        product: crate::product::APP_ID.into(),
        channel: crate::product::UPDATE_CHANNEL.into(),
        version: version.into(),
        arch: std::env::consts::ARCH.into(),
        artifact: signature::Artifact {
            url: format!(
                "{}/releases/download/gpui-v{version}/{}",
                crate::product::REPOSITORY,
                crate::product::BINARY_NAME
            ),
            size: app.size,
            sha256: app.sha256.clone(),
        },
        files: entries,
    };
    let payload = serde_json::to_vec(&manifest).unwrap();
    let signature = STANDARD.encode(key.sign(&payload).to_bytes());
    std::fs::write(root.join("bundle.json"), payload).unwrap();
    std::fs::write(root.join("bundle.json.sig"), signature).unwrap();
    STANDARD.encode(key.verifying_key().as_bytes())
}

fn assert_version(root: &Path, expected: &str, key: &str) {
    let payload = files::open_files(root, false).unwrap();
    let signed = SignedBundle::read(&payload).unwrap();
    signed.verify(&payload, key).unwrap();
    assert_eq!(signed.manifest(key).unwrap().version, expected);
}

fn fixture(mode: DataMode) -> (tempfile::TempDir, PathBuf, PathBuf, String) {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("selected-payload");
    let source = temp.path().join("staged-helper");
    let key = bundle(&target, "2.3.4");
    bundle(&source, "2.4.0");
    std::fs::write(target.join("unknown-user-file"), b"preserve me").unwrap();
    // In Standard mode these are unknown neighbors, NOT a data-location request.
    std::fs::create_dir(target.join("data")).unwrap();
    std::fs::write(target.join("data/private.json"), b"private data").unwrap();
    std::fs::write(target.join(".lanpath-gpui"), b"unrelated state").unwrap();
    if mode == DataMode::Portable {
        std::fs::write(target.join(crate::product::PORTABLE_MARKER), b"").unwrap();
    }
    assert_eq!(roots::admit(&target).unwrap().mode, mode);
    (temp, target, source, key)
}

fn assert_preserved(target: &Path, mode: DataMode) {
    assert_eq!(
        std::fs::read(target.join("data/private.json")).unwrap(),
        b"private data"
    );
    assert_eq!(
        std::fs::read(target.join("unknown-user-file")).unwrap(),
        b"preserve me"
    );
    assert_eq!(
        std::fs::read(target.join(".lanpath-gpui")).unwrap(),
        b"unrelated state"
    );
    let marker = target.join(crate::product::PORTABLE_MARKER);
    if mode == DataMode::Portable {
        assert_eq!(std::fs::read(marker).unwrap(), b"");
    } else {
        assert!(!marker.exists());
    }
}

fn payload_bytes(root: &Path) -> Vec<(String, Vec<u8>)> {
    files::paths()
        .filter_map(|name| match std::fs::read(root.join(name)) {
            Ok(bytes) => Some((name.into(), bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => panic!("cannot observe payload {name}: {error}"),
        })
        .collect()
}

fn interrupt(target: &Path, source: &Path, key: &str, after: usize) {
    let stopped = std::panic::catch_unwind(|| {
        let _ = update_with(target, source, key, |index| {
            if index == after {
                panic!("simulated process interruption between atomic replacements");
            }
            Ok(())
        });
    });
    assert!(stopped.is_err());
    assert!(target.join(JOURNAL).is_file());
    assert!(admit_application_at(target).is_err());
}

fn recover(target: &Path, key: &str) -> LanpathResult<Option<SignedBundle>> {
    let admitted = roots::admit(target)?;
    let _lock = lock(&admitted.path)?;
    recover_locked(&admitted, key)
}

fn journal_fixture(target: &Path, source: &Path, phase: Phase) -> Journal {
    let root = roots::admit(target).unwrap();
    Journal {
        schema: 1,
        root: root.path.clone(),
        mode: root.mode,
        identity: root.identity,
        phase,
        previous: SignedBundle::read(&files::open_files(target, false).unwrap()).unwrap(),
        next: SignedBundle::read(&files::open_files(source, false).unwrap()).unwrap(),
    }
}

#[test]
fn later_write_failure_restores_complete_authenticated_previous_payload_and_returns_failure() {
    for mode in MODES {
        let (_temp, target, source, key) = fixture(mode);
        let result = update_with(&target, &source, &key, |index| {
            if index == 2 {
                Err(invalid("fixture later-file replacement failure"))
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        assert_version(&target, "2.3.4", &key);
        assert_preserved(&target, mode);
        assert!(!target.join(JOURNAL).exists());
        assert!(!target.join(BACKUP).exists());
        admit_application_at(&target).unwrap();
    }
}

#[test]
fn every_atomic_boundary_and_interrupted_restore_recovers_both_modes_without_changing_identity() {
    for mode in MODES {
        for after in 0..files::paths().count() {
            let (_temp, target, source, key) = fixture(mode);
            let before = roots::admit(&target).unwrap().identity;
            interrupt(&target, &source, &key, after);
            // A prefix was already restored before a second crash. Prepared is
            // intentionally still on disk and the complete backup still exists.
            {
                let backup = files::open_files(&target.join(BACKUP), false).unwrap();
                let first = files::paths().next().unwrap();
                drop(files::replace(&target, first, &backup[first]).unwrap());
            }
            assert!(recover(&target, &key).unwrap().is_none());
            assert_version(&target, "2.3.4", &key);
            assert_preserved(&target, mode);
            assert!(roots::admit(&target).unwrap().identity == before);
            assert!(!target.join(JOURNAL).exists());
            assert!(!target.join(BACKUP).exists());
            update_with(&target, &source, &key, |_| Ok(())).unwrap();
            assert_version(&target, "2.4.0", &key);
            assert_preserved(&target, mode);
            assert!(roots::admit(&target).unwrap().identity == before);
            assert!(update_with(&target, &source, &key, |_| Ok(())).is_err());
            admit_application_at(&target).unwrap();
        }
    }
}

#[test]
fn tampered_backup_unexpected_target_edit_and_missing_target_refuse_before_any_restoration() {
    for mode in MODES {
        for defect in ["backup", "edit", "missing"] {
            let (_temp, target, source, key) = fixture(mode);
            interrupt(&target, &source, &key, 2);
            let changed = if defect == "backup" {
                target.join(BACKUP).join("LICENSE.txt")
            } else {
                target.join("LICENSE.txt")
            };
            if defect == "missing" {
                std::fs::remove_file(&changed).unwrap();
            } else {
                std::fs::write(&changed, b"unexpected edit must survive refusal").unwrap();
            }
            let before = payload_bytes(&target);
            assert!(recover(&target, &key).is_err());
            assert_eq!(payload_bytes(&target), before);
            if defect != "missing" {
                assert_eq!(
                    std::fs::read(changed).unwrap(),
                    b"unexpected edit must survive refusal"
                );
            }
            assert!(target.join(JOURNAL).exists());
            assert!(target.join(BACKUP).exists());
            assert_preserved(&target, mode);
        }
    }
}

#[test]
fn unknown_backup_files_preserve_committed_state_and_only_exact_pending_retry_can_succeed() {
    for mode in MODES {
        let (_temp, target, source, key) = fixture(mode);
        let extra = target.join(BACKUP).join("unknown-private-file");
        let result = update_with(&target, &source, &key, |index| {
            if index == 0 {
                std::fs::write(&extra, b"do not delete").unwrap();
            }
            Ok(())
        });
        assert!(result.is_err()); // Committed, but cleanup may not claim success.
        assert_version(&target, "2.4.0", &key);
        assert_eq!(std::fs::read(&extra).unwrap(), b"do not delete");
        assert!(recover(&target, &key).is_err());
        assert!(update_with(&target, &source, &key, |_| Ok(())).is_err());
        assert!(admit_application_at(&target).is_err());
        assert_preserved(&target, mode);
        std::fs::remove_file(extra).unwrap(); // Only the fixture owner removes its extra.
        update_with(&target, &source, &key, |_| Ok(())).unwrap();
        assert_version(&target, "2.4.0", &key);
        assert!(update_with(&target, &source, &key, |_| Ok(())).is_err());
        assert!(!target.join(JOURNAL).exists());
        assert!(!target.join(BACKUP).exists());
        admit_application_at(&target).unwrap();
    }
}

#[test]
fn updates_refuse_fresh_missing_incomplete_and_unauthenticated_previous_bundles_without_initialization()
 {
    for mode in MODES {
        for defect in ["absent", "empty", "incomplete", "unauthenticated"] {
            let (_temp, target, source, key) = fixture(mode);
            match defect {
                "absent" => std::fs::remove_dir_all(&target).unwrap(),
                "empty" => {
                    for name in files::paths() {
                        std::fs::remove_file(target.join(name)).unwrap();
                    }
                }
                "incomplete" => std::fs::remove_file(target.join("LICENSE.txt")).unwrap(),
                _ => {
                    std::fs::write(target.join("LICENSE.txt"), b"untrusted previous file").unwrap()
                }
            }
            let before = payload_bytes(&target);
            assert!(update_with(&target, &source, &key, |_| Ok(())).is_err());
            assert_eq!(payload_bytes(&target), before);
            assert!(!target.join(JOURNAL).exists());
            assert!(!target.join(BACKUP).exists());
            if defect == "absent" {
                assert!(!target.exists());
            } else {
                assert_preserved(&target, mode);
            }
        }
    }
}

#[test]
fn all_staging_snapshot_boundaries_and_rolled_back_cleanup_require_only_intact_previous_payload() {
    for mode in MODES {
        for copied in 0..=files::paths().count() {
            let (_temp, target, source, key) = fixture(mode);
            let journal = journal_fixture(&target, &source, Phase::Staging);
            save(&journal.root, &journal).unwrap();
            std::fs::create_dir(target.join(BACKUP)).unwrap();
            for name in files::paths().take(copied) {
                let saved = target.join(BACKUP).join(name);
                std::fs::create_dir_all(saved.parent().unwrap()).unwrap();
                std::fs::copy(target.join(name), saved).unwrap();
            }
            recover(&target, &key).unwrap();
            assert_version(&target, "2.3.4", &key);
            assert_preserved(&target, mode);
            assert!(!target.join(JOURNAL).exists());
            assert!(!target.join(BACKUP).exists());
        }
        let (_temp, target, source, key) = fixture(mode);
        let journal = journal_fixture(&target, &source, Phase::RolledBack);
        save(&journal.root, &journal).unwrap();
        // Crash after rollback was journaled and backup cleanup had already run.
        recover(&target, &key).unwrap();
        assert_version(&target, "2.3.4", &key);
        admit_application_at(&target).unwrap();
    }
}

#[test]
fn final_rename_before_commit_rolls_back_but_committed_cleanup_keeps_the_new_payload() {
    for mode in MODES {
        for (phase, removed) in std::iter::once((Phase::Prepared, 0))
            .chain((0..=files::paths().count()).map(|removed| (Phase::Committed, removed)))
        {
            let (_temp, target, source, key) = fixture(mode);
            let before = roots::admit(&target).unwrap().identity;
            let journal = journal_fixture(&target, &source, phase);
            let target_files = files::open_files(&target, true).unwrap();
            std::fs::create_dir(target.join(BACKUP)).unwrap();
            files::snapshot(&target_files, &target.join(BACKUP)).unwrap();
            {
                let source_files = files::open_files(&source, false).unwrap();
                let mut replacement = Files::new();
                for name in files::paths() {
                    replacement.insert(
                        name.into(),
                        files::replace(&target, name, &source_files[name]).unwrap(),
                    );
                }
                journal.next.verify(&replacement, &key).unwrap();
            }
            drop(target_files);
            save(&journal.root, &journal).unwrap();
            // A committed cleanup may have removed any prefix of the backup.
            // Prepared still needs the complete authenticated previous bundle.
            for name in files::paths().take(removed) {
                std::fs::remove_file(target.join(BACKUP).join(name)).unwrap();
            }
            assert!(admit_application_at(&target).is_err());
            let recovered = recover(&target, &key).unwrap();
            assert_eq!(recovered.is_some(), phase == Phase::Committed);
            assert_version(
                &target,
                if phase == Phase::Committed {
                    "2.4.0"
                } else {
                    "2.3.4"
                },
                &key,
            );
            assert_preserved(&target, mode);
            assert!(roots::admit(&target).unwrap().identity == before);
            assert!(!target.join(JOURNAL).exists());
            assert!(!target.join(BACKUP).exists());
            admit_application_at(&target).unwrap();
        }
    }
}

#[test]
fn unauthenticated_incoming_payload_is_refused_before_creating_a_transaction() {
    for mode in MODES {
        let (_temp, target, source, key) = fixture(mode);
        std::fs::write(source.join("LICENSE.txt"), b"tampered staged file").unwrap();
        assert!(update_with(&target, &source, &key, |_| Ok(())).is_err());
        assert_version(&target, "2.3.4", &key);
        assert_preserved(&target, mode);
        assert!(!target.join(JOURNAL).exists());
        assert!(!target.join(BACKUP).exists());
    }
}

#[test]
fn transaction_version_policy_uses_semver_precedence_not_build_metadata_order() {
    for mode in MODES {
        for (previous, next, advances) in [
            ("1.0.0-beta-foo.2", "1.0.0-beta-foo.10", true),
            ("1.0.0+old", "1.0.0+new", false),
            ("1.0.0-rc.1", "1.0.0", true),
            ("1.0.0-alpha", "1.0.0-alpha.1", true),
            ("1.0.0", "0.9.9", false),
        ] {
            let (_temp, target, source, key) = fixture(mode);
            bundle(&target, previous);
            bundle(&source, next);
            let result = update_with(&target, &source, &key, |_| Ok(()));
            assert_eq!(result.is_ok(), advances);
            assert_version(&target, if advances { next } else { previous }, &key);
            assert_preserved(&target, mode);
        }
    }
}

#[test]
fn mode_schema_required_previous_authentication_and_transition_are_bound_before_recovery_writes() {
    for mode in MODES {
        for defect in [
            "mode",
            "schema",
            "missing_previous",
            "null_previous",
            "previous_signature",
            "next_signature",
            "transition",
            "unknown_field",
        ] {
            let (_temp, target, source, key) = fixture(mode);
            interrupt(&target, &source, &key, 2);
            let path = target.join(JOURNAL);
            let mut journal: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            match defect {
                "mode" => {
                    journal["mode"] = if mode == DataMode::Standard {
                        "portable"
                    } else {
                        "standard"
                    }
                    .into()
                }
                "schema" => journal["schema"] = 2.into(),
                "missing_previous" => {
                    journal.as_object_mut().unwrap().remove("previous");
                }
                "null_previous" => journal["previous"] = serde_json::Value::Null,
                "previous_signature" => {
                    journal["previous"]["signature"] = STANDARD.encode([0; 64]).into()
                }
                "next_signature" => journal["next"]["signature"] = STANDARD.encode([0; 64]).into(),
                "transition" => journal["next"] = journal["previous"].clone(),
                _ => journal["unexpected"] = true.into(),
            }
            std::fs::write(&path, serde_json::to_vec(&journal).unwrap()).unwrap();
            let before = payload_bytes(&target);
            assert!(recover(&target, &key).is_err());
            assert_eq!(payload_bytes(&target), before);
            assert!(target.join(BACKUP).exists());
            assert!(path.exists());
            assert_preserved(&target, mode);
        }
    }
}

#[test]
fn recovery_binds_canonical_path_and_root_object_identity_in_both_modes() {
    for mode in MODES {
        let (temp, target, source, key) = fixture(mode);
        interrupt(&target, &source, &key, 1);
        let moved = temp.path().join("moved-payload");
        std::fs::rename(&target, &moved).unwrap();
        assert!(recover(&moved, &key).is_err());
        assert_preserved(&moved, mode);
        // Restore the exact canonical path and all original children, but not the
        // original directory object: path equality alone must not admit recovery.
        std::fs::create_dir(&target).unwrap();
        for name in files::paths().chain([
            JOURNAL,
            BACKUP,
            LOCK,
            "data",
            ".lanpath-gpui",
            "unknown-user-file",
        ]) {
            let destination = target.join(name);
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::rename(moved.join(name), destination).unwrap();
        }
        if mode == DataMode::Portable {
            std::fs::rename(
                moved.join(crate::product::PORTABLE_MARKER),
                target.join(crate::product::PORTABLE_MARKER),
            )
            .unwrap();
        }
        let before = payload_bytes(&target);
        assert!(recover(&target, &key).is_err());
        assert_eq!(payload_bytes(&target), before);
        assert_preserved(&target, mode);
        assert!(target.join(JOURNAL).exists());
    }
}

#[test]
fn portable_recovery_requires_exact_existing_empty_marker_and_data_identity() {
    for replaced in ["data", crate::product::PORTABLE_MARKER] {
        let (_temp, target, source, key) = fixture(DataMode::Portable);
        interrupt(&target, &source, &key, 1);
        let preserved = target.join(format!("preserved-{replaced}"));
        std::fs::rename(target.join(replaced), &preserved).unwrap();
        if replaced == "data" {
            std::fs::create_dir(target.join(replaced)).unwrap();
        } else {
            std::fs::write(target.join(replaced), b"").unwrap();
        }
        let before = payload_bytes(&target);
        assert!(recover(&target, &key).is_err());
        assert_eq!(payload_bytes(&target), before);
        assert!(preserved.exists());
        assert!(target.join(JOURNAL).exists());
    }
    for defect in ["nonempty_marker", "no_data", "data_file"] {
        let (_temp, target, source, key) = fixture(DataMode::Portable);
        if defect == "nonempty_marker" {
            std::fs::write(target.join(crate::product::PORTABLE_MARKER), b"not empty").unwrap();
        } else {
            std::fs::rename(target.join("data"), target.join("preserved-data")).unwrap();
            if defect == "data_file" {
                std::fs::write(target.join("data"), b"not a directory").unwrap();
            }
        }
        let before = payload_bytes(&target);
        assert!(update_with(&target, &source, &key, |_| Ok(())).is_err());
        assert_eq!(payload_bytes(&target), before);
        assert!(!target.join(JOURNAL).exists());
    }
}

#[test]
fn pending_transactions_cannot_change_data_modes_by_adding_or_removing_the_marker() {
    for mode in MODES {
        let (_temp, target, source, key) = fixture(mode);
        interrupt(&target, &source, &key, 1);
        let marker = target.join(crate::product::PORTABLE_MARKER);
        if mode == DataMode::Standard {
            std::fs::write(&marker, b"").unwrap();
        } else {
            std::fs::rename(&marker, target.join("preserved-marker")).unwrap();
        }
        let before = payload_bytes(&target);
        assert!(recover(&target, &key).is_err());
        assert_eq!(payload_bytes(&target), before);
        assert!(target.join(JOURNAL).exists());
        if mode == DataMode::Standard {
            std::fs::remove_file(&marker).unwrap();
        } else {
            std::fs::rename(target.join("preserved-marker"), marker).unwrap();
        }
        recover(&target, &key).unwrap();
        assert_version(&target, "2.3.4", &key);
        assert_preserved(&target, mode);
    }
}

#[test]
fn standard_marker_appearing_during_update_refuses_further_writes_and_preserves_recovery() {
    for boundary in [0, 2] {
        let (_temp, target, source, key) = fixture(DataMode::Standard);
        let mut expected = payload_bytes(&target);
        let incoming = payload_bytes(&source);
        for (entry, next) in expected.iter_mut().zip(incoming).take(boundary) {
            *entry = next;
        }
        let mut detected = false;
        let result = update_with(&target, &source, &key, |index| {
            if index == boundary {
                detected = true;
                std::fs::write(target.join(crate::product::PORTABLE_MARKER), b"").unwrap();
            }
            Ok(())
        });
        assert!(result.is_err());
        assert!(detected);
        assert_eq!(payload_bytes(&target), expected);
        assert!(recover(&target, &key).is_err());
        assert!(target.join(JOURNAL).exists());
        assert!(target.join(BACKUP).exists());
        assert!(admit_application_at(&target).is_err());
        std::fs::remove_file(target.join(crate::product::PORTABLE_MARKER)).unwrap();
        recover(&target, &key).unwrap();
        assert_version(&target, "2.3.4", &key);
        assert_preserved(&target, DataMode::Standard);
    }
}

#[test]
fn every_legacy_state_namespace_is_preserved_without_adoption_and_blocks_launch_update_and_recovery()
 {
    for mode in MODES {
        for name in LEGACY_STATE {
            let (_temp, target, source, key) = fixture(mode);
            let path = target.join(name);
            if name.ends_with("backup") {
                std::fs::create_dir(&path).unwrap();
                std::fs::write(path.join("unknown-private-file"), b"preserve").unwrap();
            } else {
                std::fs::write(&path, b"legacy state must not be adopted").unwrap();
            }
            assert!(update_with(&target, &source, &key, |_| Ok(())).is_err());
            assert!(recover(&target, &key).is_err());
            assert!(admit_application_at(&target).is_err());
            assert!(path.exists());
            if path.is_dir() {
                assert_eq!(
                    std::fs::read(path.join("unknown-private-file")).unwrap(),
                    b"preserve"
                );
            } else {
                assert_eq!(
                    std::fs::read(path).unwrap(),
                    b"legacy state must not be adopted"
                );
            }
            assert_version(&target, "2.3.4", &key);
            assert_preserved(&target, mode);
            assert!(!target.join(JOURNAL).exists());
            assert!(!target.join(BACKUP).exists());
        }
    }
}

#[test]
fn disjoint_roots_do_not_share_operation_ownership_and_completed_locks_do_not_block_launch() {
    for mode in MODES {
        let (temp, target, source, key) = fixture(mode);
        let other = temp.path().join("independent-payload");
        bundle(&other, "2.3.4");
        if mode == DataMode::Portable {
            std::fs::write(other.join(crate::product::PORTABLE_MARKER), b"").unwrap();
            std::fs::create_dir(other.join("data")).unwrap();
        }
        let selected = roots::admit(&target).unwrap();
        let _selected_lock = lock(&selected.path).unwrap();
        assert!(update_with(&target, &source, &key, |_| Ok(())).is_err());
        update_with(&other, &source, &key, |_| Ok(())).unwrap();
        assert_version(&target, "2.3.4", &key);
        assert_version(&other, "2.4.0", &key);
        assert!(other.join(LOCK).exists());
        admit_application_at(&other).unwrap();
    }
}

#[test]
fn removing_marker_after_a_complete_update_selects_standard_without_migrating_portable_data() {
    let (_temp, target, source, key) = fixture(DataMode::Portable);
    update_with(&target, &source, &key, |_| Ok(())).unwrap();
    std::fs::remove_file(target.join(crate::product::PORTABLE_MARKER)).unwrap();
    assert_eq!(roots::admit(&target).unwrap().mode, DataMode::Standard);
    bundle(&source, "2.5.0");
    update_with(&target, &source, &key, |_| Ok(())).unwrap();
    assert_version(&target, "2.5.0", &key);
    assert_preserved(&target, DataMode::Standard);
    admit_application_at(&target).unwrap();
}

#[test]
fn standard_update_and_recovery_never_initialize_adjacent_data_or_touch_external_profile() {
    let (temp, target, source, key) = fixture(DataMode::Standard);
    let profile = temp.path().join("independent-standard-profile");
    std::fs::rename(target.join("data"), &profile).unwrap();
    interrupt(&target, &source, &key, 2);
    recover(&target, &key).unwrap();
    update_with(&target, &source, &key, |_| Ok(())).unwrap();
    assert_version(&target, "2.4.0", &key);
    assert!(!target.join("data").exists());
    assert!(!target.join(crate::product::PORTABLE_MARKER).exists());
    assert_eq!(
        std::fs::read(profile.join("private.json")).unwrap(),
        b"private data"
    );
    assert_eq!(
        std::fs::read(target.join(".lanpath-gpui")).unwrap(),
        b"unrelated state"
    );
}

#[test]
fn authenticated_external_helpers_and_exact_offer_identity_are_required_in_both_modes() {
    for mode in MODES {
        let (_temp, target, source, key) = fixture(mode);
        let signed = SignedBundle::read(&files::open_files(&source, false).unwrap()).unwrap();
        let identity = signature::bundle_identity(&signed.manifest(&key).unwrap()).unwrap();
        assert!(
            cli::update_from_helper(&target, &source, std::ffi::OsStr::new("wrong-offer"), &key)
                .is_err()
        );
        assert_version(&target, "2.3.4", &key);
        assert!(!target.join(JOURNAL).exists());
        interrupt(&target, &source, &key, 2);
        let before = payload_bytes(&target);
        std::fs::write(
            source.join("LICENSE.txt"),
            b"unauthenticated helper resource",
        )
        .unwrap();
        assert!(cli::recover_from_helper(&target, &source, &key).is_err());
        assert_eq!(payload_bytes(&target), before);
        assert!(target.join(JOURNAL).exists());
        bundle(&source, "2.4.0");
        cli::recover_from_helper(&target, &source, &key).unwrap();
        assert_version(&target, "2.3.4", &key);
        cli::update_from_helper(&target, &source, std::ffi::OsStr::new(&identity), &key).unwrap();
        assert_version(&target, "2.4.0", &key);
        assert_preserved(&target, mode);
    }
}

#[test]
fn update_and_recovery_helpers_must_be_disjoint_from_selected_root_in_both_modes() {
    for mode in MODES {
        for relation in ["same", "nested", "ancestor"] {
            let (temp, target, source, key) = fixture(mode);
            let journal = journal_fixture(&target, &source, Phase::Staging);
            save(&journal.root, &journal).unwrap();
            let helper = match relation {
                "same" => target.clone(),
                "nested" => {
                    let path = target.join("nested-helper");
                    bundle(&path, "2.4.0");
                    path
                }
                _ => {
                    bundle(temp.path(), "2.4.0");
                    temp.path().to_path_buf()
                }
            };
            let before = payload_bytes(&target);
            assert!(update_with(&target, &helper, &key, |_| Ok(())).is_err());
            assert!(cli::recover_from_helper(&target, &helper, &key).is_err());
            assert_eq!(payload_bytes(&target), before);
            assert!(target.join(JOURNAL).exists());
            assert_preserved(&target, mode);
        }
    }
}

#[cfg(unix)]
#[test]
fn selected_recovery_links_and_their_referents_are_never_replaced() {
    for mode in MODES {
        for name in [JOURNAL, BACKUP, LOCK, "LICENSE.txt"] {
            let (temp, target, source, key) = fixture(mode);
            let unrelated = temp.path().join("unrelated-file");
            std::fs::write(&unrelated, b"preserve outside selected payload").unwrap();
            if target.join(name).exists() {
                std::fs::remove_file(target.join(name)).unwrap();
            }
            std::os::unix::fs::symlink(&unrelated, target.join(name)).unwrap();
            assert!(update_with(&target, &source, &key, |_| Ok(())).is_err());
            assert!(
                std::fs::symlink_metadata(target.join(name))
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            assert_eq!(
                std::fs::read(unrelated).unwrap(),
                b"preserve outside selected payload"
            );
            assert_preserved(&target, mode);
        }
    }
}

#[cfg(windows)]
#[test]
fn exact_resource_admission_rejects_busy_selected_executable_and_resource_but_not_unrelated_roots()
{
    use std::os::windows::fs::OpenOptionsExt;
    for mode in MODES {
        for resource in [crate::product::BINARY_NAME, "resources/seed/wintun.dll"] {
            let (temp, target, source, key) = fixture(mode);
            let unrelated = temp.path().join("unrelated-root");
            bundle(&unrelated, "2.3.4");
            let _unrelated_handle = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(unrelated.join(crate::product::BINARY_NAME))
                .unwrap();
            let selected = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(target.join(resource))
                .unwrap();
            assert!(update_with(&target, &source, &key, |_| Ok(())).is_err());
            assert!(!target.join(JOURNAL).exists());
            drop(selected);
            assert_version(&target, "2.3.4", &key);
            update_with(&target, &source, &key, |_| Ok(())).unwrap();
            assert_version(&target, "2.4.0", &key);
            assert_preserved(&target, mode);
        }
    }
}

/// Opt-in consumer proof with genuine, already-signed release bundles. The
/// interruption child exits without unwinding after the first atomic rename;
/// the real signed application then performs recovery and the successful update.
#[cfg(windows)]
#[test]
#[ignore = "requires genuine older/newer signed GPUI bundles on native Windows"]
fn real_signed_windows_upgrade_and_interruption_recovery() {
    const CHILD_TARGET: &str = "LANPATH_TEST_GPUI_INTERRUPT_TARGET";
    const NEW_BUNDLE: &str = "LANPATH_TEST_GPUI_NEW_BUNDLE";
    const TEST: &str =
        "distribution::payload::tests::real_signed_windows_upgrade_and_interruption_recovery";
    let key = crate::product::UPDATE_PUBLIC_KEY;
    let source = PathBuf::from(std::env::var_os(NEW_BUNDLE).expect(NEW_BUNDLE))
        .canonicalize()
        .unwrap();
    if let Some(target) = std::env::var_os(CHILD_TARGET) {
        update_with(Path::new(&target), &source, key, |index| {
            if index == 1 {
                // No destructor rollback: model process death with a mixed
                // authenticated old/new payload and a durable Prepared journal.
                std::process::exit(86);
            }
            Ok(())
        })
        .unwrap();
        panic!("interruption child unexpectedly completed the transaction");
    }
    let previous = PathBuf::from(
        std::env::var_os("LANPATH_TEST_GPUI_OLD_BUNDLE").expect("LANPATH_TEST_GPUI_OLD_BUNDLE"),
    )
    .canonicalize()
    .unwrap();
    let _old_root = roots::admit(&previous).unwrap();
    let _new_root = roots::admit(&source).unwrap();
    let old_files = files::open_files(&previous, false).unwrap();
    let old = SignedBundle::read(&old_files).unwrap();
    old.verify(&old_files, key).unwrap();
    let new_files = files::open_files(&source, false).unwrap();
    let new = SignedBundle::read(&new_files).unwrap();
    new.verify(&new_files, key).unwrap();
    let old_version = old.manifest(key).unwrap().version;
    let new_version = new.manifest(key).unwrap().version;
    assert_eq!(new_version, crate::product::VERSION);
    assert!(
        version(&old, key)
            .unwrap()
            .cmp_precedence(&version(&new, key).unwrap())
            .is_lt()
    );
    assert_ne!(
        old.stamps(key).unwrap()[crate::product::BINARY_NAME],
        new.stamps(key).unwrap()[crate::product::BINARY_NAME],
    );
    let helper = source.join(crate::product::BINARY_NAME);
    let identity = signature::bundle_identity(&new.manifest(key).unwrap()).unwrap();
    for mode in MODES {
        // Only this test's fresh roots are mutated. Neither source release,
        // real user profile, core, GUI nor OS integration is opened/started.
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("selected-payload");
        std::fs::create_dir(&target).unwrap();
        files::snapshot(&old_files, &target).unwrap();
        std::fs::create_dir(target.join("data")).unwrap();
        std::fs::write(target.join("data/private.json"), b"private data").unwrap();
        std::fs::write(target.join("unknown-user-file"), b"preserve me").unwrap();
        std::fs::write(target.join(".lanpath-gpui"), b"unrelated state").unwrap();
        if mode == DataMode::Portable {
            std::fs::write(target.join(crate::product::PORTABLE_MARKER), b"").unwrap();
        }
        let profile = temp.path().join("standard-profile");
        std::fs::create_dir_all(profile.join(crate::product::DATA_DIR_NAME)).unwrap();
        let profile_file = profile
            .join(crate::product::DATA_DIR_NAME)
            .join("private.json");
        std::fs::write(&profile_file, b"independent standard data").unwrap();
        let before = roots::admit(&target).unwrap().identity;
        let interrupted = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", TEST])
            .env(CHILD_TARGET, &target)
            .env(NEW_BUNDLE, &source)
            .env("LOCALAPPDATA", &profile)
            .status()
            .unwrap();
        assert_eq!(interrupted.code(), Some(86));
        let journal: Journal =
            serde_json::from_slice(&std::fs::read(target.join(JOURNAL)).unwrap()).unwrap();
        assert!(journal.phase == Phase::Prepared);
        {
            let mixed = files::open_files(&target, false).unwrap();
            assert_eq!(
                files::stamp(&mixed[crate::product::BINARY_NAME]).unwrap(),
                new.stamps(key).unwrap()[crate::product::BINARY_NAME],
            );
            assert_eq!(SignedBundle::read(&mixed).unwrap().payload, old.payload);
        }
        assert!(admit_application_at(&target).is_err());
        let recovered = std::process::Command::new(&helper)
            .arg("--lanpath-recover-payload")
            .arg(&target)
            .env("LOCALAPPDATA", &profile)
            .status()
            .unwrap();
        assert!(recovered.success());
        assert_version(&target, &old_version, key);
        assert_preserved(&target, mode);
        assert_eq!(
            std::fs::read(&profile_file).unwrap(),
            b"independent standard data"
        );
        assert!(!target.join(JOURNAL).exists());
        assert!(!target.join(BACKUP).exists());
        admit_application_at(&target).unwrap();
        let upgraded = std::process::Command::new(&helper)
            .arg("--lanpath-update-payload")
            .arg(&target)
            .arg(&identity)
            .env("LOCALAPPDATA", &profile)
            .status()
            .unwrap();
        assert!(upgraded.success());
        assert_version(&target, &new_version, key);
        assert_preserved(&target, mode);
        assert_eq!(
            std::fs::read(&profile_file).unwrap(),
            b"independent standard data"
        );
        assert!(roots::admit(&target).unwrap().identity == before);
        assert!(!target.join(JOURNAL).exists());
        assert!(!target.join(BACKUP).exists());
        admit_application_at(&target).unwrap();
    }
}
