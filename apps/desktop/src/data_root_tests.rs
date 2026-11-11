use super::*;
use std::process::Command;

fn entries(path: &Path) -> Vec<std::ffi::OsString> {
    let mut names: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    names.sort();
    names
}

#[test]
fn absent_and_empty_roots_become_canonical_persistent_owned_directories() {
    let temporary = tempfile::tempdir().unwrap();
    for path in [
        temporary.path().join("new/nested"),
        temporary.path().join("empty"),
    ] {
        if path.file_name().unwrap() == "empty" {
            fs::create_dir(&path).unwrap();
        }
        let root = DataRoot::open(&path).unwrap();
        assert_eq!(root.path(), path.canonicalize().unwrap());
        assert!(root.path().is_absolute());
        assert_eq!(fs::read(path.join(OWNER_FILE)).unwrap(), OWNER_CONTENT);
        assert!(path.join(LOCK_FILE).is_file());
        assert!(path.join("logs").is_dir());
        assert!(path.join("cache").is_dir());
        fs::write(path.join("subscriptions.json"), b"saved source").unwrap();
        drop(root);
        let reopened = DataRoot::open(&path).unwrap();
        assert_eq!(
            fs::read(reopened.path().join("subscriptions.json")).unwrap(),
            b"saved source"
        );
    }
}

#[test]
fn foreign_nonempty_directories_are_unchanged() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    fs::write(root.join("foreign.json"), b"foreign data").unwrap();
    let before = entries(root);
    assert!(DataRoot::open(root).is_err());
    assert_eq!(entries(root), before);
    assert_eq!(
        fs::read(root.join("foreign.json")).unwrap(),
        b"foreign data"
    );
}

#[test]
fn incorrect_marker_and_marker_directories_do_not_authorize_writes() {
    for content in [
        b"".as_slice(),
        b"another application",
        b"lanpath-desktop-nsh-data-root-v2\n",
    ] {
        let temporary = tempfile::tempdir().unwrap();
        fs::write(temporary.path().join(OWNER_FILE), content).unwrap();
        let before = entries(temporary.path());
        assert!(DataRoot::open(temporary.path()).is_err());
        assert_eq!(entries(temporary.path()), before);
        assert_eq!(
            fs::read(temporary.path().join(OWNER_FILE)).unwrap(),
            content
        );
    }
    let temporary = tempfile::tempdir().unwrap();
    fs::create_dir(temporary.path().join(OWNER_FILE)).unwrap();
    assert!(DataRoot::open(temporary.path()).is_err());
    assert_eq!(
        entries(temporary.path()),
        vec![std::ffi::OsString::from(OWNER_FILE)]
    );
}

#[test]
fn production_and_sibling_app_ancestors_are_rejected_before_creating_descendants() {
    for production in [
        ".lanpath",
        ".LANPATH",
        ".proxy-panel-client",
        "com.ybakiame.lanpath",
        "LanpathGPUI",
        "lanpathgpui",
        "com.ybakiame.lanpath.nsh",
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let production = temporary.path().join(production);
        let target = production.join("desktop-nsh/child");
        assert!(DataRoot::open(&target).is_err());
        assert!(!production.exists());
    }
    for marker in ["client.json", "lanpath.portable"] {
        let temporary = tempfile::tempdir().unwrap();
        // 故意损坏的业务数据：准入只使用元数据，绝不解析或迁移生产配置。
        fs::write(temporary.path().join(marker), b"do not read or rewrite").unwrap();
        let before = entries(temporary.path());
        assert!(DataRoot::open(&temporary.path().join("new/child")).is_err());
        assert_eq!(entries(temporary.path()), before);
        assert_eq!(
            fs::read(temporary.path().join(marker)).unwrap(),
            b"do not read or rewrite"
        );
    }
}

#[test]
fn portable_layout_without_marker_is_still_production() {
    let temporary = tempfile::tempdir().unwrap();
    fs::write(temporary.path().join("Lanpath.exe"), b"binary").unwrap();
    fs::create_dir(temporary.path().join("data")).unwrap();
    fs::create_dir(temporary.path().join("seed")).unwrap();
    let before = entries(temporary.path());
    assert!(DataRoot::open(&temporary.path().join("data/desktop-nsh")).is_err());
    assert_eq!(entries(temporary.path()), before);
    assert!(entries(&temporary.path().join("data")).is_empty());
}

#[test]
fn correct_desktop_marker_does_not_override_production_ancestry() {
    let temporary = tempfile::tempdir().unwrap();
    let target = temporary.path().join(".lanpath/desktop-nsh");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join(OWNER_FILE), OWNER_CONTENT).unwrap();
    let before = entries(&target);
    assert!(DataRoot::open(&target).is_err());
    assert_eq!(entries(&target), before);
}

#[test]
fn lock_is_real_exclusive_and_released_without_replacing_the_file() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path();
    let first = DataRoot::open(path).unwrap();
    let lock_path = path.join(LOCK_FILE);
    let contender = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    assert!(matches!(
        contender.try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    assert!(DataRoot::open(path).is_err());
    #[cfg(unix)]
    let original_inode = {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(&lock_path).unwrap().ino()
    };
    drop(first);
    contender.try_lock().unwrap();
    assert!(DataRoot::open(path).is_err());
    drop(contender);
    let reopened = DataRoot::open(path).unwrap();
    assert_eq!(reopened.path(), path.canonicalize().unwrap());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(fs::metadata(lock_path).unwrap().ino(), original_inode);
    }
}

#[test]
fn missing_lock_is_not_recreated_in_an_owned_directory() {
    let temporary = tempfile::tempdir().unwrap();
    let root = DataRoot::open(temporary.path()).unwrap();
    drop(root);
    fs::remove_file(temporary.path().join(LOCK_FILE)).unwrap();
    let before = entries(temporary.path());
    assert!(DataRoot::open(temporary.path()).is_err());
    assert_eq!(entries(temporary.path()), before);
    assert!(!temporary.path().join(LOCK_FILE).exists());
}

#[test]
fn contenders_do_not_create_logs_or_cache_before_acquiring_the_lock() {
    let temporary = tempfile::tempdir().unwrap();
    let root = DataRoot::open(temporary.path()).unwrap();
    fs::remove_dir(temporary.path().join("logs")).unwrap();
    fs::remove_dir(temporary.path().join("cache")).unwrap();
    assert!(DataRoot::open(temporary.path()).is_err());
    assert!(!temporary.path().join("logs").exists());
    assert!(!temporary.path().join("cache").exists());
    drop(root);
    let _root = DataRoot::open(temporary.path()).unwrap();
    assert!(temporary.path().join("logs").is_dir());
    assert!(temporary.path().join("cache").is_dir());
}

#[test]
fn stored_subscription_files_are_preserved_on_reopen() {
    let temporary = tempfile::tempdir().unwrap();
    let root = DataRoot::open(temporary.path()).unwrap();
    let names = ["subscriptions.json", "cache/subscription_cache/a.json"];
    for name in names {
        let path = root.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, name.as_bytes()).unwrap();
    }
    drop(root);
    let root = DataRoot::open(temporary.path()).unwrap();
    for name in names {
        assert_eq!(fs::read(root.path().join(name)).unwrap(), name.as_bytes());
    }
}

#[test]
fn empty_and_parent_traversal_paths_are_rejected() {
    assert!(DataRoot::open(Path::new("")).is_err());
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("not-created/../escape");
    assert!(DataRoot::open(&path).is_err());
    assert!(entries(temporary.path()).is_empty());
}

#[test]
fn hardlinked_ownership_and_subscription_files_are_rejected_without_touching_the_target() {
    let outside = tempfile::tempdir().unwrap();
    let external_file = outside.path().join("protected.json");
    fs::write(&external_file, OWNER_CONTENT).unwrap();
    for name in [
        OWNER_FILE,
        LOCK_FILE,
        "subscriptions.json",
        "cache/nested/resource",
    ] {
        let temporary = tempfile::tempdir().unwrap();
        drop(DataRoot::open(temporary.path()).unwrap());
        let linked = temporary.path().join(name);
        fs::create_dir_all(linked.parent().unwrap()).unwrap();
        if linked.exists() {
            fs::remove_file(&linked).unwrap();
        }
        fs::hard_link(&external_file, linked).unwrap();
        let before = entries(temporary.path());
        assert!(
            DataRoot::open(temporary.path()).is_err(),
            "accepted hard link at {name}"
        );
        assert_eq!(entries(temporary.path()), before);
        assert_eq!(fs::read(&external_file).unwrap(), OWNER_CONTENT);
    }
}

#[cfg(unix)]
#[test]
fn root_and_ancestor_symlinks_never_claim_their_targets() {
    use std::os::unix::fs::symlink;
    let temporary = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let link = temporary.path().join("alias");
    symlink(outside.path(), &link).unwrap();
    assert!(DataRoot::open(&link).is_err());
    assert!(DataRoot::open(&link.join("new/child")).is_err());
    assert!(entries(outside.path()).is_empty());
}

#[cfg(unix)]
#[test]
fn linked_markers_locks_and_subscription_files_are_rejected() {
    use std::os::unix::fs::symlink;
    let outside = tempfile::tempdir().unwrap();
    let external_file = outside.path().join("production-file");
    fs::write(&external_file, OWNER_CONTENT).unwrap();
    for name in [OWNER_FILE, LOCK_FILE, "subscriptions.json", "logs"] {
        let temporary = tempfile::tempdir().unwrap();
        let root = DataRoot::open(temporary.path()).unwrap();
        drop(root);
        let linked = temporary.path().join(name);
        if linked.is_dir() {
            fs::remove_dir(&linked).unwrap();
            symlink(outside.path(), &linked).unwrap();
        } else {
            if linked.exists() {
                fs::remove_file(&linked).unwrap();
            }
            symlink(&external_file, &linked).unwrap();
        }
        let before = entries(temporary.path());
        assert!(
            DataRoot::open(temporary.path()).is_err(),
            "accepted link at {name}"
        );
        assert_eq!(entries(temporary.path()), before);
        assert_eq!(fs::read(&external_file).unwrap(), OWNER_CONTENT);
    }
}

#[cfg(unix)]
#[test]
fn nested_links_and_hardlinked_regular_files_cannot_escape_isolation() {
    use std::os::unix::fs::symlink;
    let outside = tempfile::tempdir().unwrap();
    let external_file = outside.path().join("production-file");
    fs::write(&external_file, b"untouched").unwrap();
    for hard_link in [false, true] {
        let temporary = tempfile::tempdir().unwrap();
        let root = DataRoot::open(temporary.path()).unwrap();
        drop(root);
        fs::create_dir(temporary.path().join("cache/nested")).unwrap();
        let linked = temporary.path().join("cache/nested/linked");
        if hard_link {
            fs::hard_link(&external_file, linked).unwrap();
        } else {
            symlink(outside.path(), linked).unwrap();
        }
        assert!(DataRoot::open(temporary.path()).is_err());
        assert_eq!(fs::read(&external_file).unwrap(), b"untouched");
    }
}

#[test]
fn persistent_environment_paths_must_be_absolute() {
    assert!(absolute_environment_directory("TEST", "".into()).is_err());
    assert!(absolute_environment_directory("TEST", "relative/path".into()).is_err());
    let temporary = tempfile::tempdir().unwrap();
    assert_eq!(
        absolute_environment_directory("TEST", temporary.path().as_os_str().into()).unwrap(),
        temporary.path()
    );
}

#[test]
fn explicit_location_rejects_empty_and_parent_traversal_without_fallback() {
    assert!(DataLocation::resolve(Some(Path::new(""))).is_err());
    let temporary = tempfile::tempdir().unwrap();
    let escape = temporary.path().join("not-created/../escape");
    assert!(DataLocation::resolve(Some(&escape)).is_err());
    assert!(DataLocation::resolve(Some(temporary.path())).is_ok());
}

#[test]
fn another_process_cannot_open_a_held_root() {
    let temporary = tempfile::tempdir().unwrap();
    let _root = DataRoot::open(temporary.path()).unwrap();
    let witness_directory = tempfile::tempdir().unwrap();
    let witness = witness_directory.path().join("child-confirmed");
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "data_root::tests::lock_contention_child_process",
            "--ignored",
        ])
        .env("LANPATH_DATA_ROOT_LOCK_TEST", temporary.path())
        .env("LANPATH_DATA_ROOT_LOCK_WITNESS", &witness)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(witness).unwrap(), b"contention refused");
}

#[test]
#[ignore = "invoked with a real held directory by another_process_cannot_open_a_held_root"]
fn lock_contention_child_process() {
    let path =
        std::env::var_os("LANPATH_DATA_ROOT_LOCK_TEST").expect("parent supplies the held root");
    assert!(DataRoot::open(Path::new(&path)).is_err());
    let witness = std::env::var_os("LANPATH_DATA_ROOT_LOCK_WITNESS")
        .expect("parent supplies an external witness path");
    fs::write(witness, b"contention refused").unwrap();
}
