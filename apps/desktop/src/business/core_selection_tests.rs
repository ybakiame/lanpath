use std::path::{Path, PathBuf};

use super::BusinessService;
use crate::configuration::Bindings;

fn managed_file(root: &Path) -> PathBuf {
    let name = if cfg!(windows) {
        "sing-box.exe"
    } else {
        "sing-box"
    };
    let path = root.join("cores/sing-box/1.2.3").join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"inventory metadata fixture; never executed").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

#[test]
fn business_writes_and_reopen_preserve_managed_core_selection() {
    let root = tempfile::tempdir().unwrap();
    let business = BusinessService::new(root.path(), None, Bindings::new(17899, 19090));
    let selected = managed_file(root.path());
    let mut config = business.client_config().unwrap();
    config.core_binary = selected.clone();
    config.save().unwrap();
    business.set_rule_mode("direct").unwrap();
    let reopened = BusinessService::new(root.path(), None, Bindings::new(17899, 19090));
    let config = reopened.client_config().unwrap();
    assert_eq!(config.core_binary, selected);
    assert_eq!(config.rule_mode, "direct");
}

#[test]
fn explicit_test_override_never_replaces_saved_core_selection() {
    let root = tempfile::tempdir().unwrap();
    let selected = managed_file(root.path());
    let ordinary = BusinessService::new(root.path(), None, Bindings::new(17899, 19090));
    let mut config = ordinary.client_config().unwrap();
    config.core_binary = selected.clone();
    config.save().unwrap();
    let file = root.path().join("client.json");
    let before = std::fs::read(&file).unwrap();
    let override_file = tempfile::NamedTempFile::new().unwrap();
    let overridden = BusinessService::new(
        root.path(),
        Some(override_file.path().to_path_buf()),
        Bindings::new(17899, 19090),
    );
    assert_eq!(overridden.client_config().unwrap().core_binary, selected);
    assert_eq!(std::fs::read(&file).unwrap(), before);
    overridden.set_rule_mode("global").unwrap();
    let reopened = ordinary.client_config().unwrap();
    assert_eq!(reopened.core_binary, selected);
    assert_eq!(reopened.rule_mode, "global");
}
