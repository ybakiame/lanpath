use lanpath_client::{CachedSubscriptionContent, RequestProxyMode, SubFormat};
use serde_json::json;

use super::BusinessService;
use crate::configuration::Bindings;

#[tokio::test]
async fn admitted_settings_survive_external_changes_through_generation_and_policy_admission() {
    let root = tempfile::tempdir().unwrap();
    for directory in ["cache", "logs"] {
        std::fs::create_dir_all(root.path().join(directory)).unwrap();
    }
    let business = BusinessService::new(root.path(), None, Bindings::new(17899, 19090));
    let subscriptions = business.subscription_store();
    let selected = subscriptions
        .add("snapshot", "https://example.invalid/snapshot", true, None)
        .unwrap();
    subscriptions
        .write_cached_content(
            selected.id,
            &CachedSubscriptionContent {
                format: SubFormat::SingBoxJson,
                singbox_nodes: vec![json!({
                    "type": "shadowsocks",
                    "tag": "snapshot-node",
                    "server": "127.0.0.1",
                    "server_port": 8388,
                    "method": "aes-256-gcm",
                    "password": "owned-snapshot-fixture"
                })],
            },
        )
        .unwrap();
    business.set_active_subscription(Some(selected.id)).unwrap();
    let mut admitted = business.client_config().unwrap();
    admitted.request_proxy_mode = RequestProxyMode::Core;
    admitted.tun_enabled = false;
    admitted.mixed_port = 17899;
    admitted.clash_api_enabled = true;
    admitted.clash_api_port = 19090;
    admitted.core_binary.clear();
    admitted.save().unwrap();
    let validator = business.validator(&admitted);

    let mut external = business.client_config().unwrap();
    external.tun_enabled = true;
    external.mixed_port = 24567;
    external.clash_api_port = 24568;
    external.save().unwrap();
    let saved_bytes = std::fs::read(root.path().join("client.json")).unwrap();

    let (generated, _) = business
        .build_config_report(false, None, admitted)
        .await
        .unwrap();
    assert_eq!(generated["inbounds"][0]["listen_port"], json!(17899));
    assert!(
        generated["inbounds"]
            .as_array()
            .unwrap()
            .iter()
            .all(|inbound| inbound["type"] != "tun")
    );
    assert_eq!(
        generated["experimental"]["clash_api"]["external_controller"],
        json!("127.0.0.1:19090")
    );
    // With no checker selected, successful capability admission reaches Core
    // unavailability. The externally changed bindings must reject as Config first.
    let admitted_result = validator.validate_value(&generated).await;
    assert!(
        matches!(&admitted_result, Err(lanpath_common::LanpathError::Core(_))),
        "admitted policy failed before the explicit missing-checker boundary: {admitted_result:?}"
    );
    assert!(matches!(
        business
            .validator(&external)
            .validate_value(&generated)
            .await,
        Err(lanpath_common::LanpathError::Config(_))
    ));
    assert_eq!(
        std::fs::read(root.path().join("client.json")).unwrap(),
        saved_bytes
    );
}
