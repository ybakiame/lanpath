use std::{future::Future, net::Ipv4Addr, path::PathBuf, sync::Arc};

use lanpath_engine::{CoreManager, SingBoxProcessManager};
use serde_json::{Value, json};
use tokio::runtime::{Builder, Runtime};

use crate::observation::ObservationService;

const DEFAULT_CORE_PORT: u16 = 17899;
const CLASH_API_PORT: u16 = 19090;
const PLACEHOLDER_SOCKS_PORT: u16 = 19091;
const CORE_BINARY_ENV: &str = "LANPATH_GPUI_CORE_BINARY";
const CORE_DATA_DIR_ENV: &str = "LANPATH_GPUI_CORE_DATA_DIR";
const CORE_PORT_ENV: &str = "LANPATH_GPUI_CORE_PORT";

pub(crate) struct CoreValidationService {
    // Drop the manager before the runtime so the lifecycle guard and output
    // readers retain a live runtime during teardown.
    manager: Option<Arc<SingBoxProcessManager>>,
    observations: ObservationService,
    runtime: Option<Runtime>,
    config: Value,
    data_dir: PathBuf,
    port: u16,
    api_port: u16,
    configuration_error: Option<String>,
}

impl CoreValidationService {
    pub(crate) fn from_env() -> Self {
        let mut errors = Vec::new();
        let data_dir = std::env::var_os(CORE_DATA_DIR_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("lanpath-gpui-core-validation"));
        let port = match std::env::var(CORE_PORT_ENV) {
            Ok(raw) => match raw.parse::<u16>() {
                Ok(port) if port > 0 => port,
                _ => {
                    errors.push(format!("{CORE_PORT_ENV} 必须是 1-65535 的端口"));
                    DEFAULT_CORE_PORT
                }
            },
            Err(_) => DEFAULT_CORE_PORT,
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
        let manager = match std::env::var_os(CORE_BINARY_ENV) {
            Some(binary) if !binary.is_empty() => {
                match SingBoxProcessManager::new(binary, &data_dir) {
                    Ok(manager) => Some(Arc::new(manager)),
                    Err(error) => {
                        errors.push(format!("无法创建核心管理器：{error}"));
                        None
                    }
                }
            }
            _ => {
                errors.push(format!("未设置 {CORE_BINARY_ENV}"));
                None
            }
        };
        let configuration_error = (!errors.is_empty()).then(|| errors.join("；"));
        let config = json!({
            "log": { "level": "warn", "timestamp": true },
            "inbounds": [{
                "type": "mixed",
                "tag": "gpui-validation",
                "listen": "127.0.0.1",
                "listen_port": port
            }],
            "outbounds": [
                {
                    "type": "selector",
                    "tag": "proxy",
                    "outbounds": ["direct", "local-socks"],
                    "default": "direct"
                },
                { "type": "direct", "tag": "direct" },
                {
                    "type": "socks",
                    "tag": "local-socks",
                    "server": "127.0.0.1",
                    "server_port": PLACEHOLDER_SOCKS_PORT
                }
            ],
            "route": { "final": "proxy" },
            "experimental": {
                "clash_api": {
                    "external_controller": format!("127.0.0.1:{CLASH_API_PORT}")
                }
            }
        });

        Self {
            manager,
            observations: ObservationService::new(data_dir.clone()),
            runtime,
            config,
            data_dir,
            port,
            api_port: CLASH_API_PORT,
            configuration_error,
        }
    }

    pub(crate) fn is_configured(&self) -> bool {
        self.configuration_error.is_none() && self.runtime.is_some() && self.manager.is_some()
    }

    pub(crate) fn configuration_error(&self) -> Option<&str> {
        self.configuration_error.as_deref()
    }

    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    pub(crate) fn api_port(&self) -> u16 {
        self.api_port
    }

    pub(crate) fn observations(&self) -> &ObservationService {
        &self.observations
    }

    /// Synchronous entry point for GPUI's background executor. Never call this
    /// on the window thread: process wait and lifecycle cleanup are blocking.
    pub(crate) fn start(&self) -> Result<(), String> {
        let (runtime, manager) = self.parts()?;
        runtime.block_on(async {
            tokio::fs::create_dir_all(&self.data_dir)
                .await
                .map_err(|error| format!("无法创建隔离数据目录：{error}"))?;
            self.observations.prepare().await?;
            manager.reap_stale();
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, self.port))
                .await
                .map_err(|error| format!("隔离端口 {} 不可用：{error}", self.port))?;
            let api_listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, self.api_port))
                .await
                .map_err(|error| format!("Clash API 端口 {} 不可用：{error}", self.api_port))?;
            drop(api_listener);
            drop(listener);
            manager
                .start(&self.config)
                .await
                .map_err(|error| error.to_string())?;
            if let Err(error) = self.observations.start(self.api_port).await {
                return match manager.stop().await {
                    Ok(()) => Err(error),
                    Err(stop_error) => Err(format!("{error}；核心回收失败：{stop_error}")),
                };
            }
            Ok(())
        })
    }

    /// See `start`: this blocks only on a GPUI background worker.
    pub(crate) fn stop(&self) -> Result<(), String> {
        let (runtime, manager) = self.parts()?;
        runtime.block_on(async {
            self.observations.stop().await;
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
