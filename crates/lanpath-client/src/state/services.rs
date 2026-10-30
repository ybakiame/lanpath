//! Service startup/shutdown methods for [`ClientState`].

use lanpath_common::LanpathResult;
use std::net::{Ipv4Addr, SocketAddr};

use crate::runner::CoreRunner;
use crate::state::ClientState;

impl ClientState {
    /// After subscription and config composition, start core → enable system proxy (pointing to core mixed main entry),
    /// rollback on failure. MITM and scheduler are already started before core in [`ClientState::start`].
    ///
    /// Rollback strategy: if core startup fails, close MITM and scheduler; if system proxy enable fails, close core, MITM and scheduler in reverse order,
    /// then propagate the error upward.
    pub(crate) async fn start_services(
        &mut self,
        config_json: &serde_json::Value,
    ) -> LanpathResult<()> {
        // Core startup log: Android core is built-in libbox (driven by Kotlin VpnPlugin), no standalone binary;
        // desktop is external core binary, print its path.
        #[cfg(target_os = "android")]
        tracing::info!("Starting core (Android built-in libbox)");
        #[cfg(not(target_os = "android"))]
        tracing::info!(binary = %self.config.core_binary.display(), "Starting core");
        let core = CoreRunner::create(&self.config.core_binary, &self.config.data_dir)?;

        // ADR-0012 D2+D4（桌面）：先收割上次实例遗留的核心进程（孤儿占用端口
        // 场景自愈），再探测端口占用；被外部进程占用时报错并指明占用者。
        // Android 核心由 VPN 服务托管生命周期，不参与收割与端口探测。
        #[cfg(not(target_os = "android"))]
        {
            core.reap_stale();
            let mut ports: Vec<(u16, &str)> = vec![(self.config.mixed_port, "主入站 mixed")];
            if self.config.clash_api_enabled {
                ports.push((self.config.clash_api_port, "Clash API"));
            }
            // MITM 链路激活时核心另绑回流入站（mixed_port + 1，state::mitm 处定义）。
            #[cfg(feature = "mitm")]
            if self.mitm.is_some() {
                ports.push((self.config.mixed_port.saturating_add(1), "MITM 回流入站"));
            }
            if let Err(e) = (self.port_probe)(&ports) {
                return Err(self.rollback_mitm_started(e).await);
            }
        }

        // Before Android startup, write the final config sent to core to disk with credentials redacted: uuid/password/server masked as
        // "***" and written to data_dir/logs/last_start_config.json (pretty JSON). This file is included in log export zip,
        // for troubleshooting to confirm the real config reaching the core. The whole process is best-effort: any
        // IO failure only logs a warning, never affects the startup flow.
        #[cfg(target_os = "android")]
        {
            let logs_dir = self.config.data_dir.join("logs");
            let result = (|| -> std::io::Result<std::path::PathBuf> {
                std::fs::create_dir_all(&logs_dir)?;
                let mut redacted = config_json.clone();
                super::compat::redact_config_credentials(&mut redacted);
                let content =
                    serde_json::to_string_pretty(&redacted).map_err(std::io::Error::other)?;
                let path = logs_dir.join("last_start_config.json");
                std::fs::write(&path, content)?;
                Ok(path)
            })();
            match result {
                Ok(path) => tracing::info!(
                    path = %path.display(),
                    "Wrote redacted final core config to disk (JSON, included in log export zip)"
                ),
                Err(e) => tracing::warn!(
                    error = %e,
                    path = %logs_dir.display(),
                    "Redacted core config write failed (does not affect startup)"
                ),
            }
        }

        if let Err(e) = core.start(config_json).await {
            return Err(self.rollback_mitm_started(e).await);
        }
        self.core = Some(core);

        // Script core-policy requests must avoid recursion via the actual MITM return inbound.
        let port = config_json
            .get("inbounds")
            .and_then(serde_json::Value::as_array)
            .and_then(|inbounds| {
                inbounds
                    .iter()
                    .find(|inbound| inbound["tag"] == "mitm-return")
            })
            .and_then(|inbound| inbound["listen_port"].as_u64())
            .and_then(|port| u16::try_from(port).ok())
            .unwrap_or(self.config.mixed_port);
        let endpoint = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port);
        if let Err(error) = self.wait_for_core_endpoint(endpoint).await {
            self.stop().await?;
            return Err(error);
        }
        self.http.set_core_port(Some(port));
        if self.config.system_proxy_enabled {
            // System proxy always points to core mixed main entry (MITM hangs after core).
            let addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), self.config.mixed_port);
            self.http.set_system_proxy_owned(true);
            if let Err(e) = self.sysproxy.enable(addr).await {
                // Apply/notification failure may already have changed global settings.
                if let Err(restore) = self.sysproxy.disable().await {
                    return Err(lanpath_common::LanpathError::Client(format!(
                        "{e}; system proxy restoration failed, core retained: {restore}"
                    )));
                }
                self.http.set_system_proxy_owned(false);
                self.stop_core().await?;
                self.http.set_core_port(None);
                return Err(self.rollback_mitm_started(e).await);
            }
        }

        Ok(())
    }

    pub(crate) async fn stop_core(&mut self) -> LanpathResult<()> {
        if let Some(core) = self.core.as_ref() {
            core.stop().await?;
            self.core = None;
        }
        Ok(())
    }

    async fn wait_for_core_endpoint(&self, endpoint: SocketAddr) -> LanpathResult<()> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if !match self.core.as_ref() {
                Some(core) => core.is_running().await,
                None => false,
            } {
                return Err(lanpath_common::LanpathError::Client(
                    "核心在就绪前退出".into(),
                ));
            }
            if tokio::time::timeout(
                std::time::Duration::from_millis(200),
                tokio::net::TcpStream::connect(endpoint),
            )
            .await
            .is_ok_and(|result| result.is_ok())
            {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(lanpath_common::LanpathError::Client(format!(
                    "核心入口 {endpoint} 未就绪"
                )));
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
}
