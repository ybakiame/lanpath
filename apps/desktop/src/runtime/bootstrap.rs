use std::{
    path::PathBuf,
    sync::{Arc, Mutex, atomic::AtomicU16},
};

use lanpath_client::{http_exec::ReqwestHttpExecutor, mitm::MitmService};
use tokio::runtime::Builder;

use super::{
    CLASH_API_PORT, CORE_BINARY_ENV, DATA_DIR_ENV, DesktopRuntime, MIXED_PORT, RunResources,
};
use crate::{
    business::BusinessService,
    configuration::Bindings,
    data_root::{DataLocation, DataRoot},
    distribution::{instance::InstanceServer, migration::MigrationService},
    log_stream::LogService,
    observation::ObservationService,
    startup::StartupProgress,
};

impl DesktopRuntime {
    pub(super) fn initialize() -> Self {
        let mut errors = Vec::new();
        let location = (|| {
            let executable = std::env::current_exe()?;
            let explicit = std::env::var_os(DATA_DIR_ENV).map(PathBuf::from);
            DataLocation::resolve(&executable, explicit.as_deref())
        })();
        let location = match location {
            Ok(location) => Some(location),
            Err(error) => {
                errors.push(error.to_string());
                None
            }
        };
        let opened = location
            .as_ref()
            .map(|location| {
                let root = DataRoot::open(&location.path)?;
                // Both recovery paths run under the exclusive lease, before any store exists.
                MigrationService::new(&root).startup()?;
                lanpath_client::backup::recover_interrupted(root.path())?;
                Ok::<_, lanpath_common::LanpathError>(Arc::new(root))
            })
            .transpose();
        let data_root = match opened {
            Ok(root) => root,
            Err(error) => {
                errors.push(error.to_string());
                None
            }
        };
        let instance = data_root
            .as_ref()
            .map(|root| InstanceServer::bind(root.clone()))
            .transpose();
        let instance = match instance {
            Ok(instance) => instance,
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
                    errors.push(format!("无法访问指定核心：{error}"));
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
        let business = data_root.as_ref().map(|root| {
            BusinessService::new(
                root.path(),
                binary.clone(),
                Bindings::new(MIXED_PORT, CLASH_API_PORT),
            )
        });
        let script_http = data_root
            .as_ref()
            .map(|root| Arc::new(ReqwestHttpExecutor::for_data_dir(root.path())));
        let mitm_service = data_root
            .as_ref()
            .zip(script_http.as_ref())
            .map(|(root, http)| MitmService::new(root.path(), http.clone()));
        let observations = data_root
            .as_ref()
            .map(|root| ObservationService::new(root.path().to_owned()));
        let backup = data_root.as_ref().map(|root| {
            Arc::new(lanpath_client::backup::BackupService::new(
                root.path().to_owned(),
            ))
        });
        let logs = LogService::new(data_root.as_ref().map(|root| root.path()));
        #[cfg(windows)]
        let system_proxy =
            match lanpath_client::sysproxy::SystemProxySession::windows(crate::product::APP_ID) {
                Ok(session) => Some(session),
                Err(error) => {
                    errors.push(format!("系统代理所有权初始化失败：{error}"));
                    None
                }
            };
        Self {
            manager: tokio::sync::Mutex::new(None),
            broker: tokio::sync::Mutex::new(None),
            lifecycle_gate: Arc::new(tokio::sync::Mutex::new(())),
            live_inputs: tokio::sync::Mutex::new(None),
            binary_override: binary,
            selected_core: Mutex::new(None),
            startup: tokio::sync::watch::channel(StartupProgress::default()).0,
            missing_rule_sets: Mutex::new(RunResources::default()),
            mixed_port: AtomicU16::new(0),
            observations,
            business,
            backup,
            logs,
            mitm_service,
            script_http,
            script_tasks: tokio::sync::Mutex::new(None),
            mitm: tokio::sync::Mutex::new(None),
            runtime,
            configuration_error: (!errors.is_empty()).then(|| errors.join("；")),
            data_mode: location.map(|location| location.mode),
            instance: Mutex::new(instance),
            #[cfg(windows)]
            system_proxy,
            #[cfg(windows)]
            proxy_guardian: Mutex::new(None),
            _data_root: data_root,
        }
    }
}
