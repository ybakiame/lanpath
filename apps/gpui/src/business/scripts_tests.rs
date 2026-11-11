use std::{path::Path, sync::Arc};

use lanpath_client::{
    ScriptDialect,
    remote::{
        RemoteKind, RemoteManager, RemoteResource, RemoteResourceService, ScriptTaskSession,
        TracingNotifier,
    },
};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    task::JoinHandle,
};

use super::BusinessService;
use crate::configuration::Bindings;

fn service(root: &Path, binary: Option<std::path::PathBuf>) -> BusinessService {
    BusinessService::new(root, binary, Bindings::new(17899, 19090))
}

async fn spawn_task_remote(expected_requests: usize) -> (String, JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let snippet = format!(
        "#!name=Fixture Tasks\n#!desc=Remote workflow fixture\n\n[Script]\nfixture-task = type=cron,cronexp=\"0 0 1 1 *\",script-path=http://{address}/task.js\n"
    );
    let server = tokio::spawn(async move {
        for _ in 0..expected_requests {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 2048];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            let (status, content_type, body) = if request.starts_with("GET /module.sgmodule ") {
                ("200 OK", "text/plain", snippet.as_str())
            } else if request.starts_with("GET /task.js ") {
                (
                    "200 OK",
                    "application/javascript",
                    "$done({source: 'remote'})",
                )
            } else {
                ("404 Not Found", "text/plain", "missing")
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    (format!("http://{address}/module.sgmodule"), server)
}

#[tokio::test]
async fn remote_resource_workflow_fetches_imports_and_runs_cached_task() {
    let (url, server) = spawn_task_remote(3).await;
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path(), None);
    let resource = RemoteResource {
        name: " fixture ".into(),
        url,
        kind: RemoteKind::Script,
        dialect: ScriptDialect::QuantumultX,
        update_interval_secs: 0,
        ..RemoteResource::default()
    };
    let mut resource = service.add_remote_resource(resource).await.unwrap();
    assert_eq!(resource.name, "fixture");

    let detected = service.detect_remote_resource(&resource.url).await.unwrap();
    assert_eq!(detected.kind, Some(RemoteKind::Snippet));
    assert_eq!(detected.dialect, Some(ScriptDialect::Surge));
    assert_eq!(
        detected.meta.and_then(|meta| meta.name),
        Some("Fixture Tasks".into())
    );
    resource.kind = detected.kind.unwrap();
    resource.dialect = detected.dialect.unwrap();
    service
        .update_remote_resource("fixture", resource)
        .await
        .unwrap();

    service
        .set_remote_resource_enabled("fixture", false)
        .await
        .unwrap();
    service
        .set_remote_resource_enabled("fixture", true)
        .await
        .unwrap();
    let report = service.fetch_remote_resources().await.unwrap();
    assert_eq!(report.fetched, 1);
    assert_eq!(report.tasks, 1);
    server.await.unwrap();

    let import = service
        .import_script_config(
            "[URL Rewrite]\n^https://old\\.example/ https://new.example/ 302\n",
            ScriptDialect::Surge,
        )
        .await
        .unwrap();
    assert_eq!(import.rewrites, 1);

    let session = ScriptTaskSession::start(
        root.path().to_path_buf(),
        Arc::new(TracingNotifier::new()),
        Arc::new(lanpath_client::http_exec::ReqwestHttpExecutor::for_data_dir(root.path())),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(session.list_tasks()[0].name, "fixture-task");
    let output = session.run_now(&session.list_tasks()[0].id).await.unwrap();
    assert!(output.contains("remote"), "{output}");
    assert!(session.list_tasks()[0].last_run.is_some());
    session.stop().await;

    service.remove_remote_resource("fixture").await.unwrap();
    assert!(service.remote_resources().unwrap().is_empty());
}

#[tokio::test]
#[ignore = "requires LANPATH_TEST_SING_BOX pointing to a real sing-box executable"]
async fn fetched_remote_task_runs_while_real_core_is_live() {
    use lanpath_client::core_config::wait_clash_api_ready;
    use lanpath_engine::{CoreManager, SingBoxProcessManager};

    let binary = std::env::var_os("LANPATH_TEST_SING_BOX")
        .map(std::path::PathBuf::from)
        .expect("set LANPATH_TEST_SING_BOX to a real sing-box executable");
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    std::fs::create_dir(root_path.join("cache")).unwrap();
    let service = service(&root_path, Some(binary.clone()));
    let (url, server) = spawn_task_remote(2).await;
    service
        .add_remote_resource(RemoteResource {
            name: "real-core-task".into(),
            url,
            kind: RemoteKind::Snippet,
            dialect: ScriptDialect::Surge,
            ..RemoteResource::default()
        })
        .await
        .unwrap();
    let report = service.fetch_remote_resources().await.unwrap();
    assert_eq!(report.tasks, 1);
    server.await.unwrap();

    let subscription = service
        .subscription_store()
        .add(
            "real core task fixture",
            "https://example.invalid/subscription",
            true,
            None,
        )
        .unwrap();
    service
        .subscription_store()
        .write_cached_content(
            subscription.id,
            &lanpath_client::CachedSubscriptionContent {
                format: lanpath_client::SubFormat::SingBoxJson,
                singbox_nodes: vec![serde_json::json!({
                    "type": "shadowsocks",
                    "tag": "fixture-node",
                    "server": "127.0.0.1",
                    "server_port": 8388,
                    "method": "aes-128-gcm",
                    "password": "fixture"
                })],
            },
        )
        .unwrap();
    service
        .set_active_subscription(Some(subscription.id))
        .unwrap();

    let session = ScriptTaskSession::start(
        root_path.clone(),
        Arc::new(TracingNotifier::new()),
        Arc::new(lanpath_client::http_exec::ReqwestHttpExecutor::for_data_dir(&root_path)),
    )
    .await
    .unwrap()
    .unwrap();
    let generated = service.build_config(false).await.unwrap();
    service
        .validator(&service.client_config().unwrap())
        .validate_value(&generated)
        .await
        .unwrap();
    let manager = SingBoxProcessManager::new(&binary, &root_path).unwrap();
    manager.start(&generated).await.unwrap();
    wait_clash_api_ready(19090, "", std::time::Duration::from_secs(15))
        .await
        .unwrap();
    assert!(manager.is_running().await);
    let output = session.run_now(&session.list_tasks()[0].id).await.unwrap();
    assert!(output.contains("remote"), "{output}");

    manager.stop().await.unwrap();
    session.stop().await;
    assert!(!manager.is_running().await);
}

#[tokio::test]
async fn unavailable_dialects_cannot_create_or_change_resources_or_import_cache() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path(), None);
    let resource = service
        .add_remote_resource(RemoteResource {
            name: "supported".into(),
            url: "https://example.invalid/supported.js".into(),
            dialect: ScriptDialect::Surge,
            ..RemoteResource::default()
        })
        .await
        .unwrap();
    service
        .import_script_config(
            "[URL Rewrite]\n^https://sentinel.example/ https://preserved.example/ 302\n[MITM]\nhostname=sentinel.example\n",
            ScriptDialect::Surge,
        )
        .await
        .unwrap();
    let before = serde_json::to_value(service.remote_resources().unwrap()).unwrap();
    for (dialect, content) in [
        (ScriptDialect::Loon, "[MITM]\nhostname=blocked.example\n"),
        (ScriptDialect::Stash, "{}"),
    ] {
        let mut unavailable = resource.clone();
        unavailable.name = format!("new-{dialect:?}");
        unavailable.dialect = dialect;
        assert!(service.add_remote_resource(unavailable).await.is_err());
        let mut changed = resource.clone();
        changed.dialect = dialect;
        changed.description = Some("must not persist".into());
        assert!(
            service
                .update_remote_resource("supported", changed)
                .await
                .is_err()
        );
        assert!(
            service
                .import_script_config(content, dialect)
                .await
                .is_err()
        );
    }
    let reopened = super::BusinessService::new(root.path(), None, Bindings::new(17899, 19090));
    assert_eq!(
        serde_json::to_value(reopened.remote_resources().unwrap()).unwrap(),
        before
    );
    let cached = RemoteManager::new(root.path().to_path_buf())
        .load_cached()
        .unwrap();
    assert_eq!(cached.hostnames, ["sentinel.example"]);
    assert_eq!(cached.rewrites.len(), 1);
    assert_eq!(
        cached.rewrites[0].pattern.as_str(),
        "^https://sentinel.example/"
    );
}

#[tokio::test]
async fn existing_unavailable_dialect_is_preserved_until_explicit_supported_selection() {
    for dialect in [ScriptDialect::Loon, ScriptDialect::Stash] {
        let root = tempfile::tempdir().unwrap();
        let legacy = RemoteResourceService::new(root.path().to_path_buf())
            .add(RemoteResource {
                name: "legacy".into(),
                url: "https://example.invalid/legacy.plugin".into(),
                dialect,
                description: Some("legacy metadata".into()),
                ..RemoteResource::default()
            })
            .await
            .unwrap();
        let service = service(root.path(), None);
        let mut edited = legacy.clone();
        edited.description = Some("edited without conversion".into());
        edited.enabled = false;
        service
            .update_remote_resource("legacy", edited.clone())
            .await
            .unwrap();
        let stored = service.remote_resources().unwrap().remove(0);
        assert_eq!(stored.dialect, dialect);
        assert_eq!(stored.description, edited.description);
        assert!(!stored.enabled);
        let mut unavailable_switch = edited.clone();
        unavailable_switch.dialect = if dialect == ScriptDialect::Loon {
            ScriptDialect::Stash
        } else {
            ScriptDialect::Loon
        };
        assert!(
            service
                .update_remote_resource("legacy", unavailable_switch)
                .await
                .is_err()
        );
        assert_eq!(
            serde_json::to_value(service.remote_resources().unwrap()).unwrap(),
            serde_json::json!([stored])
        );
        edited.dialect = ScriptDialect::QuantumultX;
        service
            .update_remote_resource("legacy", edited)
            .await
            .unwrap();
        let reopened = super::BusinessService::new(root.path(), None, Bindings::new(17899, 19090));
        let selected = reopened.remote_resources().unwrap().remove(0);
        assert_eq!(selected.dialect, ScriptDialect::QuantumultX);
        assert_eq!(selected.url, legacy.url);
        assert_eq!(
            selected.description.as_deref(),
            Some("edited without conversion")
        );
        assert!(!selected.enabled);
    }
}

#[tokio::test]
async fn qx_import_remains_available_and_activates_its_rewrite() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path(), None);
    service
        .import_script_config(
            "hostname=qx.example\n^https://qx\\.example/.*$ url 302 https://destination.example/\n",
            ScriptDialect::QuantumultX,
        )
        .await
        .unwrap();
    let cached = RemoteManager::new(root.path().to_path_buf())
        .load_cached()
        .unwrap();
    let mut uri = "https://qx.example/path".to_string();
    let mut headers = Vec::new();
    let mut body = None;
    let action = lanpath_mitm::RewriteEngine {
        rules: cached.rewrites,
    }
    .apply_request(&mut uri, &mut headers, &mut body)
    .unwrap();
    assert!(matches!(
        action,
        lanpath_mitm::RewriteAction::Redirect { status: 302, location }
            if location == "https://destination.example/"
    ));
    assert_eq!(cached.hostnames, ["qx.example"]);
}
