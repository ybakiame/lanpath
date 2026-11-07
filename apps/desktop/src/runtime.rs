mod backup;
mod bootstrap;
mod diagnostics;
mod lifecycle;
mod native;
mod remote;
mod status;
mod system_control;
mod tun;

use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU16, Ordering},
    },
};

use lanpath_client::{
    mitm::{MitmCaView, MitmService, MitmSession},
    remote::{RemoteTaskView, ScriptTaskSession},
};
use lanpath_common::{CoreType, LanpathResult, ProcessStopKind};
use lanpath_engine::{CoreManager, MihomoProcessManager, SingBoxProcessManager};
use tokio::runtime::Runtime;

use crate::{
    business::BusinessService,
    data_root::{DataMode, DataRoot},
    distribution::instance::InstanceServer,
    log_stream::LogService,
    observation::ObservationService,
    privileged::Broker,
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

/// Concrete process manager selected by the saved core type. It retains
/// `stop_process` receipts that the object-safe engine trait does not expose.
pub(crate) enum ActiveCoreManager {
    SingBox(SingBoxProcessManager),
    Mihomo(MihomoProcessManager),
}

impl ActiveCoreManager {
    pub(crate) fn new(
        core_type: CoreType,
        binary: &Path,
        config_dir: &Path,
    ) -> LanpathResult<Self> {
        match core_type {
            CoreType::SingBox => Ok(Self::SingBox(SingBoxProcessManager::new(
                binary, config_dir,
            )?)),
            CoreType::Mihomo => Ok(Self::Mihomo(MihomoProcessManager::new(binary, config_dir)?)),
        }
    }

    pub(crate) async fn start(&self, config: &serde_json::Value) -> LanpathResult<()> {
        match self {
            Self::SingBox(manager) => manager.start(config).await,
            Self::Mihomo(manager) => manager.start(config).await,
        }
    }

    pub(crate) async fn is_running(&self) -> bool {
        match self {
            Self::SingBox(manager) => manager.is_running().await,
            Self::Mihomo(manager) => manager.is_running().await,
        }
    }

    pub(crate) async fn stop_process(&self) -> LanpathResult<ProcessStopKind> {
        match self {
            Self::SingBox(manager) => manager.stop_process().await,
            Self::Mihomo(manager) => manager.stop_process().await,
        }
    }

    pub(crate) fn core_type(&self) -> CoreType {
        match self {
            Self::SingBox(_) => CoreType::SingBox,
            Self::Mihomo(_) => CoreType::Mihomo,
        }
    }
}

pub(crate) struct DesktopRuntime {
    // Keep the executor alive for lifecycle/output-reader teardown, and retain
    // the directory lease until every service and the executor have dropped.
    manager: tokio::sync::Mutex<Option<Arc<ActiveCoreManager>>>,
    broker: tokio::sync::Mutex<Option<Arc<Mutex<Broker>>>>,
    lifecycle_gate: Arc<tokio::sync::Mutex<()>>,
    live_inputs: tokio::sync::Mutex<Option<remote::LiveInputs>>,
    script_http: Option<Arc<lanpath_client::http_exec::ReqwestHttpExecutor>>,
    mixed_port: AtomicU16,
    data_mode: Option<DataMode>,
    instance: Mutex<Option<InstanceServer>>,
    #[cfg(windows)]
    system_proxy: Option<lanpath_client::sysproxy::SystemProxySession>,
    #[cfg(windows)]
    proxy_guardian: Mutex<Option<crate::proxy_guardian::Guardian>>,
    binary_override: Option<PathBuf>,
    selected_core: Mutex<Option<(CoreType, String)>>,
    startup: tokio::sync::watch::Sender<StartupProgress>,
    missing_rule_sets: Mutex<RunResources>,
    observations: Option<ObservationService>,
    business: Option<BusinessService>,
    backup: Option<Arc<lanpath_client::backup::BackupService>>,
    logs: LogService,
    mitm: tokio::sync::Mutex<Option<MitmSession>>,
    mitm_service: Option<MitmService>,
    script_tasks: tokio::sync::Mutex<Option<ScriptTaskSession>>,
    runtime: Option<Runtime>,
    configuration_error: Option<String>,
    _data_root: Option<Arc<DataRoot>>,
}

impl DesktopRuntime {
    pub(crate) fn from_env() -> Self {
        Self::initialize()
    }

    pub(crate) fn settings_migration_required(&self) -> Result<bool, String> {
        crate::migration::is_required(self.data_root()?.path()).map_err(|error| error.to_string())
    }

    pub(crate) fn load_settings(&mut self) -> Result<(), String> {
        let result = self
            .business
            .as_ref()
            .map(BusinessService::client_config)
            .transpose();
        match result {
            Ok(Some(config)) => {
                self.saved_proxy_port_changed(config.mixed_port);
                self.refresh_selected_core();
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(error) => {
                let error = error.to_string();
                self.configuration_error = Some(match self.configuration_error.take() {
                    Some(previous) => format!("{previous}；{error}"),
                    None => error.clone(),
                });
                Err(error)
            }
        }
    }

    pub(crate) fn migrate_settings(&self) -> Result<(), String> {
        let root = self.data_root()?.path().to_owned();
        crate::migration::run(&root)
            .map(|_| ())
            .map_err(|error| format!("设置迁移失败：{error}"))
    }

    pub(crate) fn refresh_selected_core(&self) {
        let selection = self.business.as_ref().and_then(|business| {
            business.client_config().ok().map(|config| {
                let name = config
                    .core_binary
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| config.core_type.to_string());
                (config.core_type, name)
            })
        });
        if let Ok(mut selected) = self.selected_core.lock() {
            *selected = selection;
        }
    }

    pub(crate) fn selected_core_type(&self) -> CoreType {
        self.selected_core
            .lock()
            .ok()
            .and_then(|selected| selected.as_ref().map(|(kind, _)| *kind))
            .unwrap_or_default()
    }

    pub(crate) fn selected_core_name(&self) -> String {
        self.selected_core
            .lock()
            .ok()
            .and_then(|selected| selected.as_ref().map(|(_, name)| name.clone()))
            .unwrap_or_else(|| "sing-box".into())
    }

    pub(crate) fn backup_restore_pending(&self) -> bool {
        self.backup
            .as_ref()
            .is_some_and(|backup| backup.is_pending())
    }

    pub(crate) fn validate_backup_restore(&self) -> Result<(), String> {
        lanpath_client::backup::validate_pending(self.data_root()?.path())
            .map_err(|error| format!("备份校验失败：{error}"))
    }

    pub(crate) fn apply_backup_restore(&self) -> Result<(), String> {
        lanpath_client::backup::apply_validated_pending(self.data_root()?.path())
            .map_err(|error| format!("备份恢复失败：{error}"))
    }

    pub(crate) fn is_configured(&self) -> bool {
        self.configuration_error.is_none() && self.runtime.is_some() && self.business.is_some()
    }

    pub(crate) fn core_override(&self) -> Option<&Path> {
        self.binary_override.as_deref()
    }

    pub(crate) fn startup(&self) -> tokio::sync::watch::Receiver<StartupProgress> {
        self.startup.subscribe()
    }

    pub(crate) fn running(&self) -> Result<bool, String> {
        self.executor()?.block_on(self.running_async())
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
        self.mixed_port.load(Ordering::Acquire)
    }

    pub(crate) fn business(&self) -> Result<&BusinessService, String> {
        self.business
            .as_ref()
            .ok_or_else(|| self.unavailable_data_error())
    }

    pub(crate) fn backup(&self) -> Result<&Arc<lanpath_client::backup::BackupService>, String> {
        self.backup
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

    pub(crate) fn run_script_task(&self, id: &str) -> Result<String, String> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| self.unavailable_data_error())?;
        runtime.block_on(async {
            let session = self.script_tasks.lock().await;
            let session = session
                .as_ref()
                .ok_or_else(|| "任务调度器未就绪；请先更新资源并启动核心".to_string())?;
            session.run_now(id).await.map_err(|error| error.to_string())
        })
    }

    pub(crate) fn mitm_ca(&self) -> Result<MitmCaView, String> {
        self.mitm_service()?.ca().map_err(|error| error.to_string())
    }

    pub(crate) fn mitm_trust_status(
        &self,
    ) -> Result<lanpath_client::mitm::CaOwnedTrustView, String> {
        self.mitm_service()?
            .trust_status()
            .map_err(|error| error.to_string())
    }

    pub(crate) fn install_mitm_ca(
        &self,
        expected_sha256: &str,
    ) -> Result<lanpath_client::mitm::CaOwnedTrustView, String> {
        self.mitm_service()?
            .install_ca(expected_sha256)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn remove_mitm_ca(
        &self,
        expected_sha256: &str,
    ) -> Result<lanpath_client::mitm::CaRemovalReceipt, String> {
        self.mitm_service()?
            .remove_ca(expected_sha256)
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
            self.notifier()?,
            self.script_http
                .as_ref()
                .ok_or_else(|| self.unavailable_data_error())?
                .clone(),
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

    async fn stop_mitm(&self) -> Result<(), String> {
        let mut session = self.mitm.lock().await;
        if let Some(session) = session.as_mut() {
            session.stop().await.map_err(|error| error.to_string())?;
        }
        *session = None;
        Ok(())
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
