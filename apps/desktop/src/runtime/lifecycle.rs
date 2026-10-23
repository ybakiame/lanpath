use std::{net::Ipv4Addr, sync::Arc, time::Duration};

use lanpath_client::{
    ClientConfig,
    core_config::{push_clash_mode, wait_clash_api_ready},
    mitm::MitmSession,
    remote::TracingNotifier,
    replay_group_selections,
};
use lanpath_engine::{CoreManager, SingBoxProcessManager};

use super::{CLASH_API_PORT, DesktopRuntime, MIXED_PORT, RunResources};
use crate::startup::StartupReporter;

impl DesktopRuntime {
    /// Called on the background executor; one lifecycle lease spans the attempt.
    pub(crate) fn start(&self) -> Result<(), String> {
        self.executor()?.block_on(async {
            let mut current = self.manager.lock().await;
            if let Some(manager) = current.as_ref()
                && manager.is_running().await
            {
                return Err("实验核心已运行；请先停止后切换核心".into());
            }
            let mut progress = StartupReporter::new(&self.startup);
            progress.stage("selection", "读取已保存核心");
            let result = async {
                let business = self.business()?;
                let config = business
                    .client_config()
                    .map_err(|error| error.to_string())?;
                let binary = self.core_override().unwrap_or(&config.core_binary);
                if binary.as_os_str().is_empty() {
                    return Err("尚未选择核心；请在“核心”中下载并选择 sing-box".into());
                }
                // Unlike the previous boot-time manager, every attempt resolves
                // the latest persisted selection (or explicit test override).
                let manager = Arc::new(
                    SingBoxProcessManager::new(binary, business.data_dir())
                        .map_err(|error| error.to_string())?,
                );
                manager.reap_stale();
                *current = Some(manager.clone());
                self.start_selected(&manager, &config, &mut progress).await
            }
            .await;
            let result = if let Err(error) = result {
                self.stop_services().await;
                match current.as_ref() {
                    Some(manager) => match manager.stop().await {
                        Ok(()) => Err(error),
                        Err(stop_error) => Err(format!("{error}；核心回收失败：{stop_error}")),
                    },
                    None => Err(error),
                }
            } else {
                Ok(())
            };
            progress.finish(&result);
            result
        })
    }

    async fn start_selected(
        &self,
        manager: &SingBoxProcessManager,
        client_config: &ClientConfig,
        progress: &mut StartupReporter<'_>,
    ) -> Result<(), String> {
        let business = self.business()?;
        let observations = self.observations()?;
        progress.stage("data", "准备运行数据");
        observations.prepare().await?;
        progress.stage("ports", "预检隔离端口");
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, MIXED_PORT))
            .await
            .map_err(|error| format!("实验端口 {MIXED_PORT} 不可用：{error}"))?;
        let api_listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, CLASH_API_PORT))
            .await
            .map_err(|error| format!("Clash API 端口 {CLASH_API_PORT} 不可用：{error}"))?;
        let return_listener = if client_config.mitm_enabled
            && self
                .mitm_service()?
                .whitelist(client_config)
                .has_inclusions()
        {
            let port = MIXED_PORT + 1;
            Some(
                tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))
                    .await
                    .map_err(|error| format!("MITM 回流端口 {port} 不可用：{error}"))?,
            )
        } else {
            None
        };
        drop((listener, api_listener, return_listener));
        progress.stage("mitm", "准备 MITM 链");
        self.stop_mitm().await;
        let mitm = self
            .mitm_service()?
            .start(client_config, Arc::new(TracingNotifier::new()))
            .await
            .map_err(|error| error.to_string())?;
        let chain = mitm.as_ref().map(MitmSession::chain);
        *self.mitm.lock().await = mitm;
        progress.stage("configuration", "生成配置与补齐资源");
        let (config, missing) = business
            .build_config_report(true, chain)
            .await
            .map_err(|error| error.to_string())?;
        progress.stage("validation", "核心配置校验");
        business
            .validator()
            .map_err(|error| error.to_string())?
            .validate_value(&config)
            .await
            .map_err(|error| error.to_string())?;
        progress.stage("tasks", "准备脚本任务");
        if let Err(error) = self.start_script_tasks().await {
            tracing::warn!(%error, "定时任务调度器启动失败（不影响核心启动）");
            progress.warning(format!("脚本任务调度器未启动：{error}；核心继续启动"));
        }
        progress.stage("process", "启动核心进程");
        manager
            .start(&config)
            .await
            .map_err(|error| error.to_string())?;
        progress.stage("api", "等待 Clash API");
        wait_clash_api_ready(CLASH_API_PORT, "", Duration::from_secs(15))
            .await
            .map_err(|error| error.to_string())?;
        progress.stage("restore", "恢复模式与分组");
        let mode = business.rule_mode().map_err(|error| error.to_string())?;
        push_clash_mode(CLASH_API_PORT, "", &mode)
            .await
            .map_err(|error| error.to_string())?;
        replay_group_selections(CLASH_API_PORT, "", business.data_dir()).await;
        progress.stage("observation", "启动统计与日志");
        observations.start(CLASH_API_PORT).await?;
        self.logs.start(CLASH_API_PORT).await?;
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
        *self
            .missing_rule_sets
            .lock()
            .map_err(|_| "运行资源状态锁不可用".to_string())? = RunResources { missing, files };
        Ok(())
    }

    pub(crate) fn stop(&self) -> Result<(), String> {
        self.executor()?.block_on(async {
            let mut current = self.manager.lock().await;
            self.stop_services().await;
            if let Some(manager) = current.as_ref() {
                manager.stop().await.map_err(|error| error.to_string())?;
            }
            *current = None;
            *self
                .missing_rule_sets
                .lock()
                .map_err(|_| "运行资源状态锁不可用".to_string())? = RunResources::default();
            Ok(())
        })
    }

    async fn stop_services(&self) {
        self.logs.stop().await;
        if let Some(observations) = &self.observations {
            observations.stop().await;
        }
        self.stop_script_tasks().await;
        self.stop_mitm().await;
    }
}
