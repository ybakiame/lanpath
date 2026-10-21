use super::*;

#[test]
fn file_scope_keeps_tauri_contract_and_opts_into_core() {
    let root = tempfile::tempdir().unwrap();
    for name in [
        "app.log",
        "app.log.2026-08-01",
        "libbox.log",
        "core.log",
        "last_start_config.json",
        "export-old.log",
        "other.txt",
    ] {
        fs::write(root.path().join(name), name).unwrap();
    }
    fs::create_dir(root.path().join("app.log.directory")).unwrap();
    let legacy = DiskLogService::new(root.path());
    assert_eq!(
        legacy.list_files().unwrap(),
        [
            "libbox.log",
            "last_start_config.json",
            "app.log.2026-08-01",
            "app.log"
        ]
    );
    assert!(legacy.read_tail("core.log", None).is_err());
    let service = legacy.with_core_log();
    assert!(
        service
            .list_files()
            .unwrap()
            .contains(&"core.log".to_string())
    );
    assert_eq!(service.read_tail("core.log", None).unwrap(), "core.log");
    for name in [
        "",
        "../app.log",
        "app.log/evil",
        "app.log\\evil",
        "app.log:stream",
        "unlisted.log",
    ] {
        assert!(service.read_tail(name, None).is_err(), "{name}");
    }
}

#[test]
fn tail_bounds_bytes_lines_and_handles_missing_or_deleted_files() {
    let root = tempfile::tempdir().unwrap();
    let logs = root.path().join("logs");
    let service = DiskLogService::new(&logs);
    assert!(service.list_files().unwrap().is_empty());
    assert!(service.read_tail("app.log", None).is_err());
    fs::create_dir(&logs).unwrap();
    let content: String = (0..1500).map(|ix| format!("line {ix}\n")).collect();
    fs::write(logs.join("app.log"), content).unwrap();
    assert_eq!(
        service.read_tail("app.log", Some(2)).unwrap(),
        "line 1498\nline 1499"
    );
    assert_eq!(service.read_tail("app.log", Some(0)).unwrap(), "line 1499");
    assert_eq!(
        service
            .read_tail("app.log", Some(5000))
            .unwrap()
            .lines()
            .count(),
        1000
    );
    let mut large = b"head must not appear\n".to_vec();
    large.extend(std::iter::repeat_n(b'x', LOG_TAIL_MAX_BYTES as usize));
    large.extend_from_slice(b"\nend\n");
    fs::write(logs.join("app.log"), large).unwrap();
    let tail = service.read_tail("app.log", None).unwrap();
    assert!(!tail.contains("head must not appear"));
    assert!(tail.len() <= LOG_TAIL_MAX_BYTES as usize);
    assert!(tail.ends_with("end"));
    fs::remove_file(logs.join("app.log")).unwrap();
    assert!(service.read_tail("app.log", None).is_err());
}

#[test]
fn bounded_tail_keeps_complete_boundary_lines_and_discards_partial_unicode_lines() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("app.log");
    fs::write(&path, "old\n中文\nlast\n").unwrap();
    assert_eq!(
        read_tail_bytes(&path, 12).unwrap(),
        "中文\nlast\n".as_bytes()
    );
    fs::write(&path, "prefix中文没有换行").unwrap();
    assert!(read_tail_bytes(&path, 4).unwrap().is_empty());
    fs::write(&path, "prefix中文\nlast\n").unwrap();
    assert_eq!(read_tail_bytes(&path, 8).unwrap(), b"last\n");
}

#[test]
fn export_streams_actual_bytes_in_order_without_exports_or_config() {
    let root = tempfile::tempdir().unwrap();
    let logs = root.path().join("logs");
    fs::create_dir(&logs).unwrap();
    for (name, bytes) in [
        ("app.log.2026-08-02", &b"day2\n"[..]),
        ("app.log.2026-08-01", &b"day1\n"[..]),
        ("core.log", &b"\xffcore\n"[..]),
        ("libbox.log", &b"box\n"[..]),
        ("export-old.log", &b"old"[..]),
        ("last_start_config.json", &b"secret"[..]),
    ] {
        fs::write(logs.join(name), bytes).unwrap();
    }
    let exports = root.path().join("exports");
    let service = DiskLogService::new(&logs).with_core_log();
    let first = service.export_to(&exports).unwrap();
    let second = service.export_to(&exports).unwrap();
    assert_ne!(first, second);
    assert_eq!(first.parent(), Some(exports.as_path()));
    assert_eq!(fs::read(&first).unwrap(), b"day1\nday2\n\xffcore\nbox\n");
    assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
}

#[cfg(unix)]
#[test]
fn linked_files_and_directories_are_never_read_or_exported() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let logs = root.path().join("logs");
    fs::create_dir(&logs).unwrap();
    let outside = root.path().join("outside");
    fs::write(&outside, "outside data").unwrap();
    symlink(&outside, logs.join("app.log")).unwrap();
    fs::hard_link(&outside, logs.join("core.log")).unwrap();
    let service = DiskLogService::new(&logs).with_core_log();
    assert!(service.list_files().unwrap().is_empty());
    assert!(service.read_tail("app.log", None).is_err());
    assert!(service.read_tail("core.log", None).is_err());
    let exported = service.export_to(&root.path().join("exports")).unwrap();
    assert!(fs::read(exported).unwrap().is_empty());
    let linked_dir = root.path().join("linked");
    symlink(&logs, &linked_dir).unwrap();
    assert!(DiskLogService::new(linked_dir).list_files().is_err());
    let export_link = root.path().join("export-link");
    symlink(root.path().join("exports"), &export_link).unwrap();
    assert!(service.export_to(&export_link).is_err());
}
