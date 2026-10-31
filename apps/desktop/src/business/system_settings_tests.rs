use lanpath_client::{CachedSubscriptionContent, SubFormat};
use serde_json::json;

use super::BusinessService;
use crate::{configuration::Bindings, proxy_service::ProxyService};

fn service(root: &std::path::Path) -> BusinessService {
    BusinessService::new(root, None, Bindings::new(17899, 19090))
}

#[tokio::test]
async fn disabled_api_with_zero_port_omits_inherited_api_and_dashboard_assets() {
    let root = tempfile::tempdir().unwrap();
    let business = service(root.path());
    let profile = business.save_profile(
        None,
        "inherited dashboard",
        "experimental:\n  clash_api:\n    external_controller: 127.0.0.1:19091\n    external_ui: old-dashboard\n    external_ui_download_url: https://example.invalid/dashboard.zip\n".into(),
        String::new(), None, None,
    ).unwrap();
    let subscription = business
        .subscription_store()
        .add("fixture", "https://example.invalid/sub", true, None)
        .unwrap();
    business
        .subscription_store()
        .set_profile_id(subscription.id, Some(profile.id))
        .unwrap();
    business
        .subscription_store()
        .write_cached_content(
            subscription.id,
            &CachedSubscriptionContent {
                format: SubFormat::SingBoxJson,
                singbox_nodes: vec![json!({
                    "type": "shadowsocks", "tag": "fixture", "server": "127.0.0.1",
                    "server_port": 8388, "method": "aes-128-gcm", "password": "fixture"
                })],
            },
        )
        .unwrap();
    business
        .set_active_subscription(Some(subscription.id))
        .unwrap();
    let mut settings = business.system_settings().unwrap();
    settings.clash_api_enabled = false;
    settings.clash_api_port = 0;
    settings.clash_api_secret = "keep-entered-secret".into();
    settings.clash_api_ui = "metacubexd".into();
    business.save_system_settings(&settings).unwrap();

    let reopened = service(root.path());
    assert!(reopened.system_settings().unwrap() == settings);
    assert!(reopened.clash_api_endpoint().is_err());
    let generated = reopened.build_config(false).await.unwrap();
    assert!(generated.pointer("/experimental/clash_api").is_none());
    assert_eq!(generated["inbounds"][0]["listen_port"], 17899);

    let proxies = ProxyService::new(Some(reopened));
    assert!(proxies.list().await.unwrap_err().contains("Clash API"));
    assert!(
        proxies
            .select("proxy", "fixture")
            .await
            .unwrap_err()
            .contains("Clash API")
    );
    assert!(
        proxies
            .test_delay("fixture")
            .await
            .unwrap_err()
            .contains("Clash API")
    );
    assert!(
        proxies
            .test_group("proxy")
            .await
            .unwrap_err()
            .contains("Clash API")
    );
}

#[test]
fn enabling_invalid_or_conflicting_api_port_never_partially_saves() {
    let root = tempfile::tempdir().unwrap();
    let business = service(root.path());
    let mut config = business.client_config().unwrap();
    config.mitm_enabled = true;
    config.save().unwrap();
    let mut saved = business.system_settings().unwrap();
    saved.clash_api_enabled = false;
    saved.clash_api_port = 0;
    saved.clash_api_secret = "persisted-secret".into();
    saved.clash_api_ui = "metacubexd".into();
    business.save_system_settings(&saved).unwrap();
    let path = root.path().join("app.toml");
    let before = std::fs::read(&path).unwrap();

    for port in [0, saved.mixed_port, saved.mixed_port + 1] {
        let mut invalid = saved.clone();
        invalid.clash_api_enabled = true;
        invalid.clash_api_port = port;
        invalid.clash_api_secret = "must-not-save".into();
        invalid.ipv6_enabled = !saved.ipv6_enabled;
        assert!(business.save_system_settings(&invalid).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(business.system_settings().unwrap() == saved);
    }
    let mut invalid = saved.clone();
    invalid.mixed_port = 0;
    assert!(business.save_system_settings(&invalid).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);

    let mut enabled = business.system_settings().unwrap();
    enabled.clash_api_enabled = true;
    enabled.clash_api_port = 19091;
    business.save_system_settings(&enabled).unwrap();
    let reopened = service(root.path());
    assert!(reopened.system_settings().unwrap() == enabled);
    assert_eq!(
        reopened.clash_api_endpoint().unwrap(),
        (19091, "persisted-secret".into())
    );
    assert_eq!(enabled.clash_api_ui, "metacubexd");

    enabled.clash_api_enabled = false;
    reopened.save_system_settings(&enabled).unwrap();
    let disabled = service(root.path()).system_settings().unwrap();
    assert_eq!(disabled.clash_api_port, 19091);
    assert_eq!(disabled.clash_api_secret, "persisted-secret");
    assert_eq!(disabled.clash_api_ui, "metacubexd");
}
