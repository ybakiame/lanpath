//! Rule-set HTTP client and template baseline tests for sing-box.

use serde_json::json;

use crate::profile::singbox_template;

/// The direct-dial HTTP client is registered at top level and resolves download URLs through
/// the direct `local` DNS — downloads never route through `route.final` (the proxy), so a cold
/// start does not depend on the proxy being up before any rule can be evaluated.
#[test]
fn singbox_template_registers_rule_set_http_client() {
    let cfg = singbox_template(&[]);
    assert_eq!(
        cfg["http_clients"],
        json!([{ "tag": "rule-set-direct", "domain_resolver": { "server": "local" } }])
    );
}

/// Without a `local` DNS server (fully user-owned DNS body), the HTTP client injection is
/// skipped rather than risking a dangling resolver reference.
#[test]
fn rule_set_http_client_skipped_without_local_dns() {
    let mut cfg =
        json!({ "dns": { "servers": [{ "tag": "other", "type": "udp", "server": "1.1.1.1" }] } });
    crate::core_config::ensure_rule_set_http_client(cfg.as_object_mut().unwrap());
    assert!(
        cfg.get("http_clients").is_none(),
        "no local DNS server -> no http_client injected"
    );

    // Idempotency: an existing same-tag client (user-supplied) is respected.
    let mut cfg = json!({
        "dns": { "servers": [{ "tag": "local", "type": "udp", "server": "223.5.5.5" }] },
        "http_clients": [{ "tag": "rule-set-direct", "detour": "proxy" }]
    });
    crate::core_config::ensure_rule_set_http_client(cfg.as_object_mut().unwrap());
    assert_eq!(
        cfg["http_clients"],
        json!([{ "tag": "rule-set-direct", "detour": "proxy" }]),
        "existing same-tag client must not be overwritten"
    );
}

#[tokio::test]
async fn takeover_dns_keeps_generated_dialers_bound_to_declared_resolvers() {
    use crate::config_slices::{
        ConfigSlices, DnsMode, DnsServer, DnsServerType, DnsSlice, DomainResolverSlice,
    };
    use crate::profile::{EffectiveOverrides, build_core_config_v2};

    for explicit_bootstrap in [false, true] {
        let mut slices = ConfigSlices {
            dns: DnsSlice {
                mode: DnsMode::Takeover,
                servers: ["bootstrap", "final-dns"]
                    .into_iter()
                    .map(|tag| DnsServer {
                        tag: tag.into(),
                        server: "127.0.0.1".into(),
                        server_type: DnsServerType::Udp,
                        ..Default::default()
                    })
                    .collect(),
                final_tag: "final-dns".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        if explicit_bootstrap {
            slices.route.resolver = DomainResolverSlice {
                server: "final-dns".into(),
                strategy: None,
            };
        }
        let config = build_core_config_v2(
            &json!({"outbounds": []}),
            &EffectiveOverrides::default(),
            &slices,
        )
        .await
        .unwrap();
        let expected = if explicit_bootstrap {
            "final-dns"
        } else {
            "bootstrap"
        };
        for resolver in [
            &config["route"]["default_domain_resolver"],
            &config["http_clients"][0]["domain_resolver"],
        ] {
            assert_eq!(resolver["server"], expected);
            assert!(
                config["dns"]["servers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|server| server["tag"] == resolver["server"]),
                "generated dialer must not retain a removed template DNS reference"
            );
        }
        let overridden = build_core_config_v2(
            &json!({"outbounds": []}),
            &EffectiveOverrides {
                local_yaml: "http_clients:\n  - tag: rule-set-direct\n    detour: explicit-proxy\n"
                    .into(),
                ..Default::default()
            },
            &slices,
        )
        .await
        .unwrap();
        assert_eq!(
            overridden["http_clients"],
            json!([{"tag": "rule-set-direct", "detour": "explicit-proxy"}]),
            "later user-owned clients must not be rebound"
        );
    }
}

#[test]
fn singbox_template_empty_nodes_falls_back_to_direct() {
    let cfg = singbox_template(&[]);
    let auto = cfg["outbounds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["tag"] == "auto")
        .unwrap();
    assert_eq!(auto["outbounds"], json!(["direct"]));
}

/// 默认 sing-box 模板不再预置任何内置规则集，完全由用户在「规则集」页自行添加。
#[test]
fn singbox_template_ships_no_builtin_rule_sets() {
    let cfg = singbox_template(&[]);
    let arr = cfg["route"]["rule_set"].as_array().unwrap();
    assert!(arr.is_empty(), "unexpected builtin rule sets: {arr:?}");
}
