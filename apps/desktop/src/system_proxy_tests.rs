//! 系统代理偏好的持久化与配置合并测试：保存只改 `system_proxy_enabled`
//! 一个字段，绝不覆盖无关配置；fixture 不触碰 Windows 真实 WinINet。

use super::*;
use lanpath_client::{ClientConfig, RequestProxyMode};

fn open_root() -> (tempfile::TempDir, std::sync::Arc<DataRoot>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = crate::data_root::open_root(dir.path()).expect("open root");
    (dir, root)
}

#[test]
fn preference_roundtrips_through_saved_config() {
    let (_dir, root) = open_root();
    assert!(!load_preference(&root).expect("load default"));
    save_preference(&root, true).expect("save on");
    assert!(load_preference(&root).expect("reload"));
    save_preference(&root, false).expect("save off");
    assert!(!load_preference(&root).expect("reload off"));
}

#[test]
fn saving_preference_preserves_unrelated_fields() {
    let (_dir, root) = open_root();
    // 先由其他业务写入无关字段：active 订阅选择、端口、请求代理模式、MITM。
    let mut config: ClientConfig =
        crate::request_settings::load_request_config(root.path()).expect("load config");
    config.active_subscription_id = Some(uuid::Uuid::new_v4());
    config.mixed_port = 18_211;
    config.clash_api_port = 18_212;
    config.request_proxy_mode = RequestProxyMode::Custom;
    config.request_proxy_url = "http://127.0.0.1:8899".into();
    config.mitm_enabled = true;
    config.core_binary = std::path::PathBuf::from("/tmp/not-a-real-core");
    config.save().expect("save unrelated fields");

    save_preference(&root, true).expect("save preference");

    let merged = crate::request_settings::load_request_config(root.path()).expect("reload merged");
    assert!(merged.system_proxy_enabled);
    assert_eq!(merged.active_subscription_id, config.active_subscription_id);
    assert_eq!(merged.mixed_port, 18_211);
    assert_eq!(merged.clash_api_port, 18_212);
    assert_eq!(merged.request_proxy_mode, RequestProxyMode::Custom);
    assert_eq!(merged.request_proxy_url, "http://127.0.0.1:8899");
    assert!(merged.mitm_enabled);
    assert_eq!(
        merged.core_binary,
        std::path::PathBuf::from("/tmp/not-a-real-core")
    );
}

#[test]
fn corrupt_config_is_not_overwritten_by_preference_save() {
    let (_dir, root) = open_root();
    let path = root.path().join("app.toml");
    std::fs::write(&path, b"not = [valid toml").expect("corrupt app.toml");
    let error = save_preference(&root, true).expect_err("corrupt config must fail");
    assert!(error.to_string().contains("app.toml") || !error.to_string().is_empty());
    // 失败不重写用户文件：字节原样保留。
    assert_eq!(std::fs::read(&path).unwrap(), b"not = [valid toml");
}

#[cfg(not(windows))]
#[test]
fn unsupported_spawner_is_explicit_and_safe() {
    let spawner = UnsupportedGuardianSpawner;
    assert!(!spawner.supported());
    assert!(spawner.arm("owner").is_err());
    assert!(spawner.ownership("owner").is_err());
    assert!(spawner.has_recovery_for_owner("owner").is_err());
}
