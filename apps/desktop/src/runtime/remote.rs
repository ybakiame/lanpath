use std::{path::PathBuf, sync::Arc, time::Duration};

use lanpath_client::remote::{FetchReport, RemoteFetchContext, RemoteResourceService};
use lanpath_common::{LanpathError, LanpathResult, ProcessIdentity};

use super::DesktopRuntime;
use crate::privileged::SessionState;

#[derive(Clone)]
pub(super) struct LiveInputs {
    identity: ProcessIdentity,
    main_port: u16,
    request_port: u16,
    protected: Vec<PathBuf>,
    pub(super) ready: bool,
}

impl LiveInputs {
    fn verify_listeners(&self) -> LanpathResult<()> {
        verify_owned_endpoint(&self.identity, self.main_port, self.request_port)
    }
}

fn verify_owned_endpoint(
    identity: &ProcessIdentity,
    main_port: u16,
    request_port: u16,
) -> LanpathResult<()> {
    if ProcessIdentity::capture(identity.pid)? != *identity {
        return Err(LanpathError::Config(
            "核心进程身份已改变；自动更新暂缓".into(),
        ));
    }
    let ports = [
        Some(main_port),
        (request_port != main_port).then_some(request_port),
    ];
    for port in ports.into_iter().flatten() {
        let owner = lanpath_common::process::port_owner(port)
            .ok_or_else(|| LanpathError::Config(format!("无法证明监听 {port} 的所有权")))?;
        if owner.pid != identity.pid || ProcessIdentity::capture(owner.pid)? != *identity {
            return Err(LanpathError::Config(format!(
                "监听 {port} 不再属于已启动核心"
            )));
        }
    }
    Ok(())
}

impl DesktopRuntime {
    pub(super) async fn retain_live_inputs(
        &self,
        main_port: u16,
        request_port: u16,
        protected: Vec<PathBuf>,
    ) -> Result<(), String> {
        let identity = if let Some(status) = self.broker_status().await? {
            if status.state != SessionState::Running {
                return Err("TUN 核心未处于运行状态".into());
            }
            status.core.ok_or("TUN 核心缺少进程身份")?
        } else {
            // Only accept the identity record produced by this successful manager
            // start, not settings, an executable name, or a responding port.
            #[derive(serde::Deserialize)]
            struct Record {
                identity: ProcessIdentity,
            }
            let manager = self
                .manager
                .lock()
                .await
                .clone()
                .ok_or("核心管理器未就绪")?;
            if !manager.is_running().await {
                return Err("核心已退出".into());
            }
            let bytes = std::fs::read(
                self.business()?
                    .data_dir()
                    .join(format!("{}.pid", manager.core_type().binary_name())),
            )
            .map_err(|error| error.to_string())?;
            serde_json::from_slice::<Record>(&bytes)
                .map_err(|error| error.to_string())?
                .identity
        };
        *self.live_inputs.lock().await = Some(LiveInputs {
            identity,
            main_port,
            request_port,
            protected,
            ready: false,
        });
        Ok(())
    }

    pub(super) async fn ready_live_inputs(&self) -> Result<(), String> {
        let mut inputs = self.live_inputs.lock().await;
        let live = inputs.as_mut().ok_or("缺少核心启动会话")?;
        live.verify_listeners().map_err(|error| error.to_string())?;
        live.ready = true;
        Ok(())
    }

    async fn verify_live_session(&self, live: &LiveInputs) -> Result<(), String> {
        if let Some(status) = self.broker_status().await? {
            if status.state != SessionState::Running || status.core.as_ref() != Some(&live.identity)
            {
                return Err("TUN 核心会话不再匹配；自动更新已取消".into());
            }
        } else {
            let manager = self
                .manager
                .lock()
                .await
                .clone()
                .ok_or("核心管理器已失效")?;
            if !manager.is_running().await {
                return Err("核心已退出；自动更新已取消".into());
            }
        }
        live.verify_listeners().map_err(|error| error.to_string())
    }

    /// One lifecycle lease spans admission, all dependent downloads and publication.
    /// Dropping this future cancels the real writer before releasing the lease.
    pub(crate) async fn fetch_due_remote_resources(&self) -> Result<Option<FetchReport>, String> {
        let _lease = self.lifecycle_gate.lock().await;
        let business = self.business()?;
        let config = business
            .client_config()
            .map_err(|error| error.to_string())?;
        let mut live = self.live_inputs.lock().await.clone();
        let context = if let Some(live) = &mut live {
            if !live.ready {
                return Err("核心会话未就绪或待恢复；自动更新暂缓".into());
            }
            if let Err(error) = self.verify_live_session(live).await {
                if let Some(inputs) = self.live_inputs.lock().await.as_mut() {
                    inputs.ready = false;
                }
                return Err(error);
            }
            let identity = live.identity.clone();
            let main_port = live.main_port;
            let request_port = live.request_port;
            RemoteFetchContext::running(
                config,
                live.main_port,
                live.request_port,
                self.owns_system_proxy()?,
                std::mem::take(&mut live.protected),
            )
            .map_err(|error| error.to_string())?
            .with_live_guard(Arc::new(move || {
                verify_owned_endpoint(&identity, main_port, request_port)
            }))
        } else {
            if self.running_async().await?
                || self.manager.lock().await.is_some()
                || self.broker.lock().await.is_some()
                || self.mitm.lock().await.is_some()
                || self.script_tasks.lock().await.is_some()
                || self.proxy_recovery_pending()?
            {
                return Err("核心仍有未证明回收的资源；自动更新暂缓".into());
            }
            RemoteFetchContext::stopped(config).map_err(|error| error.to_string())?
        };
        let service = RemoteResourceService::new(business.data_dir().to_owned());
        let fetch = service.fetch_due(&context);
        tokio::pin!(fetch);
        let mut check = tokio::time::interval(Duration::from_millis(100));
        loop {
            tokio::select! {
                biased;
                _ = check.tick(), if live.is_some() => {
                    if let Some(live) = &live
                        && let Err(error) = self.verify_live_session(live).await {
                        if let Some(inputs) = self.live_inputs.lock().await.as_mut() {
                            inputs.ready = false;
                        }
                        return Err(error);
                    }
                }
                result = &mut fetch => return result.map_err(|error| error.to_string()),
            }
        }
    }

    fn owns_system_proxy(&self) -> Result<bool, String> {
        #[cfg(windows)]
        {
            use lanpath_client::sysproxy::SystemProxyOwnership;
            match self.proxy_ownership()? {
                SystemProxyOwnership::Owned => Ok(true),
                SystemProxyOwnership::Unowned => Ok(false),
                SystemProxyOwnership::RecoveryRequired => Err("系统代理所有权待恢复".into()),
            }
        }
        #[cfg(not(windows))]
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listener_proof_rejects_missing_listener_and_creation_mismatch() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut live = LiveInputs {
            identity: ProcessIdentity::capture(std::process::id()).unwrap(),
            main_port: port,
            request_port: port,
            protected: Vec::new(),
            ready: true,
        };
        live.verify_listeners().unwrap();
        live.identity.created = live.identity.created.wrapping_add(1);
        assert!(live.verify_listeners().is_err());
        live.identity = ProcessIdentity::capture(std::process::id()).unwrap();
        drop(listener);
        assert!(live.verify_listeners().is_err());
    }
}
