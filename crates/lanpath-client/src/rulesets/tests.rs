use axum::{
    Router,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use super::*;
use crate::ClientConfig;

fn manual_entry(id: &str, tag: &str, domain: &str) -> RuleSetEntry {
    RuleSetEntry {
        id: id.into(),
        tag: tag.into(),
        name: format!("Name {tag}"),
        category: "custom".into(),
        source: RuleSetSource::Manual {
            content: json!({
                "version": 2,
                "rules": [{ "domain_suffix": [domain] }]
            })
            .to_string(),
        },
        remote_state: RuleSetRemoteState::default(),
        created_at: 1_700_000_000,
    }
}

#[test]
fn store_saves_manual_files_cleans_orphans_and_rejects_duplicate_tags() {
    let dir = tempfile::tempdir().unwrap();
    let store = RuleSetStore::new(dir.path().to_path_buf());
    assert!(store.load().unwrap().is_empty());
    assert!(!store.manifest_path().exists());

    let a = manual_entry("rs-1", "corp-a", "a.internal");
    let b = manual_entry("rs-2", "corp-b", "b.internal");
    store.save(&[a.clone(), b.clone()]).unwrap();
    assert!(store.has_backing_file(&a));
    assert!(store.has_backing_file(&b));

    let loaded = store.load().unwrap();
    assert_eq!(loaded.len(), 2);

    // Deleting b cleans up rs-2.json automatically.
    store.save(std::slice::from_ref(&a)).unwrap();
    assert!(store.has_backing_file(&a));
    assert!(!store.has_backing_file(&b));

    // Duplicate tags are rejected case-insensitively.
    let dup = manual_entry("rs-3", "CORP-A", "dup.internal");
    assert!(store.save(&[a, dup]).is_err());
}

#[test]
fn apply_managed_rule_sets_injects_referenced_dedups_and_reports_missing() {
    let dir = tempfile::tempdir().unwrap();
    let store = RuleSetStore::new(dir.path().to_path_buf());
    let local = manual_entry("rs-local", "my-corp", "corp.test");
    let missing_remote = RuleSetEntry {
        id: "rs-miss".into(),
        tag: "missing-set".into(),
        name: "Missing".into(),
        category: "proxy".into(),
        source: RuleSetSource::Remote {
            url: "https://example.invalid/missing.srs".into(),
            format: RuleSetFormat::Binary,
        },
        remote_state: RuleSetRemoteState::default(),
        created_at: 1_700_000_000,
    };
    let unused = manual_entry("rs-unused", "unused-set", "unused.test");
    store
        .save(&[local.clone(), missing_remote.clone(), unused.clone()])
        .unwrap();

    let mut config = json!({
        "dns": {
            "rules": [{ "rule_set": "missing-set", "server": "proxy" }]
        },
        "route": {
            "rules": [{ "rule_set": ["my-corp"], "outbound": "direct" }],
            "rule_set": [
                { "type": "remote", "tag": "my-corp", "format": "source", "url": "https://old.invalid" }
            ]
        }
    });

    let missing =
        apply_managed_rule_sets(dir.path(), &mut config, &[local, missing_remote, unused]);
    assert_eq!(missing, vec!["missing-set".to_owned()]);

    let sets = config["route"]["rule_set"].as_array().unwrap();
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0]["type"], "local");
    assert_eq!(sets[0]["tag"], "my-corp");
    assert_eq!(sets[0]["format"], "source");
    assert_eq!(sets[0]["path"], "rulesets/files/rs-local.json");
}

#[tokio::test]
async fn update_one_downloads_caches_and_honors_304_etag() {
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_clone = hits.clone();
    let app = Router::new().route(
        "/cn.srs",
        get(move |headers: HeaderMap| {
            let hits = hits_clone.clone();
            async move {
                hits.fetch_add(1, Ordering::SeqCst);
                if headers
                    .get(header::IF_NONE_MATCH)
                    .and_then(|v| v.to_str().ok())
                    == Some("\"v1\"")
                {
                    return StatusCode::NOT_MODIFIED.into_response();
                }
                ([(header::ETAG, "\"v1\"")], b"SRS-BINARY-FIXTURE".as_slice()).into_response()
            }
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let dir = tempfile::tempdir().unwrap();
    let mut cfg = ClientConfig::new(dir.path().to_path_buf(), "", "", PathBuf::new());
    cfg.request_proxy_mode = crate::RequestProxyMode::None;

    let mut entry = RuleSetEntry {
        id: "rs-cn".into(),
        tag: "geosite-cn".into(),
        name: "CN".into(),
        category: "cn".into(),
        source: RuleSetSource::Remote {
            url: format!("http://{addr}/cn.srs"),
            format: RuleSetFormat::Binary,
        },
        remote_state: RuleSetRemoteState::default(),
        created_at: 1_700_000_000,
    };

    let first = update_one(dir.path(), &mut entry, Some(&cfg))
        .await
        .unwrap();
    assert_eq!(first, RuleSetUpdateStatus::Updated);
    assert_eq!(entry.remote_state.etag.as_deref(), Some("\"v1\""));
    assert!(RuleSetStore::new(dir.path().to_path_buf()).has_backing_file(&entry));

    let second = update_one(dir.path(), &mut entry, Some(&cfg))
        .await
        .unwrap();
    assert_eq!(second, RuleSetUpdateStatus::Skipped);
    assert_eq!(hits.load(Ordering::SeqCst), 2);

    server.abort();
}

#[test]
fn market_catalog_has_unique_valid_tags_and_mirror_urls() {
    let mut seen = HashSet::new();
    for spec in market_catalog() {
        validate_rule_set_tag(spec.tag).unwrap();
        assert!(seen.insert(spec.tag), "duplicate market tag: {}", spec.tag);
        assert!(
            spec.url_for_mirror(MarketMirror::JsDelivr)
                .starts_with("https://cdn.jsdelivr.net/")
        );
        assert!(
            spec.url_for_mirror(MarketMirror::GitHubRaw)
                .starts_with("https://raw.githubusercontent.com/")
        );
    }
    assert!(seen.contains("geosite-private"));
    assert!(seen.contains("geoip-private"));
}

#[test]
fn expand_rule_set_groups_expands_route_and_dns_and_warns_on_unknown() {
    let groups = vec![RuleSetGroup {
        id: "grp-1".into(),
        tag: "direct-cn".into(),
        name: "直连合集".into(),
        description: String::new(),
        member_tags: vec![
            "geosite-private".into(),
            "geosite-cn".into(),
            "geoip-cn".into(),
        ],
        created_at: 1_700_000_000,
    }];
    let mut config = json!({
        "dns": {
            "rules": [
                { "rule_set": "@direct-cn", "server": "local" }
            ]
        },
        "route": {
            "rules": [
                { "rule_set": ["@direct-cn", "geosite-cn", "custom-a"], "outbound": "direct" },
                { "rule_set": "@missing-group", "outbound": "proxy" }
            ]
        }
    });

    let warnings = expand_rule_set_groups(&mut config, &groups);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("@missing-group"));
    assert_eq!(
        config["dns"]["rules"][0]["rule_set"],
        json!(["geosite-private", "geosite-cn", "geoip-cn"])
    );
    let route_rules = config["route"]["rules"].as_array().unwrap();
    assert_eq!(route_rules.len(), 1);
    assert_eq!(
        route_rules[0]["rule_set"],
        json!(["geosite-private", "geosite-cn", "geoip-cn", "custom-a"])
    );
}

#[test]
fn manifest_roundtrips_items_and_groups_and_rejects_tag_collision() {
    let dir = tempfile::tempdir().unwrap();
    let store = RuleSetStore::new(dir.path().to_path_buf());
    let item = manual_entry("rs-1", "corp-a", "a.internal");
    let group = RuleSetGroup {
        id: "grp-1".into(),
        tag: "corp-all".into(),
        name: "公司合集".into(),
        description: "全部内网".into(),
        member_tags: vec!["corp-a".into()],
        created_at: 1_700_000_000,
    };
    let manifest = RuleSetManifest {
        version: MANIFEST_VERSION,
        items: vec![item.clone()],
        groups: vec![group.clone()],
    };
    store.save_manifest(&manifest).unwrap();
    let loaded = store.load_manifest().unwrap();
    assert_eq!(loaded.items, vec![item.clone()]);
    assert_eq!(loaded.groups, vec![group]);

    // 分组标签与规则集标签同名时拒绝。
    let colliding = RuleSetManifest {
        version: MANIFEST_VERSION,
        items: vec![item],
        groups: vec![RuleSetGroup {
            id: "grp-2".into(),
            tag: "corp-a".into(),
            name: "Collision".into(),
            description: String::new(),
            member_tags: vec![],
            created_at: 1_700_000_000,
        }],
    };
    assert!(store.save_manifest(&colliding).is_err());
}
