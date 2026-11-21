use serde_json::json;

use super::*;

#[test]
fn ensure_initialized_creates_all_five_defaults_and_preserves_edits() {
    let dir = tempfile::tempdir().unwrap();
    let store = SingboxOverwriteStore::new(dir.path().to_path_buf());
    store.ensure_initialized().unwrap();

    for kind in SingboxSliceKind::ALL {
        let path = store.slice_path(kind);
        assert!(path.is_file(), "missing {}", path.display());
        let val = store.load_value(kind).unwrap();
        assert_eq!(val, default_slice_value(kind));
    }

    store
        .save_raw(
            SingboxSliceKind::Log,
            r#"{ "log": { "level": "debug", "timestamp": false } }"#,
        )
        .unwrap();
    // 再次初始化不得覆盖已保存修改。
    store.ensure_initialized().unwrap();
    let log = store.load_value(SingboxSliceKind::Log).unwrap();
    assert_eq!(log["log"]["level"], "debug");
    assert_eq!(log["log"]["timestamp"], false);
}

#[test]
fn compose_expands_nodes_placeholder_and_avoids_duplicate_tags() {
    let bundle = SingboxOverwriteBundle::default();
    let sub_nodes = vec![
        json!({ "type": "shadowsocks", "tag": "HK-01", "server": "192.0.2.1", "server_port": 8388 }),
        // 故意与内置 proxy 重名，验证订阅节点自动重命名且不重复。
        json!({ "type": "shadowsocks", "tag": "proxy", "server": "192.0.2.2", "server_port": 8388 }),
    ];
    let composed = compose_from_overwrite(&sub_nodes, &bundle).unwrap();
    let outbounds = composed["outbounds"].as_array().unwrap();
    let tags: Vec<&str> = outbounds.iter().filter_map(|o| o["tag"].as_str()).collect();
    let unique: std::collections::HashSet<&str> = tags.iter().copied().collect();
    assert_eq!(tags.len(), unique.len(), "duplicate tags: {tags:?}");
    assert!(tags.contains(&"HK-01"));
    assert!(tags.contains(&"proxy-2"));

    let auto = outbounds.iter().find(|o| o["tag"] == "auto").unwrap();
    assert_eq!(auto["outbounds"], json!(["HK-01", "proxy-2"]));

    let proxy = outbounds.iter().find(|o| o["tag"] == "proxy").unwrap();
    assert_eq!(proxy["outbounds"], json!(["auto", "HK-01", "proxy-2"]));
}

#[test]
fn empty_nodes_urltest_falls_back_to_direct() {
    let bundle = SingboxOverwriteBundle::default();
    let composed = compose_from_overwrite(&[], &bundle).unwrap();
    let outbounds = composed["outbounds"].as_array().unwrap();
    let auto = outbounds.iter().find(|o| o["tag"] == "auto").unwrap();
    assert_eq!(auto["outbounds"], json!(["direct"]));
}

#[test]
fn validate_rejects_wrong_root_key_clash_api_and_env_paths() {
    assert!(parse_and_validate_slice(SingboxSliceKind::Log, r#"{ "inbounds": [] }"#).is_err());
    assert!(
        parse_and_validate_slice(
            SingboxSliceKind::Experimental,
            r#"{ "experimental": { "clash_api": { "external_controller": "0.0.0.0:9090" } } }"#
        )
        .is_err()
    );
    assert!(
        parse_and_validate_slice(
            SingboxSliceKind::Log,
            r#"{ "log": { "output": "$HOME/leak.log" } }"#
        )
        .is_err()
    );
    assert!(
        parse_and_validate_slice(
            SingboxSliceKind::Outbounds,
            r#"{ "outbounds": [ { "type": "direct", "tag": "dup" }, { "type": "block", "tag": "dup" } ] }"#
        )
        .is_err()
    );
}

#[test]
fn resilient_load_falls_back_and_reports_warning_on_corrupt_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = SingboxOverwriteStore::new(dir.path().to_path_buf());
    store.ensure_initialized().unwrap();
    std::fs::write(store.slice_path(SingboxSliceKind::Dns), "{not json").unwrap();

    let bundle = store.load_bundle_resilient();
    assert_eq!(bundle.warnings.len(), 1);
    assert!(bundle.warnings[0].contains("dns.json"));
    assert_eq!(bundle.dns, default_slice_value(SingboxSliceKind::Dns));
    assert!(!bundle.is_customized());
}

#[test]
fn extract_dns_servers_reads_saved_dns_slice() {
    let bundle = SingboxOverwriteBundle {
        dns: json!({
            "dns": {
                "servers": [
                    { "tag": "cf", "type": "https", "server": "1.1.1.1", "server_port": 443 }
                ]
            }
        }),
        ..Default::default()
    };
    let servers = extract_dns_servers(&bundle, false);
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].tag, "cf");
    assert_eq!(servers[0].server, "1.1.1.1");
    assert_eq!(servers[0].server_port, Some(443));
}

#[test]
fn defaults_do_not_inject_builtin_private_rulesets_and_presets_are_valid() {
    let dns = default_slice_value(SingboxSliceKind::Dns);
    assert_eq!(
        dns["dns"]["rules"],
        json!(crate::core_config::clash_mode_dns_rules())
    );
    let route = default_slice_value(SingboxSliceKind::Route);
    assert_eq!(route["route"]["rules"], json!([]));
    assert_eq!(route["route"]["rule_set"], json!([]));

    let presets = recommended_dns_presets();
    assert!(
        presets
            .iter()
            .any(|p| p.region == DnsPresetRegion::Domestic)
    );
    assert!(
        presets
            .iter()
            .any(|p| p.region == DnsPresetRegion::International)
    );
    for preset in presets {
        let entry = preset.to_server_json(None);
        assert_eq!(entry["tag"], preset.tag);
        assert_eq!(entry["type"], preset.server_type);
        assert_eq!(entry["server"], preset.server);
    }
}

#[test]
fn compose_applies_outbound_node_filter_strips_metadata_and_handles_empty_match() {
    let mut bundle = SingboxOverwriteBundle::default();
    let hk_preset = recommended_region_presets()
        .iter()
        .find(|p| p.tag == "HK")
        .unwrap();
    let jp_preset = recommended_region_presets()
        .iter()
        .find(|p| p.tag == "JP")
        .unwrap();
    let mut hk = hk_preset.to_outbound_json("urltest");
    hk["_filter"]["exclude"] = json!("倍率|过期");
    let jp = jp_preset.to_outbound_json("selector");

    bundle.outbounds = json!({
        "outbounds": [
            {
                "type": "selector",
                "tag": "proxy",
                "outbounds": ["HK", "JP", "$nodes"],
                "default": "HK"
            },
            hk,
            jp,
            { "type": "direct", "tag": "direct" }
        ]
    });
    validate_slice(SingboxSliceKind::Outbounds, &bundle.outbounds).unwrap();
    assert_eq!(
        extract_outbound_tags(&bundle.outbounds),
        vec!["proxy", "HK", "JP", "direct"]
    );

    let sub_nodes = vec![
        json!({ "type": "shadowsocks", "tag": "🇭🇰 香港 01", "server": "192.0.2.1", "server_port": 8388 }),
        json!({ "type": "shadowsocks", "tag": "HK 02 高倍率", "server": "192.0.2.2", "server_port": 8388 }),
        json!({ "type": "shadowsocks", "tag": "🇺🇸 美国 01", "server": "192.0.2.3", "server_port": 8388 }),
    ];

    let composed = compose_from_overwrite(&sub_nodes, &bundle).unwrap();
    let outbounds = composed["outbounds"].as_array().unwrap();

    // `_filter` 必须完全从最终 sing-box 产物中剥离。
    for item in outbounds {
        assert!(
            item.get(OUTBOUND_FILTER_KEY).is_none(),
            "leaked _filter in {item}"
        );
    }

    // HK 命中「🇭🇰 香港 01」，排除「HK 02 高倍率」与美国节点。
    let hk_out = outbounds.iter().find(|o| o["tag"] == "HK").unwrap();
    assert_eq!(hk_out["outbounds"], json!(["🇭🇰 香港 01"]));

    // JP 无匹配节点时安全回退 direct，保证 sing-box schema 合法。
    let jp_out = outbounds.iter().find(|o| o["tag"] == "JP").unwrap();
    assert_eq!(jp_out["outbounds"], json!(["direct"]));
}

#[test]
fn validate_rejects_invalid_outbound_filter_regex() {
    let bad = r#"{
        "outbounds": [
            {
                "type": "urltest",
                "tag": "bad-re",
                "outbounds": ["$nodes"],
                "_filter": { "include": "[unclosed" }
            }
        ]
    }"#;
    assert!(parse_and_validate_slice(SingboxSliceKind::Outbounds, bad).is_err());
}
