use std::{net::Ipv4Addr, sync::Arc, time::Duration};

use lanpath_client::{
    ClientConfig,
    core_config::{push_clash_mode, wait_clash_api_ready},
    mitm::MitmSession,
    replay_group_selections,
};
use lanpath_engine::{CoreManager, SingBoxProcessManager};

use super::{DesktopRuntime, RunResources, system_control::broker_request};
use crate::{
    privileged::{Broker, SessionState, StopKind},
    startup::StartupReporter,
};

impl DesktopRuntime {
    /// Background executor only. Keep inventory changes outside Tokio block_on
    /// while one lease excludes start/stop and another core mutation.
    pub(crate) fn mutate_core_inventory<T>(
        &self,
        change: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let _lease = self.executor()?.block_on(self.lifecycle_gate.lock());
        if self.running()? {
            return Err("核心运行期间不可修改核心文件或选择".into());
        }
        let retained = self.executor()?.block_on(async {
            self.manager.lock().await.is_some()
                || self.broker.lock().await.is_some()
                || self.mitm.lock().await.is_some()
                || self.script_tasks.lock().await.is_some()
        });
        if retained {
            return Err("核心仍有待回收资源；请先停止或恢复后再修改核心".into());
        }
        change()
    }

    /// Called on the background executor; one lease spans start and its rollback.
    pub(crate) fn start(&self) -> Result<(), String> {
        self.executor()?.block_on(async {
            let _lease = self.lifecycle_gate.lock().await;
            if self.running_async().await? {
                return Err("核心已运行；请先停止后切换核心".into());
            }
            // Restore abandoned WinINet ownership before reaping any stale core.
            self.recover_proxy_before_start()?;
            let _ = self.stop_locked().await?;
            let mut progress = StartupReporter::new(&self.startup);
            let result = self.start_selected(&mut progress).await;
            let result = match result {
                Ok(()) => Ok(()),
                Err(error) => match self.stop_locked().await {
                    Ok(_) => Err(error),
                    Err(recovery) => Err(format!("{error}；启动回收未完成：{recovery}")),
                },
            };
            progress.finish(&result);
            result
        })
    }

    async fn start_selected(&self, progress: &mut StartupReporter<'_>) -> Result<(), String> {
        progress.stage("selection", "读取已保存核心和系统设置");
        let business = self.business()?;
        let client = business
            .client_config()
            .map_err(|error| error.to_string())?;
        if client.tun_enabled && !crate::product::TUN_SUPPORTED {
            return Err(
                "当前发行未开放 TUN；原设置已保留，请关闭 TUN 后启动。不会发起提权。".into(),
            );
        }
        let binary = self.core_override().unwrap_or(&client.core_binary);
        if binary.as_os_str().is_empty() {
            return Err("尚未选择核心；请在“核心”中下载并选择 sing-box".into());
        }
        if client.tun_enabled && self.core_override().is_some() {
            return Err("TUN 不接受环境变量指定的核心".into());
        }
        self.saved_proxy_port_changed(client.mixed_port);
        progress.stage("data", "准备运行数据");
        self.observations()?.prepare().await?;
        progress.stage("ports", "预检实际监听端口");
        self.preflight_ports(&client).await?;
        progress.stage("mitm", "准备 MITM 链");
        let mitm = self
            .mitm_service()?
            .start(&client, self.notifier()?)
            .await
            .map_err(|error| error.to_string())?;
        let chain = mitm.as_ref().map(MitmSession::chain);
        let script_port = chain
            .as_ref()
            .map_or(client.mixed_port, |chain| chain.return_port);
        *self.mitm.lock().await = mitm;
        progress.stage("configuration", "生成配置与补齐资源");
        let (config, missing) = business
            .build_config_report(true, chain, client.clone())
            .await
            .map_err(|error| error.to_string())?;
        progress.stage("validation", "真实核心配置校验");
        business
            .validator(&client)
            .validate_value(&config)
            .await
            .map_err(|error| error.to_string())?;
        let files = config
            .pointer("/route/rule_set")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                if entry.get("type")?.as_str()? != "local" {
                    return None;
                }
                Some((
                    entry.get("tag")?.as_str()?.to_owned(),
                    entry.get("path")?.as_str()?.to_owned(),
                ))
            })
            .collect();
        let mut protected = crate::privileged::input_paths(business.data_dir(), &config)
            .map_err(|error| error.to_string())?;
        protected.push(binary.canonicalize().map_err(|error| error.to_string())?);
        protected.push(business.data_dir().join("sing-box.json"));
        protected.push(business.data_dir().join("sing-box.pid"));
        progress.stage(
            "process",
            if client.tun_enabled {
                "授权受限 TUN helper"
            } else {
                "启动用户态核心"
            },
        );
        if client.tun_enabled {
            self.start_tun(config, binary).await?;
        } else {
            let manager = Arc::new(
                SingBoxProcessManager::new(binary, business.data_dir())
                    .map_err(|error| error.to_string())?,
            );
            *self.manager.lock().await = Some(manager.clone());
            manager
                .start(&config)
                .await
                .map_err(|error| error.to_string())?;
        }
        self.retain_live_inputs(client.mixed_port, script_port, protected)
            .await?;
        progress.stage("listener", "等待实际代理监听");
        self.wait_listener(client.mixed_port).await?;
        if script_port != client.mixed_port {
            self.wait_listener(script_port).await?;
        }
        if client.clash_api_enabled {
            progress.stage("api", "等待已配置 Clash API");
            wait_clash_api_ready(
                client.clash_api_port,
                &client.clash_api_secret,
                Duration::from_secs(15),
            )
            .await
            .map_err(|error| error.to_string())?;
            progress.stage("restore", "恢复模式与分组");
            let mode = business.rule_mode().map_err(|error| error.to_string())?;
            push_clash_mode(client.clash_api_port, &client.clash_api_secret, &mode)
                .await
                .map_err(|error| error.to_string())?;
            replay_group_selections(
                client.clash_api_port,
                &client.clash_api_secret,
                business.data_dir(),
            )
            .await;
            progress.stage("observation", "启动统计与认证日志订阅");
            self.observations()?
                .start(client.clash_api_port, &client.clash_api_secret)
                .await?;
            self.logs
                .start(client.clash_api_port, &client.clash_api_secret)
                .await?;
        } else {
            progress.warning("Clash API 已关闭；运行观测和在线策略控制不可用".into());
        }
        self.script_http
            .as_ref()
            .ok_or_else(|| self.unavailable_data_error())?
            .set_core_port(Some(script_port));
        progress.stage("system-proxy", "应用显式保存的系统代理选择");
        self.acquire_proxy(&client)?;
        // No scheduled request may run before its real endpoint and ownership policy exist.
        progress.stage("tasks", "启动脚本任务");
        if let Err(error) = self.start_script_tasks().await {
            tracing::warn!(%error, "脚本任务调度器启动失败；核心保留运行");
            progress.warning(format!("脚本任务调度器未启动：{error}"));
        }
        self.ready_live_inputs().await?;
        *self
            .missing_rule_sets
            .lock()
            .map_err(|_| "运行资源状态锁不可用".to_string())? = RunResources { missing, files };
        Ok(())
    }

    async fn preflight_ports(&self, client: &ClientConfig) -> Result<(), String> {
        let return_port = if client.mitm_enabled
            && lanpath_client::mitm::MitmWhitelistView::load(self.business()?.data_dir(), client)
                .has_inclusions()
        {
            Some(
                client
                    .mixed_port
                    .checked_add(1)
                    .ok_or("MITM 回流端口溢出")?,
            )
        } else {
            None
        };
        let ports = [
            Some(client.mixed_port),
            client.clash_api_enabled.then_some(client.clash_api_port),
            return_port,
        ];
        let mut listeners = [None, None, None];
        for (slot, port) in listeners.iter_mut().zip(ports) {
            if let Some(port) = port {
                if port == 0 {
                    return Err("监听端口不能为零".into());
                }
                *slot = Some(
                    tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))
                        .await
                        .map_err(|error| format!("监听端口 {port} 不可用：{error}"))?,
                );
            }
        }
        Ok(())
    }

    async fn wait_listener(&self, port: u16) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            if tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port))
                .await
                .is_ok()
            {
                return Ok(());
            }
            if !self.running_async().await? {
                return Err("核心在监听就绪前已退出".into());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!("等待代理监听 {port} 超时"));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub(super) async fn running_async(&self) -> Result<bool, String> {
        if let Some(status) = self.broker_status().await? {
            return Ok(status.state == SessionState::Running);
        }
        let manager = self.manager.lock().await.clone();
        Ok(match manager {
            Some(manager) => manager.is_running().await,
            None => false,
        })
    }

    pub(crate) fn stop(&self) -> Result<Option<StopKind>, String> {
        self.executor()?.block_on(async {
            let _lease = self.lifecycle_gate.lock().await;
            #[cfg(windows)]
            {
                let idle = self.manager.lock().await.is_none()
                    && self.broker.lock().await.is_none()
                    && self.mitm.lock().await.is_none()
                    && self.script_tasks.lock().await.is_none();
                let has_guardian = self
                    .proxy_guardian
                    .lock()
                    .map_err(|_| "系统代理守护会话锁不可用".to_string())?
                    .is_some();
                // Explicit idle recovery may adopt an abandoned old owner.
                // Stopping a retained session remains limited to its own journal.
                if idle && !has_guardian {
                    self.recover_proxy_before_start()?;
                }
            }
            self.stop_locked().await
        })
    }

    /// Also used under the update receipt's exclusive lifecycle lease.
    pub(super) async fn stop_locked(&self) -> Result<Option<StopKind>, String> {
        // Invalid immediately, even if restoring owned effects fails. Retain the
        // protected inputs until process exit and complete cleanup are proven.
        if let Some(inputs) = self.live_inputs.lock().await.as_mut() {
            inputs.ready = false;
        }
        // A failed restore retains the referenced core and the MITM return listener.
        self.restore_proxy()?;
        let mut incomplete = None;
        let mut stopped = None;
        let broker = self.broker.lock().await.clone();
        if let Some(broker) = broker {
            let receipt = broker_request(broker, Broker::stop).await?;
            if !receipt.process_exited {
                return Err(unresolved_stop(&receipt));
            }
            if receipt.kind != StopKind::RecoveryRequired
                && receipt.precise_cleanup_proven
                && receipt.observed_remaining.is_empty()
            {
                *self.broker.lock().await = None;
                stopped = Some(receipt.kind);
            } else {
                incomplete = Some(unresolved_stop(&receipt));
            }
        }
        let manager = self.manager.lock().await.clone();
        if let Some(manager) = manager {
            let kind = manager
                .stop_process()
                .await
                .map_err(|error| error.to_string())?;
            stopped = Some(match kind {
                lanpath_common::ProcessStopKind::Forced => StopKind::Forced,
                lanpath_common::ProcessStopKind::AlreadyStopped => StopKind::AlreadyStopped,
            });
            *self.manager.lock().await = None;
        }
        if let Some(http) = &self.script_http {
            http.set_core_port(None);
        }
        self.stop_services().await?;
        *self
            .missing_rule_sets
            .lock()
            .map_err(|_| "运行资源状态锁不可用".to_string())? = RunResources::default();
        match incomplete {
            Some(error) => Err(error),
            None => {
                *self.live_inputs.lock().await = None;
                Ok(stopped)
            }
        }
    }

    async fn stop_services(&self) -> Result<(), String> {
        self.stop_script_tasks().await;
        self.logs.stop().await;
        if let Some(observations) = &self.observations {
            observations.stop().await;
        }
        self.stop_mitm().await
    }
}

fn unresolved_stop(receipt: &crate::privileged::StopReceipt) -> String {
    let (addresses, routes, dns) =
        receipt
            .observed_remaining
            .iter()
            .fold((0, 0, 0), |(addresses, routes, dns), row| {
                (
                    addresses + row.addresses.len(),
                    routes + row.routes.len(),
                    dns + row.dns_servers.len(),
                )
            });
    format!(
        "TUN 停止未完成（{:?}）：核心已退出={}，清理已证明={}；剩余接口行={}、地址项={}、路由项={}、DNS 项={}；保留 helper 和恢复证据：{}",
        receipt.kind,
        receipt.process_exited,
        receipt.precise_cleanup_proven,
        receipt.observed_remaining.len(),
        addresses,
        routes,
        dns,
        receipt.detail
    )
}
