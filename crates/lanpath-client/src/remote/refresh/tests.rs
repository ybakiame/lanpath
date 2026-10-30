use super::*;
use crate::remote::{RemoteKind, RemoteResourceService};
use axum::{
    extract::{Path as UrlPath, State},
    http::StatusCode,
    routing::get,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::Notify;

#[derive(Default)]
struct HttpState {
    responses: Mutex<BTreeMap<String, (StatusCode, String)>>,
    requests: AtomicUsize,
    entered: Notify,
    release: Notify,
    responded: Notify,
}

struct GateCompletion(Arc<HttpState>);

impl Drop for GateCompletion {
    fn drop(&mut self) {
        self.0.responded.notify_one();
    }
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
        let app = axum::Router::new().route("/{*path}", get(
            |State(state): State<Arc<HttpState>>, UrlPath(path): UrlPath<String>| async move {
                state.requests.fetch_add(1, Ordering::SeqCst);
                if path == "gated.js" {
                    let _completion = GateCompletion(state.clone());
                    state.entered.notify_one();
                    state.release.notified().await;
                }
                state.responses.lock().unwrap().get(&path).cloned()
                    .unwrap_or((StatusCode::NOT_FOUND, "private-token-in-error-body".into()))
            },
        )).with_state(state.clone());
        let worker = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            base,
            state,
            worker,
        }
    }

    fn set(&self, path: &str, status: StatusCode, body: impl Into<String>) {
        self.state
            .responses
            .lock()
            .unwrap()
            .insert(path.into(), (status, body.into()));
    }

    fn resource(&self, name: &str, path: &str) -> RemoteResource {
        RemoteResource {
            name: name.into(),
            url: format!("{}/{path}?token=private-query-token", self.base),
            update_interval_secs: 10,
            ..Default::default()
        }
    }

    fn requests(&self) -> usize {
        self.state.requests.load(Ordering::SeqCst)
    }
}

fn stopped_context() -> RemoteFetchContext {
    RemoteFetchContext::stopped(crate::ClientConfig {
        request_proxy_mode: crate::RequestProxyMode::None,
        ..Default::default()
    })
    .unwrap()
}

async fn run(
    manager: &RemoteManager,
    remotes: &[RemoteResource],
    automatic: bool,
    now: u64,
) -> Option<FetchReport> {
    let context = stopped_context();
    fetch_at(manager, remotes, automatic.then_some(&context), || now)
        .await
        .unwrap()
}

#[tokio::test]
async fn due_boundary_survives_reopen_and_manual_attempts_postpone_automatic() {
    let server = Server::start().await;
    server.set("script.js", StatusCode::OK, "const original = true;");
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().into());
    let remote = server.resource("script", "script.js");
    let resources = [remote];
    assert_eq!(
        run(&manager, &resources, true, 100).await.unwrap().fetched,
        1
    );
    assert_eq!(server.requests(), 1);
    drop(manager);
    let manager = RemoteManager::new(root.path().into());
    assert!(run(&manager, &resources, true, 109).await.is_none());
    assert_eq!(
        run(&manager, &resources, true, 110).await.unwrap().fetched,
        1
    );
    // A forced manual refresh is not subject to the automatic interval.
    assert_eq!(
        run(&manager, &resources, false, 111).await.unwrap().fetched,
        1
    );
    assert!(run(&manager, &resources, true, 120).await.is_none());
    assert_eq!(
        run(&manager, &resources, true, 121).await.unwrap().fetched,
        1
    );
    assert_eq!(server.requests(), 4);
    // An actual rollback is eligible instead of freezing until the old time catches up.
    assert_eq!(
        run(&manager, &resources, true, 90).await.unwrap().fetched,
        1
    );
    let schedule = std::fs::read_to_string(root.path().join("remote_refresh.json")).unwrap();
    assert!(!schedule.contains("private-query-token"));
    assert!(!schedule.contains(&server.base));
}

#[tokio::test]
async fn disabled_resources_and_all_identity_components_control_real_fetches() {
    let server = Server::start().await;
    // The same body is acceptable as both a script and a minimal Surge module.
    server.set(
        "resource",
        StatusCode::OK,
        "#!name=module\n[MITM]\nhostname=example.invalid",
    );
    server.set("other", StatusCode::OK, "[MITM]\nhostname=other.invalid");
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().into());
    let mut remote = server.resource("one", "resource");
    remote.enabled = false;
    assert!(run(&manager, &[remote.clone()], true, 100).await.is_none());
    assert_eq!(
        run(&manager, &[remote.clone()], false, 100)
            .await
            .unwrap()
            .fetched,
        0
    );
    assert_eq!(server.requests(), 0);
    remote.enabled = true;
    assert_eq!(
        run(&manager, &[remote.clone()], true, 100)
            .await
            .unwrap()
            .fetched,
        1
    );
    remote.update_interval_secs = 20;
    assert!(run(&manager, &[remote.clone()], true, 110).await.is_none());
    remote.name = "two".into();
    assert_eq!(
        run(&manager, &[remote.clone()], true, 110)
            .await
            .unwrap()
            .fetched,
        1
    );
    remote.kind = RemoteKind::Snippet;
    assert_eq!(
        run(&manager, &[remote.clone()], true, 110)
            .await
            .unwrap()
            .fetched,
        1
    );
    remote.dialect = lanpath_script::ScriptDialect::Loon;
    assert_eq!(
        run(&manager, &[remote.clone()], true, 110)
            .await
            .unwrap()
            .fetched,
        1
    );
    remote.url = format!("{}/other", server.base);
    assert_eq!(
        run(&manager, &[remote.clone()], true, 110)
            .await
            .unwrap()
            .fetched,
        1
    );
    assert_eq!(server.requests(), 5);
    remote.update_interval_secs = 0;
    assert!(run(&manager, &[remote.clone()], true, 111).await.is_none());
    assert_eq!(
        run(&manager, &[remote], true, 86_510)
            .await
            .unwrap()
            .fetched,
        1
    );
}

#[tokio::test]
async fn failed_completed_attempt_preserves_script_and_waits_its_interval() {
    let server = Server::start().await;
    server.set("script.js", StatusCode::OK, "const working = true;");
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().into());
    let resources = [server.resource("script", "script.js")];
    assert_eq!(
        run(&manager, &resources, true, 100).await.unwrap().fetched,
        1
    );
    let path = root.path().join("scripts/script.js");
    let original = std::fs::read(&path).unwrap();
    server.set("script.js", StatusCode::BAD_GATEWAY, "private-body-token");
    let report = run(&manager, &resources, true, 110).await.unwrap();
    assert_eq!(report.fetched, 0);
    assert!(!report.warnings.is_empty());
    let warnings = report.warnings.join(" ");
    assert!(!warnings.contains("private"));
    assert!(!warnings.contains(&server.base));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let reopened = RemoteManager::new(root.path().into());
    assert!(run(&reopened, &resources, true, 119).await.is_none());
    assert_eq!(server.requests(), 2);
    assert_eq!(
        run(&reopened, &resources, true, 120).await.unwrap().fetched,
        0
    );
    assert_eq!(
        run(&reopened, &resources, false, 121)
            .await
            .unwrap()
            .fetched,
        0
    );
    assert!(run(&reopened, &resources, true, 130).await.is_none());
    assert_eq!(
        run(&reopened, &resources, true, 131).await.unwrap().fetched,
        0
    );
}

#[tokio::test]
async fn automatic_hook_and_task_failures_retain_complete_module_but_not_all_warnings_are_fatal() {
    let server = Server::start().await;
    let module = format!(
        "[Script]\nhook=type=http-request,pattern=^https://example\\.invalid/,script-path={}/hook.js\ntask=type=cron,cronexp=\"0 0 * * *\",script-path={}/task.js\n[MITM]\nhostname=example.invalid\n[Unknown]\nprivate-unsupported-token\n",
        server.base, server.base,
    );
    server.set("module", StatusCode::OK, module.clone());
    server.set("hook.js", StatusCode::OK, "const hook = true;");
    server.set("task.js", StatusCode::OK, "const task = true;");
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().into());
    let mut remote = server.resource("module", "module");
    remote.kind = RemoteKind::Snippet;
    manager.save(&[remote.clone()]).unwrap();
    let report = run(&manager, &[remote.clone()], true, 100).await.unwrap();
    assert_eq!((report.fetched, report.scripts, report.tasks), (1, 1, 1));
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("unsupported"))
    );
    assert!(
        !report
            .warnings
            .join(" ")
            .contains("private-unsupported-token")
    );
    let id = crate::remote::activation::remote_module_id(&remote).replace(':', "-");
    let path = root.path().join(format!("remote_cache/{id}.json"));
    let complete = std::fs::read(&path).unwrap();
    for (now, failed) in [(110, "hook.js"), (120, "task.js")] {
        server.set("hook.js", StatusCode::OK, "const hook = true;");
        server.set("task.js", StatusCode::OK, "const task = true;");
        server.set(failed, StatusCode::NOT_FOUND, "private-source-token");
        let report = run(&manager, &[remote.clone()], true, now).await.unwrap();
        assert_eq!(report.fetched, 0);
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("previous complete cache retained"))
        );
        assert!(!report.warnings.join(" ").contains("private"));
        assert_eq!(std::fs::read(&path).unwrap(), complete);
        let active = manager.load_cached().unwrap();
        assert_eq!((active.scripts.len(), active.task_scripts.len()), (1, 1));
    }
    server.set("module", StatusCode::OK, "");
    assert_eq!(
        run(&manager, &[remote.clone()], true, 130)
            .await
            .unwrap()
            .fetched,
        0
    );
    assert_eq!(std::fs::read(&path).unwrap(), complete);
    server.set("module", StatusCode::OK, module);
    // Manual partial-source behavior remains explicit and compatible.
    assert_eq!(
        run(&manager, &[remote], false, 131).await.unwrap().fetched,
        1
    );
    assert!(manager.load_cached().unwrap().task_scripts.is_empty());
}

#[tokio::test]
async fn cancelled_dependent_source_keeps_cache_and_completed_attempt_age() {
    let server = Server::start().await;
    let module = format!(
        "[Script]\nhook=type=http-request,pattern=^https://example\\.invalid/,script-path={}/hook.js",
        server.base,
    );
    server.set("module", StatusCode::OK, module.clone());
    server.set("hook.js", StatusCode::OK, "const complete = true;");
    server.set("gated.js", StatusCode::OK, "const replacement = true;");
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().into());
    let mut remote = server.resource("module", "module");
    remote.kind = RemoteKind::Snippet;
    let resources = [remote];
    assert_eq!(
        run(&manager, &resources, true, 100).await.unwrap().fetched,
        1
    );
    let id = crate::remote::activation::remote_module_id(&resources[0]).replace(':', "-");
    let cache = root.path().join(format!("remote_cache/{id}.json"));
    let schedule = root.path().join("remote_refresh.json");
    let original_cache = std::fs::read(&cache).unwrap();
    let original_schedule = std::fs::read(&schedule).unwrap();
    server.set(
        "module",
        StatusCode::OK,
        module.replace("/hook.js", "/gated.js"),
    );
    let context = stopped_context();
    let mut automatic = Box::pin(fetch_at(&manager, &resources, Some(&context), || 110));
    tokio::select! {
        result = &mut automatic => panic!("automatic unexpectedly completed: {result:?}"),
        () = server.state.entered.notified() => {},
    }
    drop(automatic);
    server.state.release.notify_one();
    server.state.responded.notified().await;
    assert_eq!(std::fs::read(&cache).unwrap(), original_cache);
    assert_eq!(std::fs::read(&schedule).unwrap(), original_schedule);
    server.set("module", StatusCode::OK, module);
    let reopened = RemoteManager::new(root.path().into());
    assert_eq!(
        run(&reopened, &resources, true, 110).await.unwrap().fetched,
        1
    );
}

#[tokio::test]
async fn corrupt_schedule_is_preserved_and_never_discloses_its_content() {
    let server = Server::start().await;
    server.set("script.js", StatusCode::OK, "const script = true;");
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().into());
    let path = root.path().join("remote_refresh.json");
    let corrupt = b"{private-ledger-token";
    std::fs::write(&path, corrupt).unwrap();
    let context = stopped_context();
    for automatic in [true, false] {
        let error = fetch_at(
            &manager,
            &[server.resource("script", "script.js")],
            automatic.then_some(&context),
            || 100,
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(!error.contains("private-ledger-token"));
        assert_eq!(std::fs::read(&path).unwrap(), corrupt);
    }
    assert_eq!(server.requests(), 0);
}

#[tokio::test]
async fn shared_writer_serializes_manual_auto_and_cancellation_has_no_late_publication() {
    let server = Server::start().await;
    server.set("gated.js", StatusCode::OK, "const late = true;");
    server.set("next.js", StatusCode::OK, "const next = true;");
    let root = tempfile::tempdir().unwrap();
    let service = RemoteResourceService::new(root.path());
    let context = stopped_context();
    service
        .add(server.resource("gated", "gated.js"))
        .await
        .unwrap();
    service
        .add(server.resource("next", "next.js"))
        .await
        .unwrap();
    // Poll a real accepted manual operation until its request is in flight.
    let mut manual = Box::pin(service.fetch_all());
    tokio::select! {
        result = &mut manual => panic!("manual unexpectedly completed: {result:?}"),
        () = server.state.entered.notified() => {},
    }
    assert!(service.fetch_due(&context).await.unwrap().is_none());
    server.state.release.notify_one();
    assert_eq!(manual.await.unwrap().fetched, 2);
    server.state.responded.notified().await;
    assert!(service.fetch_due(&context).await.unwrap().is_none());
    // Force the same resources due without modifying the user-editable manifest.
    std::fs::remove_file(root.path().join("remote_refresh.json")).unwrap();
    let script = root.path().join("scripts/gated.js");
    std::fs::write(&script, "const retained = true;").unwrap();
    let before = server.requests();
    // Any unrelated service writer from another test is allowed to finish first.
    // Holding/releasing the lock in this task makes the initial automatic poll deterministic.
    let guard = crate::remote::service::WRITE_LOCK.lock().await;
    let mut automatic = Box::pin(service.fetch_due(&context));
    drop(guard);
    tokio::select! {
        result = &mut automatic => panic!("automatic unexpectedly completed: {result:?}"),
        () = server.state.entered.notified() => {},
    }
    let mut queued_manual = Box::pin(service.fetch_all());
    assert!(matches!(
        futures::poll!(queued_manual.as_mut()),
        std::task::Poll::Pending
    ));
    drop(queued_manual);
    drop(automatic);
    // No schedule exists: the cancelled first resource was never completed, and
    // the next resource was never started.
    assert!(!root.path().join("remote_refresh.json").exists());
    assert_eq!(server.requests(), before + 1);
    assert_eq!(
        std::fs::read_to_string(&script).unwrap(),
        "const retained = true;"
    );
    // Let the old handler finish (or observe its connection cancellation). There
    // is no detached client continuation that can publish after the drop.
    server.state.release.notify_one();
    server.state.responded.notified().await;
    let reopened = RemoteResourceService::new(root.path());
    reopened.set_enabled("gated", false).await.unwrap();
    let report = reopened.fetch_due(&context).await.unwrap().unwrap();
    assert_eq!(report.fetched, 1);
    assert_eq!(
        std::fs::read_to_string(&script).unwrap(),
        "const retained = true;"
    );
    let manager = RemoteManager::new(root.path().into());
    let schedule = Schedule::load(&root.path().join("remote_refresh.json")).unwrap();
    let resources = manager.load().unwrap();
    assert!(!schedule.attempts.contains_key(&identity(&resources[0])));
    assert!(schedule.attempts.contains_key(&identity(&resources[1])));
}

#[tokio::test]
async fn optional_icon_cannot_starve_automatic_module_activation() {
    let server = Server::start().await;
    server.set(
        "module",
        StatusCode::OK,
        "[MITM]\nhostname=api.owned.example\n",
    );
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().into());
    let mut remote = server.resource("module", "module");
    remote.kind = RemoteKind::Snippet;
    remote.icon = Some(format!("{}/gated.js", server.base));
    manager.save(std::slice::from_ref(&remote)).unwrap();
    let report = tokio::time::timeout(Duration::from_secs(5), run(&manager, &[remote], true, 100))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(report.fetched, 1);
    assert_eq!(
        manager.load_cached().unwrap().hostnames,
        ["api.owned.example"]
    );
}

mod live;
