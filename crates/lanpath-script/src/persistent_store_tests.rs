use super::*;
use std::path::Path;
use std::sync::{Arc, Barrier};

fn store(root: &Path) -> FilePersistentStore {
    FilePersistentStore::new(root.to_path_buf())
}

fn legacy(root: &Path, scope: &str, entries: &[(&str, &str)]) -> PathBuf {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(scope.as_bytes());
    let name: String = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let path = root.join(format!("{name}.json"));
    let entries: BTreeMap<_, _> = entries.iter().copied().collect();
    std::fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "scope": scope,
            "entries": entries,
        }))
        .unwrap(),
    )
    .unwrap();
    path
}

fn state(root: &Path) -> StoreFile {
    serde_json::from_slice(&std::fs::read(root.join(STATE_FILE)).unwrap()).unwrap()
}

#[test]
fn live_instances_preserve_each_others_writes_and_replacements() {
    let root = tempfile::tempdir().unwrap();
    let first = store(root.path());
    let second = store(root.path());
    assert_eq!(second.read(SHARED_SCOPE, "token").unwrap(), None);
    first.write(SHARED_SCOPE, "token", "old").unwrap();
    assert_eq!(
        second.read(SHARED_SCOPE, "token").unwrap().as_deref(),
        Some("old")
    );
    second.write(SHARED_SCOPE, "other", "kept").unwrap();
    first.write(SHARED_SCOPE, "token", "new").unwrap();
    assert_eq!(
        second.read(SHARED_SCOPE, "token").unwrap().as_deref(),
        Some("new")
    );
    drop(first);
    drop(second);
    let restarted = store(root.path());
    assert_eq!(
        restarted.read(SHARED_SCOPE, "token").unwrap().as_deref(),
        Some("new")
    );
    assert_eq!(
        restarted.read(SHARED_SCOPE, "other").unwrap().as_deref(),
        Some("kept")
    );
}

#[test]
fn concurrent_independent_instances_do_not_lose_keys() {
    let root = tempfile::tempdir().unwrap();
    let barrier = Arc::new(Barrier::new(6));
    let handles: Vec<_> = (0..6)
        .map(|worker| {
            let root = root.path().to_path_buf();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let writer = FilePersistentStore::new(root);
                barrier.wait();
                for index in 0..12 {
                    writer
                        .write(SHARED_SCOPE, &format!("{worker}:{index}"), "value")
                        .unwrap();
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let restarted = store(root.path());
    for worker in 0..6 {
        for index in 0..12 {
            assert_eq!(
                restarted
                    .read(SHARED_SCOPE, &format!("{worker}:{index}"))
                    .unwrap()
                    .as_deref(),
                Some("value")
            );
        }
    }
    assert_eq!(state(root.path()).scopes[SHARED_SCOPE].len(), 72);
}

#[test]
fn independent_roots_and_private_default_slots_are_isolated() {
    let first_root = tempfile::tempdir().unwrap();
    let second_root = tempfile::tempdir().unwrap();
    let first = store(first_root.path());
    let second = store(second_root.path());
    first.write(SHARED_SCOPE, "key", "first root").unwrap();
    second.write(SHARED_SCOPE, "key", "second root").unwrap();
    first
        .write(
            "script:https://example.test/a.js?x=1:中文",
            DEFAULT_KEY,
            "a",
        )
        .unwrap();
    first
        .write("script:https://example.test/b.js", DEFAULT_KEY, "b")
        .unwrap();
    first.write(SHARED_SCOPE, DEFAULT_KEY, "explicit").unwrap();
    assert_eq!(
        second.read(SHARED_SCOPE, "key").unwrap().as_deref(),
        Some("second root")
    );
    assert_eq!(
        second
            .read("script:https://example.test/b.js", DEFAULT_KEY)
            .unwrap(),
        None
    );
    let restarted = store(first_root.path());
    assert_eq!(
        restarted.read(SHARED_SCOPE, "key").unwrap().as_deref(),
        Some("first root")
    );
    assert_eq!(
        restarted
            .read("script:https://example.test/a.js?x=1:中文", DEFAULT_KEY)
            .unwrap()
            .as_deref(),
        Some("a")
    );
    assert_eq!(
        restarted
            .read("script:https://example.test/b.js", DEFAULT_KEY)
            .unwrap()
            .as_deref(),
        Some("b")
    );
    assert_eq!(
        restarted
            .read(SHARED_SCOPE, DEFAULT_KEY)
            .unwrap()
            .as_deref(),
        Some("explicit")
    );
}

#[test]
fn erase_clear_and_reset_are_visible_to_live_and_restarted_instances() {
    let root = tempfile::tempdir().unwrap();
    let first = store(root.path());
    let second = store(root.path());
    first.write(SHARED_SCOPE, "a", "1").unwrap();
    first.write(SHARED_SCOPE, "b", "2").unwrap();
    first
        .write("script:private", DEFAULT_KEY, "private")
        .unwrap();
    assert!(second.erase(SHARED_SCOPE, "a").unwrap());
    assert!(!first.erase(SHARED_SCOPE, "a").unwrap());
    assert_eq!(first.read(SHARED_SCOPE, "a").unwrap(), None);
    second.clear(SHARED_SCOPE).unwrap();
    assert_eq!(first.read(SHARED_SCOPE, "b").unwrap(), None);
    assert_eq!(
        first
            .read("script:private", DEFAULT_KEY)
            .unwrap()
            .as_deref(),
        Some("private")
    );
    first.write(SHARED_SCOPE, "new", "3").unwrap();
    second.clear_all().unwrap();
    assert_eq!(first.read("script:private", DEFAULT_KEY).unwrap(), None);
    first.write(SHARED_SCOPE, "after-reset", "4").unwrap();
    let restarted = store(root.path());
    assert_eq!(restarted.read(SHARED_SCOPE, "new").unwrap(), None);
    assert_eq!(
        restarted
            .read(SHARED_SCOPE, "after-reset")
            .unwrap()
            .as_deref(),
        Some("4")
    );
}

#[test]
fn migration_merges_explicit_keys_preserves_defaults_and_original_files() {
    let root = tempfile::tempdir().unwrap();
    let prefs = legacy(
        root.path(),
        "prefs:source-a",
        &[("token", "same"), (DEFAULT_KEY, "private")],
    );
    let pstore = legacy(
        root.path(),
        "pstore:source-b",
        &[("token", "same"), (DEFAULT_KEY, "explicit-default")],
    );
    let custom = legacy(root.path(), "custom:scope", &[("key", "custom")]);
    let originals: Vec<_> = [&prefs, &pstore, &custom]
        .into_iter()
        .map(|path| (path.clone(), std::fs::read(path).unwrap()))
        .collect();
    let migrated = store(root.path());
    // A read itself commits migration; callers need not perform a write first.
    assert_eq!(
        migrated.read(SHARED_SCOPE, "token").unwrap().as_deref(),
        Some("same")
    );
    assert_eq!(
        migrated
            .read("script:source-a", DEFAULT_KEY)
            .unwrap()
            .as_deref(),
        Some("private")
    );
    assert_eq!(
        migrated.read(SHARED_SCOPE, DEFAULT_KEY).unwrap().as_deref(),
        Some("explicit-default")
    );
    assert_eq!(
        migrated.read("custom:scope", "key").unwrap().as_deref(),
        Some("custom")
    );
    assert_eq!(state(root.path()).migrated_legacy.len(), 3);
    for (path, original) in originals {
        assert_eq!(std::fs::read(path).unwrap(), original);
    }
    migrated.erase(SHARED_SCOPE, "token").unwrap();
    let restarted = store(root.path());
    assert_eq!(restarted.read(SHARED_SCOPE, "token").unwrap(), None);
    restarted.clear_all().unwrap();
    assert_eq!(
        store(root.path())
            .read("script:source-a", DEFAULT_KEY)
            .unwrap(),
        None
    );
    assert_eq!(state(root.path()).migrated_legacy.len(), 3);
}

#[test]
fn conflicting_legacy_values_abort_without_partial_commit_and_can_be_resolved() {
    let root = tempfile::tempdir().unwrap();
    let first = legacy(root.path(), "prefs:a", &[("token", "first")]);
    let second = legacy(root.path(), "pstore:b", &[("token", "second")]);
    let first_bytes = std::fs::read(&first).unwrap();
    let second_bytes = std::fs::read(&second).unwrap();
    let current = store(root.path());
    let error = current
        .write(SHARED_SCOPE, "unrelated", "not committed")
        .unwrap_err();
    assert!(error.to_string().contains("migration conflict"));
    assert!(!root.path().join(STATE_FILE).exists());
    assert_eq!(std::fs::read(&first).unwrap(), first_bytes);
    assert_eq!(std::fs::read(&second).unwrap(), second_bytes);
    legacy(root.path(), "pstore:b", &[("token", "first")]);
    assert_eq!(
        current.read(SHARED_SCOPE, "token").unwrap().as_deref(),
        Some("first")
    );
    assert_eq!(current.read(SHARED_SCOPE, "unrelated").unwrap(), None);
}

#[test]
fn new_legacy_conflict_does_not_overwrite_existing_committed_state() {
    let root = tempfile::tempdir().unwrap();
    let current = store(root.path());
    current.write(SHARED_SCOPE, "token", "current").unwrap();
    let before = std::fs::read(root.path().join(STATE_FILE)).unwrap();
    let legacy_file = legacy(root.path(), "prefs:late", &[("token", "legacy")]);
    assert!(
        current
            .erase(SHARED_SCOPE, "token")
            .unwrap_err()
            .to_string()
            .contains("migration conflict")
    );
    assert_eq!(std::fs::read(root.path().join(STATE_FILE)).unwrap(), before);
    assert!(
        !state(root.path())
            .migrated_legacy
            .contains(legacy_file.file_name().unwrap().to_str().unwrap())
    );
    legacy(root.path(), "prefs:late", &[("token", "current")]);
    assert_eq!(
        current.read(SHARED_SCOPE, "token").unwrap().as_deref(),
        Some("current")
    );
}

#[test]
fn malformed_legacy_data_is_not_silently_skipped_or_acknowledged() {
    let root = tempfile::tempdir().unwrap();
    legacy(root.path(), "pstore:valid", &[("good", "value")]);
    let malformed = root.path().join("broken.json");
    std::fs::write(&malformed, b"{not valid json").unwrap();
    let current = store(root.path());
    assert!(
        current
            .read(SHARED_SCOPE, "good")
            .unwrap_err()
            .to_string()
            .contains("invalid legacy file")
    );
    assert!(current.write(SHARED_SCOPE, "new", "lost").is_err());
    assert!(current.erase(SHARED_SCOPE, "good").is_err());
    assert!(current.clear(SHARED_SCOPE).is_err());
    assert!(current.clear_all().is_err());
    assert!(!root.path().join(STATE_FILE).exists());
    assert_eq!(std::fs::read(malformed).unwrap(), b"{not valid json");
}

#[test]
fn corrupt_or_unsupported_state_fails_all_operations_without_replacing_it() {
    for bytes in [
        b"not json".as_slice(),
        br#"{"version":99,"scopes":{},"migrated_legacy":[]}"#.as_slice(),
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(STATE_FILE);
        std::fs::write(&path, bytes).unwrap();
        let current = store(root.path());
        assert!(current.read(SHARED_SCOPE, "missing").is_err());
        assert!(current.write(SHARED_SCOPE, "key", "value").is_err());
        assert!(current.erase(SHARED_SCOPE, "key").is_err());
        assert!(current.clear(SHARED_SCOPE).is_err());
        assert!(current.clear_all().is_err());
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn filesystem_failures_report_the_failed_stage() {
    let root = tempfile::tempdir().unwrap();
    let not_a_directory = root.path().join("file");
    std::fs::write(&not_a_directory, "original").unwrap();
    assert!(
        store(&not_a_directory)
            .read(SHARED_SCOPE, "key")
            .unwrap_err()
            .to_string()
            .contains("create directory")
    );
    assert_eq!(std::fs::read(&not_a_directory).unwrap(), b"original");

    let lock_root = root.path().join("lock-root");
    std::fs::create_dir_all(lock_root.join(LOCK_FILE)).unwrap();
    assert!(
        store(&lock_root)
            .write(SHARED_SCOPE, "key", "value")
            .unwrap_err()
            .to_string()
            .contains("open lock")
    );
    let state_root = root.path().join("state-root");
    std::fs::create_dir_all(state_root.join(STATE_FILE)).unwrap();
    assert!(
        store(&state_root)
            .erase(SHARED_SCOPE, "key")
            .unwrap_err()
            .to_string()
            .contains("read state")
    );
}

#[test]
fn failed_atomic_replace_preserves_destination_and_cleans_temporary_file() {
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join(STATE_FILE);
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("original"), "preserved").unwrap();
    let current = store(root.path());
    let error = current
        .persist(&StoreFile {
            version: VERSION,
            ..StoreFile::default()
        })
        .unwrap_err();
    assert!(error.to_string().contains("replace state"));
    assert_eq!(
        std::fs::read(destination.join("original")).unwrap(),
        b"preserved"
    );
    assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| {
        entry
            .unwrap()
            .path()
            .extension()
            .and_then(|extension| extension.to_str())
            != Some("tmp")
    }));
}

#[cfg(unix)]
#[test]
fn legacy_symlinks_fail_migration_instead_of_importing_external_files() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let target = legacy(external.path(), "prefs:external", &[("key", "secret")]);
    std::os::unix::fs::symlink(target, root.path().join("linked.json")).unwrap();
    assert!(
        store(root.path())
            .read(SHARED_SCOPE, "key")
            .unwrap_err()
            .to_string()
            .contains("not a regular file")
    );
    assert!(!root.path().join(STATE_FILE).exists());
}

#[cfg(windows)]
#[test]
fn windows_existing_state_is_replaced_and_sharing_violations_are_not_success() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = tempfile::tempdir().unwrap();
    let current = store(root.path());
    current.write(SHARED_SCOPE, "key", "one").unwrap();
    current.write(SHARED_SCOPE, "key", "two").unwrap();
    let path = root.path().join(STATE_FILE);
    let committed = std::fs::read(&path).unwrap();
    // FILE_SHARE_READ | FILE_SHARE_WRITE, deliberately excluding FILE_SHARE_DELETE.
    let held = OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2)
        .open(&path)
        .unwrap();
    assert!(current.write(SHARED_SCOPE, "key", "three").is_err());
    assert!(current.erase(SHARED_SCOPE, "key").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), committed);
    drop(held);
    assert_eq!(
        current.read(SHARED_SCOPE, "key").unwrap().as_deref(),
        Some("two")
    );
    assert!(current.erase(SHARED_SCOPE, "key").unwrap());
    assert_eq!(store(root.path()).read(SHARED_SCOPE, "key").unwrap(), None);
}
