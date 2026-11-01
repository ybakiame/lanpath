//! Mihomo config composition and settings injection.

use lanpath_common::{LanpathError, LanpathResult};
use serde_json::{Value, json};

use super::MitmChain;

/// Inject local-only mixed inbounds and optional MITM return chain.
pub fn compose_mihomo_config(
    config: &mut Value,
    mixed_port: u16,
    mitm_chain: Option<MitmChain>,
) -> LanpathResult<()> {
    let obj = config
        .as_object_mut()
        .ok_or_else(|| LanpathError::Client("Mihomo config must be an object".into()))?;
    for key in ["port", "socks-port", "redir-port", "tproxy-port"] {
        obj.remove(key);
    }
    obj.insert("allow-lan".to_string(), json!(false));
    obj.insert("bind-address".to_string(), json!("127.0.0.1"));

    let Some(chain) = mitm_chain else {
        obj.insert("mixed-port".to_string(), json!(mixed_port));
        return Ok(());
    };

    obj.remove("mixed-port");
    obj.insert(
        "listeners".to_string(),
        json!([
            { "name": "main-in", "type": "mixed", "port": mixed_port, "listen": "127.0.0.1" },
            { "name": "mitm-return", "type": "mixed", "port": chain.return_port, "listen": "127.0.0.1" }
        ]),
    );
    let mut proxies = obj
        .remove("proxies")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    proxies.push(json!({
        "name": "lanpath-mitm",
        "type": "http",
        "server": "127.0.0.1",
        "port": chain.proxy_addr.port(),
    }));
    obj.insert("proxies".to_string(), Value::Array(proxies));

    let rules = obj
        .remove("rules")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    let mut prepended = Vec::new();
    for hostname in &chain.hostnames {
        if hostname.starts_with('-') || hostname.starts_with('!') {
            continue;
        }
        let rule = match hostname.strip_prefix("*.") {
            Some(suffix) => {
                format!("AND,((IN-NAME,main-in),(DOMAIN-SUFFIX,{suffix})),lanpath-mitm")
            }
            None => format!("AND,((IN-NAME,main-in),(DOMAIN,{hostname})),lanpath-mitm"),
        };
        prepended.push(Value::String(rule));
    }
    prepended.extend(rules);
    obj.insert("rules".to_string(), Value::Array(prepended));
    Ok(())
}

/// Apply TUN, mode and Clash API settings with highest priority.
pub fn apply_mihomo_panel_features(config: &mut Value, features: &super::PanelFeatures) {
    let Some(obj) = config.as_object_mut() else {
        return;
    };
    obj.insert(
        "mode".to_string(),
        json!(match features.rule_mode.as_str() {
            "global" => "global",
            "direct" => "direct",
            _ => "rule",
        }),
    );
    if features.tun_enabled {
        obj.insert(
            "tun".to_string(),
            json!({
                "enable": true,
                "stack": features.tun_stack,
                "auto-route": features.tun_auto_route,
                "auto-detect-interface": true,
                "dns-hijack": ["any:53"],
            }),
        );
    }
    if features.clash_api_enabled {
        obj.insert(
            "external-controller".to_string(),
            json!(format!("127.0.0.1:{}", features.clash_api_port)),
        );
        obj.insert(
            "external-ui".to_string(),
            json!(format!(
                "ui-{}",
                super::normalized_clash_api_ui(&features.clash_api_ui)
            )),
        );
        obj.insert(
            "external-ui-url".to_string(),
            json!(super::clash_api_ui_download_url(&features.clash_api_ui)),
        );
        if features.clash_api_secret.is_empty() {
            obj.remove("secret");
        } else {
            obj.insert("secret".to_string(), json!(features.clash_api_secret));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_config::PanelFeatures;

    fn features() -> PanelFeatures {
        PanelFeatures {
            tun_enabled: false,
            tun_stack: "mixed".into(),
            tun_auto_route: true,
            clash_api_enabled: true,
            clash_api_port: 19090,
            clash_api_secret: "secret".into(),
            clash_api_ui: "zashboard".into(),
            rule_mode: "global".into(),
            ipv6_enabled: false,
            dns_fakeip_enabled: false,
            dns_mode: crate::config_slices::DnsMode::FollowSystem,
            github_proxy_prefix: String::new(),
            data_dir: String::new(),
        }
    }

    #[test]
    fn injects_local_mixed_inbound_and_panel_settings() {
        let mut config = serde_json::json!({
            "mixed-port": 1,
            "port": 2,
            "socks-port": 3,
            "proxies": [],
            "rules": ["MATCH,DIRECT"]
        });
        compose_mihomo_config(&mut config, 17899, None).unwrap();
        apply_mihomo_panel_features(&mut config, &features());

        assert_eq!(config["mixed-port"], 17899);
        assert!(config.get("port").is_none());
        assert_eq!(config["allow-lan"], false);
        assert_eq!(config["bind-address"], "127.0.0.1");
        assert_eq!(config["mode"], "global");
        assert_eq!(config["external-controller"], "127.0.0.1:19090");
        assert_eq!(config["secret"], "secret");
        assert_eq!(config["external-ui"], "ui-zashboard");
    }

    #[test]
    fn mitm_chain_adds_listeners_proxy_and_scoped_rules() {
        let mut config = serde_json::json!({
            "proxies": [],
            "rules": ["MATCH,DIRECT"]
        });
        compose_mihomo_config(
            &mut config,
            17899,
            Some(MitmChain {
                proxy_addr: "127.0.0.1:18001".parse().unwrap(),
                return_port: 17900,
                hostnames: vec!["*.example.com".into(), "!skip.example.com".into()],
            }),
        )
        .unwrap();

        assert!(config.get("mixed-port").is_none());
        assert_eq!(config["listeners"][0]["name"], "main-in");
        assert_eq!(config["listeners"][1]["name"], "mitm-return");
        assert_eq!(config["proxies"][0]["name"], "lanpath-mitm");
        assert_eq!(
            config["rules"][0],
            "AND,((IN-NAME,main-in),(DOMAIN-SUFFIX,example.com)),lanpath-mitm"
        );
        assert_eq!(config["rules"][1], "MATCH,DIRECT");
    }
}
