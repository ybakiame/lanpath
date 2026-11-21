use axum::{extract::State, http::Uri, routing::get};
use lanpath_client::profile::{
    ProfileStoreV2, apply_yaml_override, resolve_remote_overrides,
    resolve_remote_overrides_with_context,
};
use lanpath_client::rulesets::{
    RuleSetEntry, RuleSetFormat, RuleSetSource, RuleSetStore, prepare_referenced_missing,
};
use lanpath_client::{
    ClientConfig, RemoteFetchContext, RemoteResource, RemoteResourceService, RequestProxyMode,
};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[derive(Default)]
struct HttpState {
    requests: AtomicUsize,
    change_policy: Mutex<Option<ClientConfig>>,
}
struct Server {
    base: String,
    state: Arc<HttpState>,
    worker: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.worker.abort();
    }
}
impl Server {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(HttpState::default());
        let app = axum::Router::new()
            .fallback(get(
                |State(state): State<Arc<HttpState>>, uri: Uri| async move {
                    state.requests.fetch_add(1, Ordering::SeqCst);
                    if uri.path().ends_with(".yaml") {
                        if let Some(mut config) = state.change_policy.lock().unwrap().take() {
                            config.request_proxy_mode = RequestProxyMode::None;
                            config.save().unwrap();
                        }
                        "log:\n  level: debug\n".to_owned()
                    } else {
                        json!({"version":3,"rules":[{"domain_suffix":["accepted.owned"]}]})
                            .to_string()
                    }
                },
            ))
            .with_state(state.clone());
        let worker = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            base,
            state,
            worker,
        }
    }
    fn requests(&self) -> usize {
        self.state.requests.load(Ordering::SeqCst)
    }
    fn port(&self) -> u16 {
        reqwest::Url::parse(&self.base).unwrap().port().unwrap()
    }
}
fn config(root: &Path) -> ClientConfig {
    let mut config = ClientConfig::new(root.to_path_buf(), "", "", Default::default());
    config.request_proxy_mode = RequestProxyMode::None;
    config.github_proxy_prefix.clear();
    config.save().unwrap();
    config
}
fn resource(id: &str, source: RuleSetSource) -> RuleSetEntry {
    RuleSetEntry {
        id: id.into(),
        name: id.into(),
        tag: id.into(),
        source,
        category: String::new(),
        remote_state: Default::default(),
        created_at: 0,
    }
}

#[tokio::test]
async fn core_unavailable_uses_profile_cache_without_borrowing_listener_or_replacing_content() {
    let origin = Server::start().await;
    let foreign = Server::start().await;
    let root = tempfile::tempdir().unwrap();
    let mut saved = config(root.path());
    let mut profile = ProfileStoreV2::new(root.path().into())
        .add("owned")
        .unwrap();
    profile.yaml_url = Some(format!("{}/override.yaml", origin.base));
    profile.yaml_override = "log:\n  timestamp: true\n".into();
    let cache = root.path().join("profile_cache");
    let (_, warnings) = resolve_remote_overrides(&cache, &profile).await;
    assert!(warnings.is_empty());
    let path = cache.join(format!("{}.yaml", profile.id));
    let before = std::fs::read(&path).unwrap();
    saved.request_proxy_mode = RequestProxyMode::Core;
    saved.mixed_port = foreign.port();
    saved.save().unwrap();
    let context = RemoteFetchContext::cache_only();
    let (effective, warnings) = tokio::time::timeout(
        Duration::from_secs(2),
        resolve_remote_overrides_with_context(&cache, &profile, &context),
    )
    .await
    .unwrap();
    assert!(warnings.is_empty());
    let merged = apply_yaml_override(json!({}), &effective.remote_yaml).unwrap();
    let merged = apply_yaml_override(merged, &effective.local_yaml).unwrap();
    assert_eq!(merged["log"], json!({"level":"debug","timestamp":true}));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::remove_file(&path).unwrap();
    let (effective, warnings) =
        resolve_remote_overrides_with_context(&cache, &profile, &context).await;
    assert!(effective.remote_yaml.is_empty());
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        apply_yaml_override(json!({}), &effective.local_yaml).unwrap()["log"]["timestamp"],
        true
    );
    assert!(!path.exists());
    assert_eq!(origin.requests(), 1);
    assert_eq!(foreign.requests(), 0);
}

#[tokio::test]
async fn profile_and_rules_share_admitted_custom_policy_after_saved_policy_changes() {
    let origin = Server::start().await;
    let proxy = Server::start().await;
    let root = tempfile::tempdir().unwrap();
    let mut saved = config(root.path());
    saved.request_proxy_mode = RequestProxyMode::Custom;
    saved.request_proxy_url = proxy.base.clone();
    saved.save().unwrap();
    *proxy.state.change_policy.lock().unwrap() = Some(saved.clone());
    let context = RemoteFetchContext::stopped(saved).unwrap();
    let mut profile = ProfileStoreV2::new(root.path().into())
        .add("owned")
        .unwrap();
    profile.yaml_url = Some(format!("{}/override.yaml", origin.base));
    let (effective, warnings) = resolve_remote_overrides_with_context(
        &root.path().join("profile_cache"),
        &profile,
        &context,
    )
    .await;
    assert!(warnings.is_empty());
    assert_eq!(
        apply_yaml_override(json!({}), &effective.remote_yaml).unwrap()["log"]["level"],
        "debug"
    );
    assert_eq!(
        ClientConfig::load(root.path()).unwrap().request_proxy_mode,
        RequestProxyMode::None
    );
    let store = RuleSetStore::new(root.path().into());
    let mut entries = vec![resource(
        "owned",
        RuleSetSource::Remote {
            url: format!("{}/rules.json", origin.base),
            format: RuleSetFormat::Source,
        },
    )];
    store.save(&entries).unwrap();
    let cfg = json!({ "route": { "rules": [{ "rule_set": "owned", "outbound": "direct" }] } });
    let (changed, warnings) =
        prepare_referenced_missing(root.path(), &cfg, &mut entries, &context).await;
    assert!(changed);
    assert!(warnings.is_empty());
    let path = store.backing_file_path("owned", RuleSetFormat::Source);
    let downloaded: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        downloaded["rules"][0]["domain_suffix"],
        json!(["accepted.owned"])
    );
    assert_eq!(proxy.requests(), 2);
    assert_eq!(origin.requests(), 0);
}

#[tokio::test]
async fn cache_only_restores_manual_rules_but_never_downloads_or_ages_remote_work() {
    let foreign = Server::start().await;
    let root = tempfile::tempdir().unwrap();
    let mut saved = config(root.path());
    saved.request_proxy_mode = RequestProxyMode::Core;
    saved.mixed_port = foreign.port();
    saved.save().unwrap();
    let store = RuleSetStore::new(root.path().into());
    let mut entries = vec![
        resource(
            "manual",
            RuleSetSource::Manual {
                content: json!({"version":3,"rules":[{"domain":["local.owned"]}]}).to_string(),
            },
        ),
        resource(
            "missing",
            RuleSetSource::Remote {
                url: format!("{}/rules.json", foreign.base),
                format: RuleSetFormat::Source,
            },
        ),
    ];
    store.save(&entries).unwrap();
    let manual = store.backing_file_path("manual", RuleSetFormat::Source);
    std::fs::remove_file(&manual).unwrap();
    let context = RemoteFetchContext::cache_only();
    let cfg = json!({
        "route": {
            "rules": [
                { "rule_set": "manual", "outbound": "direct" },
                { "rule_set": "missing", "outbound": "direct" }
            ]
        }
    });
    let _ = prepare_referenced_missing(root.path(), &cfg, &mut entries, &context).await;
    let restored: Value = serde_json::from_slice(&std::fs::read(manual).unwrap()).unwrap();
    assert_eq!(restored["rules"][0]["domain"], json!(["local.owned"]));
    assert!(
        !store
            .backing_file_path("missing", RuleSetFormat::Source)
            .exists()
    );
    let remotes = RemoteResourceService::new(root.path());
    remotes
        .add(RemoteResource {
            name: "owned".into(),
            url: format!("{}/owned.js", foreign.base),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(remotes.fetch_due(&context).await.is_err());
    assert!(!root.path().join("remote_refresh.json").exists());
    assert!(!root.path().join("scripts/owned.js").exists());
    assert_eq!(foreign.requests(), 0);
}
