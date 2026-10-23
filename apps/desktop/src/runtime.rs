mod diagnostics;
mod lifecycle;

use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use lanpath_client::{
    mitm::{MitmCaView, MitmService, MitmSession},
    remote::{RemoteTaskView, ScriptTaskSession, TracingNotifier},
};
use lanpath_engine::{CoreManager, SingBoxProcessManager};
use tokio::runtime::{Builder, Runtime};

use crate::{
    business::BusinessService,
    configuration::Bindings,
    data_root::{DataRoot, default_data_dir},
    log_stream::LogService,
    observation::ObservationService,
    startup::StartupProgress,
};

const MIXED_PORT: u16 = 17899;
const CLASH_API_PORT: u16 = 19090;
const CORE_BINARY_ENV: &str = "LANPATH_DESKTOP_CORE_BINARY";
const DATA_DIR_ENV: &str = "LANPATH_DESKTOP_DATA_DIR";

#[derive(Default)]
struct RunResources {
    missing: Vec<String>,
    files: Vec<(String, String)>,
}

pub(crate) struct DesktopRuntime {
    // Keep the executor alive for lifecycle/output-reader teardown, and retain
    // the directory lease until every service and the executor have dropped.
    manager: tokio::sync::Mutex<Option<Arc<SingBoxProcessManager>>>,
    binary_override: Option<PathBuf>,
    startup: tokio::sync::watch::Sender<StartupProgress>,
    missing_rule_sets: Mutex<RunResources>,
    observations: Option<ObservationService>,
    business: Option<BusinessService>,
    logs: LogService,
    mitm: tokio::sync::Mutex<Option<MitmSession>>,
    mitm_service: Option<MitmService>,
    script_tasks: tokio::sync::Mutex<Option<ScriptTaskSession>>,
    runtime: Option<Runtime>,
    configuration_error: Option<String>,
    _data_root: Option<DataRoot>,
}

impl DesktopRuntime {
    pub(crate) fn from_env() -> Self {
        let mut errors = Vec::new();
        let root = match std::env::var_os(DATA_DIR_ENV) {
            Some(path) if !path.is_empty() => Ok(PathBuf::from(path)),
            Some(_) => Err(lanpath_common::LanpathError::Config(format!(
                "{DATA_DIR_ENV} 不能为空"
            ))),
            None => default_data_dir(),
        }
        .and_then(|path| DataRoot::open(&path));
        let data_root = match root {
            Ok(root) => Some(root),
            Err(error) => {
                errors.push(error.to_string());
                None
            }
        };
        let binary = match std::env::var_os(CORE_BINARY_ENV) {
            Some(path) if !path.is_empty() => match std::fs::canonicalize(path) {
                Ok(path) if path.is_file() => Some(path),
                Ok(_) => {
                    errors.push(format!("{CORE_BINARY_ENV} 必须指向核心文件"));
                    None
                }
                Err(error) => {
                    errors.push(format!("无法访问核心文件：{error}"));
                    None
                }
            },
            Some(_) => {
                errors.push(format!("{CORE_BINARY_ENV} 不能为空"));
                None
            }
            None => None,
        };
        let runtime = match Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
        {
            Ok(runtime) => Some(runtime),
            Err(error) => {
                errors.push(format!("无法创建 Tokio runtime：{error}"));
                None
            }
        };
        // A rejected directory never reaches a store, including offline stats.
        let observations = data_root
            .as_ref()
            .map(|root| ObservationService::new(root.path().to_path_buf()));
        let bindings = Bindings::new(MIXED_PORT, CLASH_API_PORT);
        let business = data_root
            .as_ref()
            .map(|root| BusinessService::new(root.path(), binary.clone(), bindings));
        let mitm_service = data_root.as_ref().map(|root| MitmService::new(root.path()));
        let logs = LogService::new(data_root.as_ref().map(|root| root.path()));
        Self {
            manager: tokio::sync::Mutex::new(None),
            binary_override: binary,
            startup: tokio::sync::watch::channel(StartupProgress::default()).0,
            missing_rule_sets: Mutex::new(RunResources::default()),
            observations,
            business,
            logs,
            script_tasks: tokio::sync::Mutex::new(None),
            mitm: tokio::sync::Mutex::new(None),
            mitm_service,
            runtime,
            configuration_error: (!errors.is_empty()).then(|| errors.join("；")),
            _data_root: data_root,
        }
    }

    pub(crate) fn is_configured(&self) -> bool {
        self.configuration_error.is_none() && self.runtime.is_some() && self.business.is_some()
    }

    pub(crate) fn configuration_error(&self) -> Option<&str> {
        self.configuration_error.as_deref()
    }

    pub(crate) fn core_override(&self) -> Option<&Path> {
        self.binary_override.as_deref()
    }

    pub(crate) fn startup(&self) -> tokio::sync::watch::Receiver<StartupProgress> {
        self.startup.subscribe()
    }

    pub(crate) fn running(&self) -> Result<bool, String> {
        self.block_on(async {
            match self.manager.lock().await.as_ref() {
                Some(manager) => manager.is_running().await,
                None => false,
            }
        })
    }

    pub(crate) fn missing_rule_sets(&self) -> Result<Vec<String>, String> {
        let resources = self
            .missing_rule_sets
            .lock()
            .map_err(|_| "运行资源状态锁不可用".to_string())?;
        let mut missing = resources.missing.clone();
        let root = self.business()?.data_dir();
        for (tag, path) in &resources.files {
            let available = crate::configuration::resources::confined(root, path, "rule_set.path")
                .is_ok_and(|path| path.is_file());
            if !available && !missing.contains(tag) {
                missing.push(tag.clone());
            }
        }
        Ok(missing)
    }
    pub(crate) fn port(&self) -> u16 {
        MIXED_PORT
    }
    pub(crate) fn api_port(&self) -> u16 {
        CLASH_API_PORT
    }

    pub(crate) fn business(&self) -> Result<&BusinessService, String> {
        self.business
            .as_ref()
            .ok_or_else(|| self.unavailable_data_error())
    }

    pub(crate) fn observations(&self) -> Result<&ObservationService, String> {
        self.observations
            .as_ref()
            .ok_or_else(|| self.unavailable_data_error())
    }

    pub(crate) fn logs(&self) -> &LogService {
        &self.logs
    }

    pub(crate) fn script_tasks(&self) -> Result<Vec<RemoteTaskView>, String> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| self.unavailable_data_error())?;
        runtime.block_on(async {
            Ok(self
                .script_tasks
                .lock()
                .await
                .as_ref()
                .map(ScriptTaskSession::list_tasks)
                .unwrap_or_default())
        })
    }

    pub(crate) fn run_script_task(&self, name: &str) -> Result<String, String> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| self.unavailable_data_error())?;
        runtime.block_on(async {
            let session = self.script_tasks.lock().await;
            let session = session
                .as_ref()
                .ok_or_else(|| "任务调度器未就绪；请先更新资源并启动核心".to_string())?;
            session
                .run_now(name)
                .await
                .map_err(|error| error.to_string())
        })
    }

    pub(crate) fn mitm_ca(&self) -> Result<MitmCaView, String> {
        self.mitm_service()?.ca().map_err(|error| error.to_string())
    }

    pub(crate) fn mitm_trust_status(&self) -> Result<lanpath_client::mitm::CaTrustView, String> {
        self.mitm_service()?
            .trust_status()
            .map_err(|error| error.to_string())
    }

    pub(crate) fn install_mitm_ca(&self) -> Result<lanpath_client::mitm::CaTrustView, String> {
        self.mitm_service()?
            .install_ca()
            .map_err(|error| error.to_string())
    }

    pub(crate) fn mitm_traffic(&self) -> Result<Vec<lanpath_client::mitm::TrafficRecord>, String> {
        Ok(self.mitm_service()?.traffic())
    }

    pub(crate) fn mitm_addr(&self) -> Result<Option<std::net::SocketAddr>, String> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| self.unavailable_data_error())?;
        Ok(runtime.block_on(async { self.mitm.lock().await.as_ref().map(MitmSession::addr) }))
    }

    pub(crate) fn block_on<F: Future>(&self, future: F) -> Result<F::Output, String> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| "Tokio runtime 不可用".to_string())?;
        Ok(runtime.block_on(future))
    }

    async fn start_script_tasks(&self) -> Result<(), String> {
        self.stop_script_tasks().await;
        let business = self.business()?;
        let session = ScriptTaskSession::start(
            business.data_dir().to_path_buf(),
            Arc::new(TracingNotifier::new()),
        )
        .await
        .map_err(|error| error.to_string())?;
        *self.script_tasks.lock().await = session;
        Ok(())
    }

    async fn stop_script_tasks(&self) {
        if let Some(session) = self.script_tasks.lock().await.take() {
            session.stop().await;
        }
    }

    async fn stop_mitm(&self) {
        if let Some(session) = self.mitm.lock().await.take() {
            session.stop();
        }
    }

    fn mitm_service(&self) -> Result<&MitmService, String> {
        self.mitm_service
            .as_ref()
            .ok_or_else(|| self.unavailable_data_error())
    }

    fn unavailable_data_error(&self) -> String {
        self.configuration_error
            .clone()
            .unwrap_or_else(|| "实验数据目录不可用".into())
    }

    fn executor(&self) -> Result<&Runtime, String> {
        if let Some(error) = &self.configuration_error {
            return Err(error.clone());
        }
        self.runtime
            .as_ref()
            .ok_or_else(|| "Tokio runtime 不可用".into())
    }
}
