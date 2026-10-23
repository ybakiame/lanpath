use std::fs;

use lanpath_client::backup::{BackupService, MAX_ARCHIVE};

use super::files::{export_destination, read_encrypted, save_encrypted};

#[test]
fn native_file_adapter_round_trips_real_encrypted_backup_without_overwriting() {
    let root = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    lanpath_client::ClientConfig::new(
        root.path().into(),
        "",
        "native-file-token",
        Default::default(),
    )
    .save()
    .unwrap();
    let service = BackupService::new(root.path().to_path_buf());
    let password = "a meaningful test backup password";
    let encrypted = service.export(password, None).unwrap();
    let path = output.path().join("backup.zip.age");
    save_encrypted(&path, root.path(), &encrypted).unwrap();
    let loaded = read_encrypted(&path).unwrap();
    let preview = service.inspect(&loaded, password.into()).unwrap();
    let target = tempfile::tempdir().unwrap();
    service
        .take_preview(&preview.preview_id)
        .unwrap()
        .prepare(target.path(), None)
        .unwrap();
    lanpath_client::backup::apply_pending_at_startup(target.path()).unwrap();
    assert_eq!(
        lanpath_client::ClientConfig::load(target.path())
            .unwrap()
            .sub_token,
        "native-file-token"
    );
    assert!(save_encrypted(&path, root.path(), b"must not replace existing bytes").is_err());
    assert_eq!(fs::read(&path).unwrap(), encrypted);
}

#[test]
fn native_export_cannot_target_application_root_or_descendants() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("nested")).unwrap();
    for path in [
        root.path().join("config.zip.age"),
        root.path().join("nested/core.zip.age"),
    ] {
        assert!(export_destination(&path, root.path()).is_err());
        assert!(save_encrypted(&path, root.path(), b"ciphertext").is_err());
        assert!(!path.exists());
    }
}

#[test]
fn native_import_rejects_oversized_nonregular_and_nonarchive_paths() {
    let directory = tempfile::tempdir().unwrap();
    let oversized = directory.path().join("oversized.zip.age");
    fs::File::create(&oversized)
        .unwrap()
        .set_len((MAX_ARCHIVE + 1) as u64)
        .unwrap();
    assert!(read_encrypted(&oversized).is_err());
    let folder = directory.path().join("folder.zip.age");
    fs::create_dir(&folder).unwrap();
    assert!(read_encrypted(&folder).is_err());
    let text = directory.path().join("plain.json");
    fs::write(&text, b"{}").unwrap();
    assert!(read_encrypted(&text).is_err());
}

#[test]
fn hard_linked_import_and_existing_export_are_never_followed_or_replaced() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let source = outside.path().join("source.zip.age");
    fs::write(&source, b"keep me").unwrap();
    let link = outside.path().join("hard-link.zip.age");
    fs::hard_link(&source, &link).unwrap();
    assert!(read_encrypted(&source).is_err());
    assert!(read_encrypted(&link).is_err());
    assert!(save_encrypted(&link, root.path(), b"replacement").is_err());
    assert_eq!(fs::read(&source).unwrap(), b"keep me");
}

#[cfg(unix)]
#[test]
fn native_file_adapter_rejects_leaf_and_parent_symlinks() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let source = outside.path().join("source.zip.age");
    fs::write(&source, b"keep me").unwrap();
    let link = outside.path().join("linked.zip.age");
    symlink(&source, &link).unwrap();
    assert!(read_encrypted(&link).is_err());
    assert!(save_encrypted(&link, root.path(), b"replacement").is_err());
    let parent_link = outside.path().join("linked-parent");
    symlink(root.path(), &parent_link).unwrap();
    assert!(read_encrypted(&parent_link.join("source.zip.age")).is_err());
    assert!(save_encrypted(&parent_link.join("new.zip.age"), root.path(), b"ciphertext").is_err());
    assert!(!root.path().join("new.zip.age").exists());
    assert_eq!(fs::read(&source).unwrap(), b"keep me");
}
