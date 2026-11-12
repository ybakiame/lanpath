//! 统一 TOML 存储初始化测试：新根初始化默认、已有统一根保留、旧 JSON
//! 测试布局明确拒绝（不迁移、不删除、不静默忽略）。

use lanpath_client::{
    ClientConfig, RequestProxyMode, SubscriptionStore, resource_storage, settings,
};

use crate::data_root::DataRoot;

#[test]
fn fresh_root_initializes_unified_toml_defaults() {
    let temporary = tempfile::tempdir().unwrap();
    let root = DataRoot::open(temporary.path()).unwrap();

    assert!(settings::app_settings_path(root.path()).is_file());
    assert!(
        root.path()
            .join(resource_storage::SUBSCRIPTIONS_FILE)
            .is_file()
    );
    assert!(!root.path().join("client.json").exists());
    assert!(!root.path().join("subscriptions.json").exists());
    // 新装默认：直连、MITM 关闭、data_dir 本根；独立端口 17899/19090
    // （GPUI 实验值，避免与生产 17890/9090 冲突）。
    let config = ClientConfig::load(root.path()).unwrap();
    assert_eq!(config.request_proxy_mode, RequestProxyMode::None);
    assert!(!config.mitm_enabled);
    assert_eq!(config.data_dir, root.path());
    assert_eq!(config.mixed_port, 17899);
    assert_eq!(config.clash_api_port, 19090);
    assert!(
        SubscriptionStore::new(root.path().to_path_buf())
            .load()
            .unwrap()
            .is_empty()
    );

    // 已有统一根保留：重开不重复改写文件字节。
    let app_bytes = std::fs::read(settings::app_settings_path(root.path())).unwrap();
    let subscriptions_bytes =
        std::fs::read(root.path().join(resource_storage::SUBSCRIPTIONS_FILE)).unwrap();
    drop(root);
    let _reopened = DataRoot::open(temporary.path()).unwrap();
    assert_eq!(
        std::fs::read(settings::app_settings_path(temporary.path())).unwrap(),
        app_bytes
    );
    assert_eq!(
        std::fs::read(temporary.path().join(resource_storage::SUBSCRIPTIONS_FILE)).unwrap(),
        subscriptions_bytes
    );
}

/// 旧 JSON 测试布局的每个入口都明确拒绝：真实报错点名该入口，不迁移、
/// 不删除、不创建统一产物，其余文件不动。
#[test]
fn legacy_json_layout_is_rejected_with_a_real_error() {
    for legacy in ["client.json", "subscriptions.json"] {
        let temporary = tempfile::tempdir().unwrap();
        let root = DataRoot::open(temporary.path()).unwrap();
        let app_bytes = std::fs::read(settings::app_settings_path(root.path())).unwrap();
        drop(root);
        let legacy_bytes = b"{legacy test data".to_vec();
        std::fs::write(temporary.path().join(legacy), &legacy_bytes).unwrap();

        let error = DataRoot::open(temporary.path())
            .err()
            .expect("旧布局必须被拒绝");
        let message = error.to_string();
        assert!(message.contains(legacy), "{message}");
        assert!(message.contains("不受支持"), "{message}");
        // 不删除、不改写旧文件；已建统一产物保持原字节。
        assert_eq!(
            std::fs::read(temporary.path().join(legacy)).unwrap(),
            legacy_bytes
        );
        assert_eq!(
            std::fs::read(settings::app_settings_path(temporary.path())).unwrap(),
            app_bytes
        );
    }
    for legacy in ["subscription_cache", "profile_cache"] {
        let temporary = tempfile::tempdir().unwrap();
        let root = DataRoot::open(temporary.path()).unwrap();
        drop(root);
        std::fs::create_dir_all(temporary.path().join(legacy)).unwrap();
        std::fs::write(temporary.path().join(legacy).join("keep.json"), b"keep").unwrap();

        let error = DataRoot::open(temporary.path())
            .err()
            .expect("旧布局必须被拒绝");
        let message = error.to_string();
        assert!(message.contains(legacy), "{message}");
        assert!(message.contains("不受支持"), "{message}");
        // 不删除、不清空旧目录。
        assert_eq!(
            std::fs::read(temporary.path().join(legacy).join("keep.json")).unwrap(),
            b"keep"
        );
    }
}
