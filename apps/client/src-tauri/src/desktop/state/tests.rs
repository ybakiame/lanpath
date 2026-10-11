use super::*;

fn fixture() -> PathBuf {
    let directory = std::env::temp_dir().join(format!("lanpath-storage-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

#[test]
fn installed_uses_product_directory_and_preserves_legacy_location() {
    let root = fixture();
    let exe = root.join("Lanpath.exe");
    let first = DesktopStorage::from_paths(&exe, Some(&root)).unwrap();
    assert_eq!(first.data_dir, root.join(".lanpath"));
    assert_eq!(first.webview_dir(), root.join(".lanpath/webview"));
    assert!(!first.portable);
    std::fs::create_dir(root.join(".proxy-panel-client")).unwrap();
    assert_eq!(
        DesktopStorage::from_paths(&exe, Some(&root))
            .unwrap()
            .data_dir,
        root.join(".proxy-panel-client")
    );
    std::fs::create_dir(root.join(".lanpath")).unwrap();
    assert_eq!(
        DesktopStorage::from_paths(&exe, Some(&root))
            .unwrap()
            .data_dir,
        root.join(".lanpath")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(not(target_os = "macos"))]
#[test]
fn portable_is_relative_to_executable_and_needs_no_home() {
    let root = fixture();
    std::fs::write(root.join(PORTABLE_MARKER), "").unwrap();
    let storage = DesktopStorage::from_paths(&root.join("Lanpath.exe"), None).unwrap();
    assert!(storage.portable);
    assert_eq!(storage.data_dir, root.join("data"));
    assert_eq!(storage.webview_dir(), root.join("data/webview"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_home_and_invalid_marker_do_not_fall_back_to_working_directory() {
    let root = fixture();
    let exe = root.join("Lanpath.exe");
    assert!(DesktopStorage::from_paths(&exe, None).is_err());
    std::fs::create_dir(root.join(PORTABLE_MARKER)).unwrap();
    assert!(DesktopStorage::from_paths(&exe, Some(&root)).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
