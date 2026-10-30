use std::sync::{Arc, Mutex};

use lanpath_client::ClientConfig;
use lanpath_common::LanpathResult;

use super::DesktopRuntime;
use crate::privileged::{Broker, BrokerStatus};

pub(super) async fn broker_request<T: Send + 'static>(
    broker: Arc<Mutex<Broker>>,
    operation: impl FnOnce(&mut Broker) -> LanpathResult<T> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(move || {
        let mut broker = broker
            .lock()
            .map_err(|_| "TUN helper 会话锁不可用".to_string())?;
        operation(&mut broker).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("TUN helper 通信任务失败：{error}"))?
}

impl DesktopRuntime {
    pub(super) async fn broker_status(&self) -> Result<Option<BrokerStatus>, String> {
        let broker = self.broker.lock().await.clone();
        match broker {
            Some(broker) => broker_request(broker, Broker::status).await.map(Some),
            None => Ok(None),
        }
    }

    pub(super) fn recover_proxy_before_start(&self) -> Result<(), String> {
        #[cfg(windows)]
        if let Some(proxy) = &self.system_proxy {
            use lanpath_client::sysproxy::SystemProxyOwnership;
            let mut guardian = self
                .proxy_guardian
                .lock()
                .map_err(|_| "系统代理守护会话锁不可用".to_string())?;
            if let Some(armed) = guardian.as_mut() {
                if armed.status().map_err(|error| error.to_string())? == SystemProxyOwnership::Owned
                {
                    return Err("系统代理会话仍被本实例持有；请先停止并恢复".into());
                }
                armed
                    .shutdown()
                    .map_err(|error| format!("遗留守护会话恢复/退出未确认：{error}"))?;
                *guardian = None;
            }
            match proxy.status().map_err(|error| error.to_string())? {
                SystemProxyOwnership::Unowned => {}
                SystemProxyOwnership::Owned => {
                    return Err("系统代理会话仍被本实例持有；请先停止并恢复".into());
                }
                SystemProxyOwnership::RecoveryRequired => {
                    // Existing explicit startup recovery authority moves to the same
                    // independent worker; GUI death cannot interrupt its checkpoints.
                    let armed = crate::proxy_guardian::Guardian::arm(proxy.owner_id())
                        .map_err(|error| format!("遗留代理恢复守护启动失败：{error}"))?;
                    let armed = guardian.insert(armed);
                    armed
                        .recover_abandoned()
                        .map_err(|error| format!("遗留系统代理恢复失败：{error}"))?;
                    armed
                        .shutdown()
                        .map_err(|error| format!("遗留代理恢复守护退出未确认：{error}"))?;
                    *guardian = None;
                }
            }
        }
        Ok(())
    }

    pub(super) fn acquire_proxy(&self, config: &ClientConfig) -> Result<(), String> {
        if !config.system_proxy_enabled {
            return Ok(());
        }
        #[cfg(windows)]
        {
            let proxy = self
                .system_proxy
                .as_ref()
                .ok_or("Windows 系统代理服务不可用")?;
            let http = self
                .script_http
                .as_ref()
                .ok_or_else(|| self.unavailable_data_error())?;
            let mut guardian = self
                .proxy_guardian
                .lock()
                .map_err(|_| "系统代理守护会话锁不可用".to_string())?;
            if guardian.is_some() {
                return Err("上次系统代理守护会话尚未结束；请先停止并恢复".into());
            }
            let armed = crate::proxy_guardian::Guardian::arm(proxy.owner_id())
                .map_err(|error| format!("系统代理守护启动失败；未接管系统代理：{error}"))?;
            let armed = guardian.insert(armed);
            // Intent precedes the RPC: timeout/lost response can mean the independent
            // guardian has already mutated WinINet. Retain core/MITM until recovery.
            http.set_system_proxy_owned(true);
            armed
                .acquire(config.mixed_port)
                .map_err(|error| format!("系统代理守护接管失败；保留恢复资源：{error}"))
        }
        #[cfg(not(windows))]
        {
            Err("GPUI 系统代理接管仅支持 Windows".into())
        }
    }

    pub(super) fn restore_proxy(&self) -> Result<(), String> {
        #[cfg(windows)]
        if let Some(proxy) = &self.system_proxy {
            let mut guardian = self
                .proxy_guardian
                .lock()
                .map_err(|_| "系统代理守护会话锁不可用".to_string())?;
            if let Some(armed) = guardian.as_mut() {
                armed.shutdown().map_err(|error| {
                    format!("系统代理守护恢复/退出未确认；保留核心及 MITM 服务：{error}")
                })?;
                *guardian = None;
            }
            // A dead guardian is not restoration proof. A replacement worker may
            // recover only our exact journal, never a successor's abandoned state.
            let matching_owner = proxy
                .has_recovery_for_owner(proxy.owner_id())
                .map_err(|error| error.to_string())?;
            if matching_owner {
                let armed = crate::proxy_guardian::Guardian::arm(proxy.owner_id())
                    .map_err(|error| format!("系统代理恢复守护启动失败：{error}"))?;
                let armed = guardian.insert(armed);
                armed
                    .recover_watched()
                    .map_err(|error| format!("系统代理守护遗留恢复失败：{error}"))?;
                armed
                    .shutdown()
                    .map_err(|error| format!("系统代理恢复守护退出未确认：{error}"))?;
                *guardian = None;
            }
        }
        if let Some(http) = &self.script_http {
            http.set_system_proxy_owned(false);
        }
        Ok(())
    }

    #[cfg(windows)]
    pub(super) fn proxy_ownership(
        &self,
    ) -> Result<lanpath_client::sysproxy::SystemProxyOwnership, String> {
        if let Some(guardian) = self
            .proxy_guardian
            .lock()
            .map_err(|_| "系统代理守护会话锁不可用".to_string())?
            .as_mut()
        {
            return guardian
                .status()
                .map_err(|error| format!("系统代理守护状态未知：{error}"));
        }
        match &self.system_proxy {
            Some(proxy) => proxy.status().map_err(|error| error.to_string()),
            None => Ok(lanpath_client::sysproxy::SystemProxyOwnership::Unowned),
        }
    }

    pub(super) fn proxy_recovery_pending(&self) -> Result<bool, String> {
        #[cfg(windows)]
        return self
            .proxy_ownership()
            .map(|state| state != lanpath_client::sysproxy::SystemProxyOwnership::Unowned);
        #[cfg(not(windows))]
        Ok(false)
    }

    pub(crate) fn network_recovery_required(&self) -> Result<bool, String> {
        self.executor()?.block_on(async {
            // Prepared/partial helper sessions remain owned until a proven stop.
            let running = self.running_async().await?;
            Ok(self.proxy_recovery_pending()?
                || (!running && self.manager.lock().await.is_some())
                || self.broker.lock().await.is_some()
                || self.mitm.lock().await.is_some())
        })
    }
}
