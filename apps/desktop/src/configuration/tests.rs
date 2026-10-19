use std::path::PathBuf;

use serde_json::{Value, json};

use super::{Bindings, ConfigStore, policy, resources, unique_json};

fn bindings() -> Bindings {
    Bindings::new(17891, 19091)
}

fn store(root: &std::path::Path) -> ConfigStore {
    ConfigStore::new(root, None, bindings())
}

fn document(root: &std::path::Path) -> Value {
    serde_json::from_str(&store(root).starter_text().unwrap()).unwrap()
}

fn reject(root: &std::path::Path, value: &Value, location: &str) {
    let error = policy::check(root, value, bindings())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(location),
        "missing field location {location}: {error}"
    );
}

#[test]
fn hosts_paths_cannot_expand_environment_variables_outside_the_owned_root() {
    let root = tempfile::tempdir().unwrap();
    let literal = root.path().join("$LANPATH_OUTSIDE/hosts");
    std::fs::create_dir(literal.parent().unwrap()).unwrap();
    std::fs::write(&literal, b"127.0.0.1 fixture.invalid\n").unwrap();
    let mut config = document(root.path());
    config["dns"] = json!({
        "servers": [{"type": "hosts", "tag": "local", "path": ["$LANPATH_OUTSIDE/hosts"]}]
    });
    reject(root.path(), &config, "dns.servers[0].path");
}

#[test]
fn managed_listener_cannot_be_widened_or_given_system_effects() {
    let root = tempfile::tempdir().unwrap();
    for (key, value) in [
        ("listen", json!("0.0.0.0")),
        ("listen", json!("::1")),
        ("listen_port", json!(17892)),
        ("type", json!("tun")),
        ("set_system_proxy", json!(true)),
        ("set_system_proxy", Value::Null),
    ] {
        let mut config = document(root.path());
        config["inbounds"][0][key] = value;
        reject(root.path(), &config, &format!("inbounds[0].{key}"));
    }
    let mut config = document(root.path());
    config["inbounds"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"tun"}));
    reject(root.path(), &config, "inbounds");
    let mut config = document(root.path());
    config["inbounds"][0]
        .as_object_mut()
        .unwrap()
        .remove("listen");
    reject(root.path(), &config, "inbounds[0].listen");
}

#[test]
fn extra_control_planes_and_unknown_top_level_capabilities_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    for key in [
        "services",
        "endpoints",
        "network_namespaces",
        "certificate_providers",
        "future_control_plane",
    ] {
        let mut config = document(root.path());
        config[key] = json!([]);
        reject(root.path(), &config, key);
    }
    let mut config = document(root.path());
    config["experimental"]["v2ray_api"] = json!({"listen":"127.0.0.1:9000"});
    reject(root.path(), &config, "experimental.v2ray_api");
    for (key, value) in [
        ("secret", json!("not-erased")),
        ("external_controller", json!("localhost:19091")),
        ("external_ui", json!("ui")),
        (
            "external_ui_download_url",
            json!("https://example.invalid/panel.zip"),
        ),
    ] {
        let mut config = document(root.path());
        config["experimental"]["clash_api"][key] = value;
        reject(
            root.path(),
            &config,
            &format!("experimental.clash_api.{key}"),
        );
    }
}

#[test]
fn ntp_clock_writes_and_external_runtime_protocols_are_not_authorized() {
    let root = tempfile::tempdir().unwrap();
    let mut config = document(root.path());
    config["ntp"] = json!({"enabled":true,"server":"time.example","write_to_system":true});
    reject(root.path(), &config, "ntp.write_to_system");
    config["ntp"]["write_to_system"] = json!(false);
    policy::check(root.path(), &config, bindings()).unwrap();
    for kind in [
        "wireguard",
        "tailscale",
        "ssh",
        "tor",
        "bridge",
        "naive",
        "future_listener",
    ] {
        let mut config = document(root.path());
        config["outbounds"][1]["type"] = json!(kind);
        reject(root.path(), &config, "outbounds[1].type");
    }
    let mut config = document(root.path());
    config["outbounds"][1]["netns"] = json!("/proc/1/ns/net");
    reject(root.path(), &config, "outbounds[1].netns");
}

#[test]
fn common_client_protocols_are_not_blanket_rejected() {
    let root = tempfile::tempdir().unwrap();
    for protocol in [
        "socks",
        "http",
        "shadowsocks",
        "vmess",
        "vless",
        "trojan",
        "hysteria",
        "hysteria2",
        "tuic",
        "shadowtls",
        "anytls",
        "snell",
    ] {
        let mut config = document(root.path());
        config["outbounds"][1] = json!({
            "type":protocol,"tag":"direct","server":"proxy.example","server_port":443
        });
        // These cases exercise the capability boundary, not protocol schema.
        // Required credentials remain the real core's responsibility.
        policy::check(root.path(), &config, bindings()).unwrap();
    }
}

#[test]
fn tls_nested_resources_and_unsupported_certificate_automation_cannot_escape() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("ca.pem"), "certificate fixture").unwrap();
    let mut config = document(root.path());
    config["outbounds"][1] = json!({
        "type":"trojan","tag":"direct","server":"proxy.example","server_port":443,
        "password":"secret","tls":{"enabled":true,"certificate_path":"ca.pem"}
    });
    policy::check(root.path(), &config, bindings()).unwrap();
    for name in [
        "certificate_path",
        "client_certificate_path",
        "client_key_path",
    ] {
        config["outbounds"][1]["tls"][name] = json!("../outside.pem");
        reject(root.path(), &config, &format!("outbounds[1].tls.{name}"));
        config["outbounds"][1]["tls"]
            .as_object_mut()
            .unwrap()
            .remove(name);
    }
    config["outbounds"][1]["tls"]["ech"] = json!({"config_path":"../outside.json"});
    reject(root.path(), &config, "outbounds[1].tls.ech.config_path");
    config["outbounds"][1]["tls"]
        .as_object_mut()
        .unwrap()
        .remove("ech");
    config["outbounds"][1]["tls"]["acme"] = json!({"domain":["example.invalid"]});
    reject(root.path(), &config, "outbounds[1].tls.acme");
}

#[test]
fn local_rule_files_and_http_client_tls_are_confined_without_expansion() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("rules.json"),
        r#"{"version":3,"rules":[]}"#,
    )
    .unwrap();
    let mut config = document(root.path());
    config["route"]["rule_set"] = json!([{"type":"local","tag":"private","path":"rules.json"}]);
    policy::check(root.path(), &config, bindings()).unwrap();
    for path in [
        "../rules.json",
        "/etc/passwd",
        "C:/production/rules.json",
        "rules-{tag}.json",
    ] {
        config["route"]["rule_set"][0]["path"] = json!(path);
        reject(root.path(), &config, "route.rule_set[0].path");
    }
    config["route"]["rule_set"][0]["path"] = json!("rules.json");
    config["route"]["rule_set"][0]["tag"] = json!(["a", "../../production"]);
    reject(root.path(), &config, "route.rule_set[0].tag");
    config["route"].as_object_mut().unwrap().remove("rule_set");
    config["http_clients"] = json!([{"tag":"download","tls":{"client_key_path":"../key.pem"}}]);
    reject(root.path(), &config, "http_clients[0].tls.client_key_path");
}

#[test]
fn write_resources_cannot_overwrite_managed_documents_or_use_implicit_cache_path() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("logs")).unwrap();
    std::fs::create_dir(root.path().join("cache")).unwrap();
    let mut config = document(root.path());
    config["log"]["output"] = json!("logs/core.log");
    config["experimental"]["cache_file"] = json!({"enabled":true,"path":"cache/core.db"});
    policy::check(root.path(), &config, bindings()).unwrap();
    for path in [
        "configuration.json",
        "sing-box.json",
        ".lanpath-experimental.lock",
        "stats.db",
        "../outside.log",
        "logs/../configuration.json",
        "logs/NUL",
        "logs/a:stream",
        "logs/dir./file",
        "logs/core.log ",
    ] {
        config["log"]["output"] = json!(path);
        reject(root.path(), &config, "log.output");
    }
    config["log"].as_object_mut().unwrap().remove("output");
    config["experimental"]["cache_file"]
        .as_object_mut()
        .unwrap()
        .remove("path");
    reject(root.path(), &config, "experimental.cache_file.path");
}

#[test]
fn dns_hosts_cannot_fall_back_to_external_system_files() {
    let root = tempfile::tempdir().unwrap();
    let mut config = document(root.path());
    for server in [
        json!({"type":"hosts","tag":"hosts"}),
        json!({"type":"hosts","tag":"hosts","path":[]}),
        json!({"type":"hosts","tag":"hosts","path":["../hosts"]}),
        json!({"type":"dhcp","tag":"dhcp"}),
        json!({"type":"mdns","tag":"mdns"}),
    ] {
        config["dns"] = json!({"servers":[server]});
        reject(root.path(), &config, "dns.servers[0]");
    }
}

#[test]
fn unknown_nested_tls_and_rule_actions_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let mut config = document(root.path());
    config["route"]["rules"] =
        json!([{"type":"logical","mode":"or","rules":[{"action":"bypass"}]}]);
    reject(root.path(), &config, "route.rules[0].rules[0].action");
    config["route"].as_object_mut().unwrap().remove("rules");
    config["outbounds"][1] =
        json!({"type":"http","tag":"direct","tls":{"future_key_log":"/tmp/secrets"}});
    reject(root.path(), &config, "outbounds[1].tls.future_key_log");
}

#[test]
fn duplicate_object_keys_are_rejected_at_every_depth_including_escaped_keys() {
    for text in [
        r#"{"experimental":{"v2ray_api":{}},"experimental":{"clash_api":{}}}"#,
        r#"{"a":[{"tls":{"key_path":"outside"},"tls":{}}]}"#,
        r#"{"listen":"0.0.0.0","\u006cisten":"127.0.0.1"}"#,
    ] {
        assert!(unique_json::parse(text).is_err());
    }
    let text = r#"{"a":[null,true,false,-1,18446744073709551615,1.25,"é"]}"#;
    assert_eq!(
        unique_json::parse(text).unwrap(),
        serde_json::from_str::<Value>(text).unwrap()
    );
}

#[tokio::test]
async fn missing_or_malformed_saved_documents_remain_distinguishable_and_editable() {
    let root = tempfile::tempdir().unwrap();
    let store = store(root.path());
    assert!(store.load().await.unwrap().is_none());
    assert!(store.for_start().await.is_err());
    let malformed = "{\n  unclosed draft\n";
    std::fs::write(root.path().join("configuration.json"), malformed).unwrap();
    assert_eq!(store.load().await.unwrap().unwrap(), malformed);
    assert!(store.save("[]").await.is_err());
    assert!(store.save("{invalid}").await.is_err());
    assert_eq!(store.load().await.unwrap().unwrap(), malformed);
    assert!(store.for_start().await.is_err());
}

#[tokio::test]
async fn missing_real_core_never_marks_a_valid_draft_as_saved() {
    let root = tempfile::tempdir().unwrap();
    let store = store(root.path());
    let original = "old source retained exactly\n";
    std::fs::write(root.path().join("configuration.json"), original).unwrap();
    let draft = store.starter_text().unwrap();
    assert!(store.validate(&draft).await.is_err());
    assert!(store.save(&draft).await.is_err());
    assert_eq!(store.load().await.unwrap().unwrap(), original);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn existing_resource_hardlinks_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("external"), "do not overwrite").unwrap();
    std::fs::hard_link(outside.path().join("external"), root.path().join("linked")).unwrap();
    assert!(resources::confined(root.path(), "linked", "resource").is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn symlinked_sources_and_resource_parents_are_refused_without_reading_them() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("production.json"), "production source").unwrap();
    symlink(
        outside.path().join("production.json"),
        root.path().join("configuration.json"),
    )
    .unwrap();
    assert!(store(root.path()).load().await.is_err());
    symlink(outside.path(), root.path().join("logs")).unwrap();
    let mut config = document(root.path());
    config["log"]["output"] = json!("logs/core.log");
    reject(root.path(), &config, "log.output");
}

/// Explicit opt-in test against a real selected sing-box, never an echo script
/// or a fake checker. Set LANPATH_TEST_SING_BOX to its absolute executable path.
#[tokio::test]
#[ignore = "requires LANPATH_TEST_SING_BOX pointing to a real sing-box executable"]
async fn real_core_checks_exact_saved_source_and_rechecks_before_start() {
    let binary = PathBuf::from(
        std::env::var_os("LANPATH_TEST_SING_BOX")
            .expect("set LANPATH_TEST_SING_BOX to a real core"),
    );
    assert!(binary.is_absolute());
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    let store = ConfigStore::new(&root_path, Some(binary), bindings());
    let exact = format!(" \n{}\n\n", store.starter_text().unwrap());
    store.validate(&exact).await.unwrap();
    assert!(!root_path.join("configuration.json").exists());
    store.save(&exact).await.unwrap();
    assert_eq!(store.load().await.unwrap().unwrap(), exact);
    assert_eq!(
        store.for_start().await.unwrap(),
        serde_json::from_str::<Value>(&exact).unwrap()
    );
    let mut invalid = serde_json::from_str::<Value>(&exact).unwrap();
    invalid["outbounds"][1] =
        json!({"type":"socks","tag":"direct","server":"127.0.0.1","server_port":"not-a-port"});
    let invalid = serde_json::to_string(&invalid).unwrap();
    // Passes capability checks; only the actual core rejects the bad schema.
    assert!(store.save(&invalid).await.is_err());
    assert_eq!(store.load().await.unwrap().unwrap(), exact);
    std::fs::write(root_path.join("configuration.json"), invalid).unwrap();
    assert!(store.for_start().await.is_err());
    assert_eq!(std::fs::read_dir(&root_path).unwrap().count(), 1);
}

#[test]
fn go_case_insensitive_field_aliases_cannot_hide_side_effects() {
    let root = tempfile::tempdir().unwrap();
    for (pointer, key, value, location) in [
        ("/log", "OUTPUT", json!("../production.json"), "log.OUTPUT"),
        ("/log", "Output", json!("../production.json"), "log.Output"),
        (
            "/experimental/clash_api",
            "EXTERNAL_CONTROLLER",
            json!("0.0.0.0:9000"),
            "experimental.clash_api.EXTERNAL_CONTROLLER",
        ),
        (
            "/experimental/clash_api",
            "Secret",
            json!("hidden"),
            "experimental.clash_api.Secret",
        ),
        (
            "/inbounds/0",
            "SET_SYSTEM_PROXY",
            json!(true),
            "inbounds[0].SET_SYSTEM_PROXY",
        ),
        (
            "/outbounds/1",
            "NetNS",
            json!("/proc/1/ns/net"),
            "outbounds[1].NetNS",
        ),
    ] {
        let mut config = document(root.path());
        config
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(key.into(), value);
        reject(root.path(), &config, location);
    }
    for (key, value) in [
        ("WRITE_TO_SYSTEM", json!(true)),
        ("Write_To_System", json!(true)),
    ] {
        let mut config = document(root.path());
        config["ntp"] = json!({"write_to_system":false,"server":"time.example"});
        config["ntp"][key] = value;
        reject(root.path(), &config, &format!("ntp.{key}"));
    }
    for (key, value) in [
        ("ENABLED", json!(true)),
        ("PATH", json!("../production.db")),
    ] {
        let mut config = document(root.path());
        config["experimental"]["cache_file"] = json!({"enabled":false});
        config["experimental"]["cache_file"][key] = value;
        reject(
            root.path(),
            &config,
            &format!("experimental.cache_file.{key}"),
        );
    }
    let mut config = document(root.path());
    config["outbounds"][1] = json!({"type":"http","tls":{"CERTIFICATE_PATH":"../production.pem"}});
    reject(root.path(), &config, "outbounds[1].tls.CERTIFICATE_PATH");
    let mut config = document(root.path());
    config["route"]["rule_set"] =
        json!([{"type":"local","tag":"rules","PATH":"../production.json"}]);
    reject(root.path(), &config, "route.rule_set[0].PATH");
}
