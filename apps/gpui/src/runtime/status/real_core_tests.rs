use crate::{
    log_stream::LogService,
    runtime::{ActiveCoreManager, DesktopRuntime, RunResources},
    startup::StartupProgress,
};
use lanpath_client::{ClientConfig, mitm::MitmService, remote::TracingNotifier};
use lanpath_common::CoreType;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, atomic::AtomicU16},
};

struct IsolatedSession(DesktopRuntime);

impl IsolatedSession {
    fn new(binary: PathBuf) -> Self {
        // Never call initialize(): no product data root, global proxy backend,
        // ownership journal, guardian or privileged helper is opened by this test.
        Self(DesktopRuntime {
            manager: tokio::sync::Mutex::new(None),
            broker: tokio::sync::Mutex::new(None),
            lifecycle_gate: Arc::new(tokio::sync::Mutex::new(())),
            live_inputs: tokio::sync::Mutex::new(None),
            script_http: None,
            mixed_port: AtomicU16::new(0),
            data_mode: None,
            instance: Mutex::new(None),
            #[cfg(windows)]
            system_proxy: None,
            #[cfg(windows)]
            proxy_guardian: Mutex::new(None),
            binary_override: Some(binary),
            selected_core: Mutex::new(None),
            startup: tokio::sync::watch::channel(StartupProgress::default()).0,
            missing_rule_sets: Mutex::new(RunResources::default()),
            observations: None,
            business: None,
            backup: None,
            logs: LogService::new(None),
            mitm: tokio::sync::Mutex::new(None),
            mitm_service: None,
            script_tasks: tokio::sync::Mutex::new(None),
            runtime: Some(
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .unwrap(),
            ),
            configuration_error: None,
            _data_root: None,
        })
    }
}

impl Drop for IsolatedSession {
    fn drop(&mut self) {
        self.0.executor().unwrap().block_on(async {
            if let Some(manager) = self.0.manager.lock().await.take() {
                let _ = manager.stop_process().await;
            }
            let _ = self.0.stop_mitm().await;
        });
    }
}

#[test]
#[ignore = "requires LANPATH_TEST_SING_BOX; real core and private MITM only, no system changes"]
fn running_mitm_is_healthy_but_stopped_retained_resources_require_recovery() {
    let binary = PathBuf::from(
        std::env::var_os("LANPATH_TEST_SING_BOX").expect("set LANPATH_TEST_SING_BOX"),
    );
    let root = tempfile::tempdir().unwrap();
    let owned = IsolatedSession::new(binary.clone());
    let runtime = &owned.0;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let manager =
        Arc::new(ActiveCoreManager::new(CoreType::SingBox, &binary, root.path()).unwrap());
    runtime.executor().unwrap().block_on(async {
        *runtime.manager.lock().await = Some(manager.clone());
        manager
            .start(&serde_json::json!({
                "log": {"level": "error"},
                "inbounds": [{"type":"mixed","tag":"in","listen":"127.0.0.1","listen_port":port}],
                "outbounds": [{"type":"direct","tag":"direct"}],
                "route": {"final":"direct"}
            }))
            .await
            .unwrap();
        let service = MitmService::new(
            root.path(),
            Arc::new(lanpath_script::MockHttpExecutor::with_responses(vec![])),
        );
        let mut config = ClientConfig::new(root.path().to_path_buf(), "", "", binary.clone());
        config.mixed_port = port;
        config.mitm_enabled = true;
        config.mitm.hostnames = vec!["status.fixture.invalid".into()];
        *runtime.mitm.lock().await = service
            .start(&config, Arc::new(TracingNotifier::new()))
            .await
            .unwrap();
        assert!(runtime.mitm.lock().await.is_some());
    });
    let status = runtime.poll_lifecycle(false).unwrap();
    assert!(status.running);
    assert!(!status.recovery_required);
    assert!(!runtime.network_recovery_required().unwrap());
    runtime
        .executor()
        .unwrap()
        .block_on(manager.stop_process())
        .unwrap();
    let stopped = runtime.poll_lifecycle(false).unwrap();
    assert!(!stopped.running);
    assert!(stopped.recovery_required);
    assert!(runtime.network_recovery_required().unwrap());
    runtime.executor().unwrap().block_on(async {
        *runtime.manager.lock().await = None;
        runtime.stop_mitm().await.unwrap();
    });
    assert!(!runtime.network_recovery_required().unwrap());
}
