//! `ruleset_manager` 单元测试（下载路径以本地 axum 服务模拟镜像）。

use std::collections::BTreeMap;

use serde_json::json;

use super::*;

fn data_dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

/// 已有本地文件（非空）直接可用（下载桩不应被调用）；下载失败的 tag 记为缺失。
#[tokio::test]
async fn ensure_uses_existing_files_and_marks_missing() {
    let dir = data_dir();
    let path = rule_set_path(dir.path(), "geosite-private");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"local-cache").unwrap();

    let available =
        ensure_with_downloader(dir.path(), |_rel| async move { Err("offline".to_string()) }).await;
    assert_eq!(available.get("geosite-private"), Some(&path));
    assert!(!available.contains_key("geoip-private"));
}

/// 下载桩全部成功 → 文件原子写入且全部可用。
#[tokio::test]
async fn ensure_writes_downloaded_files() {
    let dir = data_dir();
    let available = ensure_with_downloader(dir.path(), |_rel| async move {
        Ok(bytes::Bytes::from_static(b"srs"))
    })
    .await;
    assert_eq!(available.len(), builtin_tags().len());
    assert_eq!(
        std::fs::read(rule_set_path(dir.path(), "geoip-private")).unwrap(),
        b"srs"
    );
}

/// 镜像 URL 构成：前缀只作用于 GitHub raw（追加第三镜像），空前缀不追加。
#[test]
fn mirror_urls_composition() {
    let urls = mirror_urls("geosite/cn", "");
    assert_eq!(urls.len(), 2);
    assert!(urls[0].1.contains("jsdelivr.net"));
    assert!(urls[1].1.contains("raw.githubusercontent.com"));

    let urls = mirror_urls("geosite/cn", "https://gh-proxy.com");
    assert_eq!(urls.len(), 3);
    assert!(
        urls[2]
            .1
            .starts_with("https://gh-proxy.com/https://raw.githubusercontent.com/")
    );
}

/// 原子写入：父目录自动创建，内容完整。
#[test]
fn write_atomic_creates_parent_and_writes() {
    let dir = data_dir();
    let path = rule_set_path(dir.path(), "geoip-private");
    write_atomic(&path, b"content").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"content");
    // 重写覆盖正常。
    write_atomic(&path, b"new").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
}

/// 物化：可用 tag 改写为 local 路径；缺失 tag 的条目与引用规则全部移除（降级）。
#[test]
fn materialize_rewrites_available_and_strips_missing() {
    let dir = data_dir();
    let path = rule_set_path(dir.path(), "geosite-private");
    let mut available = BTreeMap::new();
    available.insert("geosite-private".to_string(), path.clone());

    let mut config = json!({
        "dns": {
            "rules": [
                { "query_type": ["HTTPS"], "action": "predefined", "rcode": "NOERROR" },
                { "rule_set": ["geosite-private"], "action": "route", "server": "local" },
                { "rule_set": ["geoip-private"], "query_type": ["A"], "action": "route", "server": "fakeip" }
            ]
        },
        "route": {
            "rule_set": [
                { "type": "remote", "tag": "geosite-private", "format": "binary", "url": "https://x/private.srs" },
                { "type": "remote", "tag": "geoip-private", "format": "binary", "url": "https://x/ip.srs" },
                { "type": "local", "tag": "geosite-telegram", "format": "binary", "path": "/user/t.srs" }
            ],
            "rules": [
                { "action": "sniff" },
                { "rule_set": ["geosite-private"], "outbound": "direct" },
                { "rule_set": ["geoip-private"], "outbound": "direct" },
                { "rule_set": ["geosite-telegram"], "outbound": "proxy" }
            ]
        }
    });

    let report = materialize_rule_sets(&mut config, dir.path(), &available);

    // geosite-private 改写为 local；geoip-private 缺失被移除；用户条目不动。
    let rule_sets = config["route"]["rule_set"].as_array().unwrap();
    assert_eq!(rule_sets.len(), 2);
    assert_eq!(rule_sets[0]["type"], "local");
    assert_eq!(rule_sets[0]["tag"], "geosite-private");
    assert_eq!(rule_sets[0]["path"], json!(path.to_string_lossy()));
    assert_eq!(rule_sets[1]["tag"], "geosite-telegram");

    // 引用 geoip-private 的路由规则被移除，其余保留。
    let route_rules = config["route"]["rules"].as_array().unwrap();
    assert_eq!(route_rules.len(), 3);
    assert_eq!(report.stripped_route_rules, 1);

    // 引用 geoip-private（缺失）的 DNS 规则被移除，其余保留。
    let dns_rules = config["dns"]["rules"].as_array().unwrap();
    assert_eq!(dns_rules.len(), 2);
    assert_eq!(report.stripped_dns_rules, 1);

    assert!(report.missing_tags.contains(&"geoip-private".to_string()));
    assert!(!report.missing_tags.contains(&"geosite-private".to_string()));
}

/// 无缺失时物化为纯改写（不移除任何规则）。
#[test]
fn materialize_no_missing_keeps_all_rules() {
    let dir = data_dir();
    let available: BTreeMap<String, PathBuf> = builtin_tags()
        .into_iter()
        .map(|tag| (tag.to_string(), rule_set_path(dir.path(), tag)))
        .collect();
    let mut config = json!({
        "route": {
            "rule_set": [{ "type": "remote", "tag": "geoip-private", "format": "binary", "url": "https://x" }],
            "rules": [{ "rule_set": ["geoip-private"], "outbound": "direct" }]
        }
    });
    let report = materialize_rule_sets(&mut config, dir.path(), &available);
    assert!(report.missing_tags.is_empty());
    assert_eq!(report.stripped_route_rules, 0);
    assert_eq!(config["route"]["rules"].as_array().unwrap().len(), 1);
    assert_eq!(config["route"]["rule_set"][0]["type"], "local");
}

#[test]
fn materialize_keeps_authoritative_local_private_rules_without_legacy_cache() {
    let dir = data_dir();
    let local = dir.path().join("user-private.json");
    std::fs::write(
        &local,
        r#"{"version":1,"rules":[{"domain_suffix":["owned.example"]}]}"#,
    )
    .unwrap();
    let mut config = json!({
        "dns": {"rules": [
            {"rule_set": ["geosite-private"], "server": "local"},
            {"rule_set": ["geoip-private"], "server": "remote"}
        ]},
        "route": {
            "rule_set": [
                {"type": "local", "tag": "geosite-private", "format": "source", "path": local},
                {"type": "remote", "tag": "geoip-private", "format": "binary", "url": "https://owned.invalid/missing.srs"}
            ],
            "rules": [
                {"rule_set": ["geosite-private"], "outbound": "direct"},
                {"rule_set": ["geoip-private"], "outbound": "proxy"}
            ]
        }
    });
    let report = materialize_rule_sets(&mut config, dir.path(), &BTreeMap::new());
    assert_eq!(
        config["route"]["rules"],
        json!([
            {"rule_set": ["geosite-private"], "outbound": "direct"}
        ])
    );
    assert_eq!(
        config["dns"]["rules"],
        json!([
            {"rule_set": ["geosite-private"], "server": "local"}
        ])
    );
    assert_eq!(
        config["route"]["rule_set"],
        json!([
            {"type": "local", "tag": "geosite-private", "format": "source", "path": local}
        ])
    );
    assert_eq!(report.missing_tags, ["geoip-private"]);
    assert_eq!(report.stripped_route_rules, 1);
    assert_eq!(report.stripped_dns_rules, 1);
}

#[test]
fn materialize_does_not_report_unused_builtin_tags_as_missing() {
    let dir = data_dir();
    let mut config = json!({
        "route": {"final": "direct", "rules": [{"domain": ["owned.example"], "outbound": "direct"}]}
    });
    let report = materialize_rule_sets(&mut config, dir.path(), &BTreeMap::new());
    assert!(report.missing_tags.is_empty());
    assert_eq!(report.stripped_route_rules, 0);
    assert_eq!(report.stripped_dns_rules, 0);
}
