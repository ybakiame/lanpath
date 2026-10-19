use std::{
    future::Future,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::Arc,
};

use lanpath_engine::{CoreManager, SingBoxProcessManager};
use tokio::runtime::{Builder, Runtime};

use crate::{
    configuration::{Bindings, ConfigStore},
    data_root::{DataRoot, default_data_dir},
    log_stream::LogService,
    observation::ObservationService,
};

const MIXED_PORT: u16 = 17899;
const CLASH_API_PORT: u16 = 19090;
const CORE_BINARY_ENV: &str = "LANPATH_DESKTOP_CORE_BINARY";
const DATA_DIR_ENV: &str = "LANPATH_DESKTOP_DATA_DIR";

pub(crate) struct DesktopRuntime {
    // Keep the executor alive for lifecycle/output-reader teardown, and retain
    // the directory lease until every service and the executor have dropped.
    manager: Option<Arc<SingBoxProcessManager>>,
    observations: Option<ObservationService>,
    configuration: Option<ConfigStore>,
    logs: LogService,
    runtime: Option<Runtime>,
    configuration_error: Option<String>,
    data_root: Option<DataRoot>,
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
            _ => {
                errors.push(format!("未设置 {CORE_BINARY_ENV}"));
                None
            }
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
        let manager = match (data_root.as_ref(), binary.as_ref()) {
            (Some(root), Some(binary)) => match SingBoxProcessManager::new(binary, root.path()) {
                Ok(manager) => Some(Arc::new(manager)),
                Err(error) => {
                    errors.push(format!("无法创建核心管理器：{error}"));
                    None
                }
            },
            _ => None,
        };
        // A rejected directory never reaches a store, including offline stats.
        let observations = data_root
            .as_ref()
            .map(|root| ObservationService::new(root.path().to_path_buf()));
        let configuration = data_root.as_ref().map(|root| {
            ConfigStore::new(
                root.path(),
                binary,
                Bindings::new(MIXED_PORT, CLASH_API_PORT),
            )
        });
        Self {
            manager,
            observations,
            configuration,
            logs: LogService::new(),
            runtime,
            configuration_error: (!errors.is_empty()).then(|| errors.join("；")),
            data_root,
        }
    }

    pub(crate) fn is_configured(&self) -> bool {
        self.configuration_error.is_none() && self.runtime.is_some() && self.manager.is_some()
    }

    pub(crate) fn configuration_error(&self) -> Option<&str> {
        self.configuration_error.as_deref()
    }
    pub(crate) fn port(&self) -> u16 {
        MIXED_PORT
    }
    pub(crate) fn api_port(&self) -> u16 {
        CLASH_API_PORT
    }
    pub(crate) fn data_dir(&self) -> Option<&Path> {
        self.data_root.as_ref().map(DataRoot::path)
    }

    pub(crate) fn configuration(&self) -> Result<&ConfigStore, String> {
        self.configuration
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

    /// Blocking lifecycle entry points are called only by GPUI's background executor.
    pub(crate) fn start(&self) -> Result<(), String> {
        let (runtime, manager) = self.parts()?;
        runtime.block_on(async {
            let config = self
                .configuration()?
                .for_start()
                .await
                .map_err(|error| error.to_string())?;
            let observations = self.observations()?;
            observations.prepare().await?;
            manager.reap_stale();
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, MIXED_PORT))
                .await
                .map_err(|error| format!("实验端口 {MIXED_PORT} 不可用：{error}"))?;
            let api_listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, CLASH_API_PORT))
                .await
                .map_err(|error| format!("Clash API 端口 {CLASH_API_PORT} 不可用：{error}"))?;
            drop(api_listener);
            drop(listener);
            manager
                .start(&config)
                .await
                .map_err(|error| error.to_string())?;
            let result = async {
                observations.start(CLASH_API_PORT).await?;
                self.logs.start(CLASH_API_PORT).await
            }
            .await;
            if let Err(error) = result {
                self.logs.stop().await;
                observations.stop().await;
                return match manager.stop().await {
                    Ok(()) => Err(error),
                    Err(stop_error) => Err(format!("{error}；核心回收失败：{stop_error}")),
                };
            }
            Ok(())
        })
    }

    pub(crate) fn stop(&self) -> Result<(), String> {
        let (runtime, manager) = self.parts()?;
        runtime.block_on(async {
            self.logs.stop().await;
            self.observations()?.stop().await;
            manager.stop().await.map_err(|error| error.to_string())
        })
    }

    pub(crate) fn block_on<F: Future>(&self, future: F) -> Result<F::Output, String> {
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| "Tokio runtime 不可用".to_string())?;
        Ok(runtime.block_on(future))
    }

    fn unavailable_data_error(&self) -> String {
        self.configuration_error
            .clone()
            .unwrap_or_else(|| "实验数据目录不可用".into())
    }

    fn parts(&self) -> Result<(&Runtime, &Arc<SingBoxProcessManager>), String> {
        if let Some(error) = &self.configuration_error {
            return Err(error.clone());
        }
        let runtime = self
            .runtime
            .as_ref()
            .ok_or_else(|| "Tokio runtime 不可用".to_string())?;
        let manager = self
            .manager
            .as_ref()
            .ok_or_else(|| "核心管理器未配置".to_string())?;
        Ok((runtime, manager))
    }
}
