//! 核心运行时生命周期收尾：启动就绪等待与策略组回放、失败回滚、停止回收与 Drop 清理。

use std::sync::Arc;
use std::sync::atomic::Ordering;

use lanpath_client::ClientConfig;
use lanpath_common::{LanpathError, LanpathResult, ProcessStopKind};

use super::manager::{ActiveManager, wait_listener};
use super::{CoreRuntime, PHASE_IDLE, PHASE_STOPPING, RuntimeStage};

impl CoreRuntime {
    /// 等待 mixed 端口真实监听；Clash API 启用时等待其就绪并尽力回放策略组选择。失败由调用方回滚。
    pub(super) async fn wait_ready(
        &self,
        manager: &ActiveManager,
        config: &ClientConfig,
    ) -> LanpathResult<()> {
        wait_listener(manager, config.mixed_port, self.listener_timeout).await?;
        if config.clash_api_enabled {
            lanpath_client::core_config::wait_clash_api_ready(
                config.clash_api_port,
                &config.clash_api_secret,
                self.api_timeout,
            )
            .await?;
            // 尽力回放用户在 `app.toml` 中记住的各策略组上次选择；组/节点不存在仅记录日志不阻断启动。
            for (group, member) in &config.group_selections {
                if let Err(error) = lanpath_client::clash_select_proxy(
                    config.clash_api_port,
                    &config.clash_api_secret,
                    group,
                    member,
                )
                .await
                {
                    tracing::debug!(
                        target: "lanpath::core_runtime",
                        group,
                        member,
                        %error,
                        "启动回放策略组选择跳过"
                    );
                }
            }
        }
        Ok(())
    }

    /// 启动失败回滚：回收已 spawn 的子进程。回收失败时同时报告原错误与回收
    /// 错误，manager 保留在运行时里（再次启动/停止先重试回收）。
    pub(super) async fn rollback(
        &self,
        slot: &mut tokio::sync::MutexGuard<'_, Option<ActiveManager>>,
        manager: ActiveManager,
        original: LanpathError,
    ) -> LanpathError {
        match manager.stop_process().await {
            Ok(_) => original,
            Err(recovery) => {
                tracing::error!(target: "lanpath::core_runtime",
                    "启动失败且回收未完成：{original}；回收错误：{recovery}");
                **slot = Some(manager);
                LanpathError::Core(format!(
                    "{original}；启动回收未完成：{recovery}（可再次停止或启动重试回收）"
                ))
            }
        }
    }

    /// 停止：先恢复系统代理（守护 restore + 确认退出），再回收核心子进程
    /// 并清理本次会话状态；同 `start` 在独立任务中跑到完成（UI 离页不会让
    /// 恢复/停止半途而废）。
    pub(crate) async fn stop(self: &Arc<Self>) -> LanpathResult<ProcessStopKind> {
        let this = self.clone();
        tokio::spawn(async move { this.stop_impl().await })
            .await
            .map_err(|error| LanpathError::Core(format!("停止任务异常中断:{error}")))?
    }

    async fn stop_impl(self: &Arc<Self>) -> LanpathResult<ProcessStopKind> {
        let _mutation = self.inventory.try_mutation()?;
        let mut slot = self.lifecycle.lock().await;
        self.phase.store(PHASE_STOPPING, Ordering::Release);
        self.set_stage(RuntimeStage::RestoreProxyAndStop);
        let result = self.stop_locked(&mut slot).await;
        self.phase.store(PHASE_IDLE, Ordering::Release);
        self.clear_stage();
        if let Ok(kind) = result {
            tracing::info!(target: "lanpath::core_runtime", "核心已停止：{kind:?}");
        }
        result
    }

    pub(super) async fn stop_locked(
        &self,
        slot: &mut tokio::sync::MutexGuard<'_, Option<ActiveManager>>,
    ) -> LanpathResult<ProcessStopKind> {
        // 先守护恢复：守护死亡/通信失败不是恢复证明，按 owner 精确恢复。
        // 恢复失败（Err 提前返回）时 manager 保留：running 未清，库存仍拒绝，
        // 且可再次停止重试；绝不跳过恢复直接停核心。
        self.restore_proxy()?;
        let kind = match slot.as_ref() {
            Some(manager) => manager.stop_process().await?,
            None => ProcessStopKind::AlreadyStopped,
        };
        // 清理失败（Err 提前返回）时 manager 保留：running 未清，库存仍拒绝，
        // 且可再次停止重试。
        **slot = None;
        self.stop_tracker().await;
        if let Ok(mut active) = self.active.lock() {
            *active = None;
        }
        if let Ok(mut exit) = self.unexpected_exit.lock() {
            *exit = None;
        }
        Ok(kind)
    }
}

impl Drop for CoreRuntime {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.tracker.try_lock()
            && let Some(handle) = guard.take()
        {
            handle.abort();
        }
        // 应用退出：先尽力安全恢复系统代理（守护 shutdown = restore + 退出
        // 确认），再回收核心。恢复失败不阻断核心回收：恢复证据（日志/租约）
        // 保留，GUI 进程句柄与管道断开后独立守护会自行完成恢复；下次启动
        // 的 abandoned journal 恢复仍可重试。
        if let Ok(guardian) = self.proxy_guardian.get_mut() {
            if let Some(armed) = guardian.as_mut()
                && let Err(error) = armed.shutdown()
            {
                tracing::warn!(target: "lanpath::core_runtime",
                    "退出时系统代理守护恢复未完成（守护自行恢复/下次启动重试）：{error}");
            }
            *guardian = None;
        }
        // GUI 正常退出：按记录的 OS 进程身份回收本次子进程（spawn 时的
        // 父子绑定是 OS 级兜底；这里做确定性的显式回收，绝不按进程名强杀）。
        let identity = self
            .active
            .get_mut()
            .map(|active| active.take())
            .ok()
            .flatten()
            .map(|info| info.identity);
        if let Some(identity) = identity {
            match identity.terminate_verified() {
                Ok(kind) => {
                    tracing::info!(target: "lanpath::core_runtime", "核心随运行时释放回收：{kind:?}")
                }
                Err(error) => {
                    tracing::warn!(target: "lanpath::core_runtime",
                        "核心回收未完成（依赖 OS 父子绑定兜底）：{error}")
                }
            }
        }
    }
}
