//! 请求设置业务测试：四模式重开、保存合并且不覆盖无关字段、非法代理
//! 脱敏且旧文件不变、新装直连默认与 data_dir 固定、损坏配置真实错误。
//! 网络语义（GitHub 前缀拼接）直接调用共享 `apply_github_proxy_prefix` 验证。

use lanpath_client::{ClientConfig, RequestProxyMode, ScriptDialect};

use super::*;

/// 把三个请求字段外的全部配置与期望值逐项对齐（不引入 serde_json 依赖）：
/// 将已保存配置的三个字段覆盖为草稿值后，应与“外部最新配置 + 草稿请求字段”完全相等。
fn assert_unrelated_fields_match(
    saved: &ClientConfig,
    latest: &ClientConfig,
    draft: &RequestSettingsDocument,
) {
    let mut expected = latest.clone();
    expected.request_proxy_mode = draft.mode();
    expected.request_proxy_url = draft.custom_url().to_owned();
    expected.github_proxy_prefix = draft.github_prefix().to_owned();
    assert_eq!(saved, &expected, "无关字段必须保持最后一次外部写入的值");
}

#[test]
fn fresh_install_loads_direct_none_pins_root_and_never_writes_on_read() {
    let temporary = tempfile::tempdir().unwrap();
    let service = RequestSettings::for_root(temporary.path()).unwrap();

    let document = service.load().unwrap();
    assert_eq!(document.mode(), RequestProxyMode::None);
    assert!(document.custom_url().is_empty());
    assert!(document.github_prefix().is_empty());
    // 读取不无故写盘：新装根上 client.json 仍不存在。
    assert!(!temporary.path().join("client.json").exists());

    // 首次保存写入文件；load_for_requests 立即可读且 data_dir 固定本根。
    service
        .save(&RequestSettingsDocument::new(
            RequestProxyMode::System,
            String::new(),
            String::new(),
        ))
        .unwrap();
    let saved = ClientConfig::load_for_requests(temporary.path()).unwrap();
    assert_eq!(saved.data_dir.as_path(), temporary.path());
    assert_eq!(saved.request_proxy_mode, RequestProxyMode::System);
}

#[test]
fn four_modes_roundtrip_reopen_and_shared_consumer_reads_same_policy() {
    let temporary = tempfile::tempdir().unwrap();
    let service = RequestSettings::for_root(temporary.path()).unwrap();
    assert_eq!(
        service.load().unwrap().mode(),
        RequestProxyMode::None,
        "新装默认直连"
    );
    let url = "socks5h://fixture-user:fixture-password@127.0.0.1:1080";
    let prefix = "  https://mirror.example/  ";
    for mode in [
        RequestProxyMode::None,
        RequestProxyMode::System,
        RequestProxyMode::Core,
        RequestProxyMode::Custom,
    ] {
        service
            .save(&RequestSettingsDocument::new(
                mode,
                url.to_owned(),
                prefix.to_owned(),
            ))
            .unwrap();
        // 同一数据根重开：拿到共享根句柄上的最新已保存文档。
        let reopened = RequestSettings::for_root(temporary.path()).unwrap();
        let document = reopened.load().unwrap();
        assert_eq!(document.mode(), mode);
        assert_eq!(document.custom_url(), url);
        assert_eq!(document.github_prefix(), prefix);
        // 共享消费端（订阅刷新/远程资源请求同一条读取路径）读到同一策略。
        let consumer = ClientConfig::load_for_requests(temporary.path()).unwrap();
        assert_eq!(consumer.request_proxy_mode, mode);
        assert_eq!(consumer.request_proxy_url, url);
        assert_eq!(consumer.github_proxy_prefix, prefix);
        // 前缀按共享语义在使用时修剪并拼接，保存值原样保留。
        assert_eq!(
            lanpath_client::apply_github_proxy_prefix(
                "https://api.github.com/zen",
                &consumer.github_proxy_prefix,
            ),
            "https://mirror.example/https://api.github.com/zen",
        );
    }
}

#[test]
fn non_custom_modes_keep_custom_url_draft_for_later_switch_back() {
    let temporary = tempfile::tempdir().unwrap();
    let service = RequestSettings::for_root(temporary.path()).unwrap();
    let url = "http://127.0.0.1:8080";
    service
        .save(&RequestSettingsDocument::new(
            RequestProxyMode::Custom,
            url.to_owned(),
            String::new(),
        ))
        .unwrap();
    // 切回直连：自定义地址作为草稿保留存储（与 GPUI 一致），再切回 Custom 直接可用。
    service
        .save(&RequestSettingsDocument::new(
            RequestProxyMode::None,
            url.to_owned(),
            String::new(),
        ))
        .unwrap();
    let kept = ClientConfig::load_for_requests(temporary.path()).unwrap();
    assert_eq!(kept.request_proxy_mode, RequestProxyMode::None);
    assert_eq!(kept.request_proxy_url, url);
    let document = service.load().unwrap();
    assert_eq!(document.mode(), RequestProxyMode::None);
    assert_eq!(document.custom_url(), url);
}

#[test]
fn save_merges_latest_unrelated_fields_without_clobbering_them() {
    let temporary = tempfile::tempdir().unwrap();
    let service = RequestSettings::for_root(temporary.path()).unwrap();
    let stale_draft = RequestSettingsDocument::new(
        RequestProxyMode::Custom,
        "http://fixture-user:fixture-password@127.0.0.1:8080".into(),
        "https://mirror.example".into(),
    );
    service.load().unwrap();
    // 这些编辑发生在请求设置页打开之后：active 选择、路由、MITM、
    // system/tun、端口、Clash API 等无关字段都被另一个业务写盘。
    let mut latest = ClientConfig::load_for_requests(temporary.path()).unwrap();
    latest.active_subscription_id = Some(uuid::Uuid::new_v4());
    latest.rule_mode = "global".into();
    latest.mitm_enabled = true;
    latest.mitm.hostnames = vec!["api.example.com".into()];
    latest.mitm.script_dialect = ScriptDialect::Loon;
    latest.system_proxy_enabled = true;
    latest.tun_enabled = true;
    latest.tun_auto_route = false;
    latest.ipv6_enabled = true;
    latest.mixed_port = 23456;
    latest.clash_api_enabled = false;
    latest.clash_api_port = 23457;
    latest.clash_api_secret = "fixture-api-secret".into();
    latest.save().unwrap();

    service.save(&stale_draft).unwrap();
    let saved = ClientConfig::load_for_requests(temporary.path()).unwrap();
    assert_eq!(saved.request_proxy_mode, RequestProxyMode::Custom);
    assert_eq!(saved.request_proxy_url, stale_draft.custom_url());
    assert_eq!(saved.github_proxy_prefix, stale_draft.github_prefix());
    assert_unrelated_fields_match(&saved, &latest, &stale_draft);
}

#[test]
fn invalid_custom_proxy_redacts_credentials_and_keeps_prior_saved_document() {
    let temporary = tempfile::tempdir().unwrap();
    let service = RequestSettings::for_root(temporary.path()).unwrap();
    service
        .save(&RequestSettingsDocument::new(
            RequestProxyMode::System,
            String::new(),
            "https://mirror.example".into(),
        ))
        .unwrap();
    let mut config = ClientConfig::load_for_requests(temporary.path()).unwrap();
    config.rule_mode = "direct".into();
    config.mitm_enabled = true;
    config.save().unwrap();
    let file = temporary.path().join("client.json");
    let prior = std::fs::read(&file).unwrap();
    for url in [
        "http://secret-user:secret-password@127.0.0.1:8080/private",
        "ftp://secret-user:secret-password@127.0.0.1:8080",
        "http://secret-user:secret-password@127.0.0.1:0",
        "http://secret-user:secret-password@127.0.0.1:8080?secret-token",
        "http://secret-user:secret-password@127.0.0.1:8080#secret-fragment",
        "not-a-url",
    ] {
        let error = service
            .save(&RequestSettingsDocument::new(
                RequestProxyMode::Custom,
                url.into(),
                "changed-mirror".into(),
            ))
            .err()
            .expect("invalid custom settings must fail")
            .to_string();
        for secret in ["secret-user", "secret-password", "secret-token", url] {
            assert!(!error.contains(secret), "error leaked {secret}: {error}");
        }
        assert!(std::fs::read(&file).unwrap() == prior, "旧文件被改写");
        let saved = service.load().unwrap();
        assert_eq!(saved.mode(), RequestProxyMode::System);
        assert!(saved.custom_url().is_empty());
        assert_eq!(saved.github_prefix(), "https://mirror.example");
    }
}

#[test]
fn reads_leave_existing_file_bytes_untouched() {
    let temporary = tempfile::tempdir().unwrap();
    let service = RequestSettings::for_root(temporary.path()).unwrap();
    service
        .save(&RequestSettingsDocument::new(
            RequestProxyMode::Core,
            String::new(),
            "https://mirror.example".into(),
        ))
        .unwrap();
    let file = temporary.path().join("client.json");
    let original = std::fs::read(&file).unwrap();
    service.load().unwrap();
    let reopened = RequestSettings::for_root(temporary.path()).unwrap();
    let document = reopened.load().unwrap();
    assert_eq!(document.mode(), RequestProxyMode::Core);
    assert_eq!(document.github_prefix(), "https://mirror.example");
    assert!(std::fs::read(&file).unwrap() == original);
}

#[test]
fn corrupt_config_returns_real_error_and_is_never_rewritten_to_defaults() {
    let temporary = tempfile::tempdir().unwrap();
    let service = RequestSettings::for_root(temporary.path()).unwrap();
    let file = temporary.path().join("client.json");
    std::fs::write(&file, b"{ not valid json").unwrap();
    let bytes = std::fs::read(&file).unwrap();

    let load_error = service.load().err().expect("损坏配置必须返回真实错误");
    assert!(!load_error.to_string().is_empty());
    let save_error = service
        .save(&RequestSettingsDocument::new(
            RequestProxyMode::System,
            String::new(),
            String::new(),
        ))
        .err()
        .expect("损坏配置的保存同样必须失败");
    assert!(!save_error.to_string().is_empty());
    // 失败路径绝不把损坏文件重写为默认配置。
    assert!(std::fs::read(&file).unwrap() == bytes);
}
