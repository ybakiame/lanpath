//! 统一 TOML 存储初始化测试：新根初始化默认、已有根原样保留。

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
    // 新装默认：直连、MITM 关闭、data_dir 本根；默认本地监听端口 17899/19090。
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
