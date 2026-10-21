use std::{future::Future, net::Ipv4Addr, path::PathBuf, sync::Arc, time::Duration};

use lanpath_client::{
    core_config::{push_clash_mode, wait_clash_api_ready},
    mitm::{MitmCaView, MitmService, MitmSession},
    remote::{RemoteTaskView, ScriptTaskSession, TracingNotifier},
    replay_group_selections,
};
use lanpath_engine::{CoreManager, SingBoxProcessManager};
use tokio::runtime::{Builder, Runtime};

use crate::{
    business::BusinessService,
    configuration::Bindings,
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
        let bindings = Bindings::new(MIXED_PORT, CLASH_API_PORT);
        let business = data_root
            .as_ref()
            .map(|root| BusinessService::new(root.path(), binary.clone(), bindings));
        let mitm_service = data_root.as_ref().map(|root| MitmService::new(root.path()));
        let logs = LogService::new(data_root.as_ref().map(|root| root.path()));
        Self {
            manager,
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
        self.configuration_error.is_none()
            && self.runtime.is_some()
            && self.manager.is_some()
            && self.business.is_some()
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

    /// Blocking lifecycle entry points are called only by GPUI's background executor.
    pub(crate) fn start(&self) -> Result<(), String> {
        let (runtime, manager) = self.parts()?;
        runtime.block_on(async {
            let business = self.business()?;
            let client_config = business
                .client_config()
                .map_err(|error| error.to_string())?;
            let observations = self.observations()?;
            observations.prepare().await?;
            manager.reap_stale();
            let should_reserve_return_port = client_config.mitm_enabled
                && self
                    .mitm_service()?
                    .whitelist(&client_config)
                    .has_inclusions();
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, MIXED_PORT))
                .await
                .map_err(|error| format!("实验端口 {MIXED_PORT} 不可用：{error}"))?;
            let api_listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, CLASH_API_PORT))
                .await
                .map_err(|error| format!("Clash API 端口 {CLASH_API_PORT} 不可用：{error}"))?;
            let return_listener = if should_reserve_return_port {
                let port = MIXED_PORT + 1;
                Some(
                    tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))
                        .await
                        .map_err(|error| format!("MITM 回流端口 {port} 不可用：{error}"))?,
                )
            } else {
                None
            };
            drop(return_listener);
            drop(api_listener);
            drop(listener);
            self.stop_mitm().await;
            let mitm = self
                .mitm_service()?
                .start(&client_config, Arc::new(TracingNotifier::new()))
                .await
                .map_err(|error| error.to_string())?;
            let chain = mitm.as_ref().map(MitmSession::chain);
            let config = business
                .build_config_with_chain(true, chain)
                .await
                .map_err(|error| error.to_string())?;
            business
                .validator()
                .validate_value(&config)
                .await
                .map_err(|error| error.to_string())?;
            if let Err(error) = self.start_script_tasks().await {
                tracing::warn!(%error, "定时任务调度器启动失败（不影响核心启动）");
            }
            if let Err(error) = manager.start(&config).await {
                self.stop_script_tasks().await;
                return Err(error.to_string());
            }
            let result = async {
                wait_clash_api_ready(CLASH_API_PORT, "", Duration::from_secs(15))
                    .await
                    .map_err(|error| error.to_string())?;
                let mode = business.rule_mode().map_err(|error| error.to_string())?;
                push_clash_mode(CLASH_API_PORT, "", &mode)
                    .await
                    .map_err(|error| error.to_string())?;
                // Stale persisted members must not prevent startup; valid groups are
                // restored independently and the live snapshot remains authoritative.
                replay_group_selections(CLASH_API_PORT, "", business.data_dir()).await;
                observations.start(CLASH_API_PORT).await?;
                self.logs.start(CLASH_API_PORT).await
            }
            .await;
            if let Err(error) = result {
                self.logs.stop().await;
                observations.stop().await;
                self.stop_script_tasks().await;
                return match manager.stop().await {
                    Ok(()) => Err(error),
                    Err(stop_error) => Err(format!("{error}；核心回收失败：{stop_error}")),
                };
            }
            *self.mitm.lock().await = mitm;
            Ok(())
        })
    }

    pub(crate) fn stop(&self) -> Result<(), String> {
        let (runtime, manager) = self.parts()?;
        runtime.block_on(async {
            self.logs.stop().await;
            self.observations()?.stop().await;
            self.stop_script_tasks().await;
            self.stop_mitm().await;
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
