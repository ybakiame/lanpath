//! CoreRuntime 的真实状态 poll：`status()` 每次调用都核验核心进程活性；
//! 异常退出在此被发现并记录（不依赖按钮状态）。系统代理 ownership 随
//! 同一快照如实上报（守护真实状态，守护已退出是 RecoveryRequired）。

use std::sync::atomic::Ordering;

use super::CoreRuntime;
use super::types::{CoreExit, CoreStatus, SessionSummary};
use crate::core_runtime::{PHASE_STARTING, PHASE_STOPPING};

impl CoreRuntime {
    /// 诊断只使用当前会话启动时的固定设置；后续保存值不冒充运行事实。
    pub(crate) fn diagnostic_context(&self) -> Option<(lanpath_client::ClientConfig, Vec<String>)> {
        self.active.lock().ok().and_then(|active| {
            active
                .as_ref()
                .map(|info| (info.config.clone(), info.missing_rule_sets.clone()))
        })
    }

    /// 真实状态 poll：每次调用都核验核心进程活性；异常退出在此被发现并
    /// 记录（不依赖按钮状态）。系统代理 ownership 同一快照如实上报。
    pub(crate) async fn status(&self) -> CoreStatus {
        let operation = match self.phase.load(Ordering::Acquire) {
            PHASE_STARTING => Some(super::types::CoreOperation::Starting),
            PHASE_STOPPING => Some(super::types::CoreOperation::Stopping),
            _ => None,
        };
        let stage = self.stage.lock().ok().and_then(|slot| *slot);
        let proxy = if self.proxy_supported() {
            Some(self.proxy_ownership())
        } else {
            None
        };
        let Ok(mut slot) = self.lifecycle.try_lock() else {
            let session = self.session_snapshot();
            return CoreStatus {
                operation,
                stage,
                running: session.is_some(),
                session,
                unexpected_exit: self.exit_snapshot(),
                proxy_supported: self.proxy_supported(),
                proxy_ownership: proxy,
            };
        };
        if let Some(manager) = slot.as_ref()
            && !manager.is_running().await
        {
            let detail = manager.last_error().await;
            let exit = if detail.is_empty() {
                CoreExit::Exited
            } else {
                CoreExit::Failed(detail)
            };
            if let Ok(mut slot) = self.unexpected_exit.lock() {
                *slot = Some(exit);
            }
            *slot = None;
            if let Ok(mut active) = self.active.lock() {
                *active = None;
            }
            self.stop_tracker().await;
        }
        let session = self.session_snapshot();
        CoreStatus {
            operation,
            stage,
            running: session.is_some(),
            session,
            unexpected_exit: self.exit_snapshot(),
            proxy_supported: self.proxy_supported(),
            proxy_ownership: proxy,
        }
    }

    pub(super) fn session_snapshot(&self) -> Option<SessionSummary> {
        self.active.lock().ok().and_then(|active| {
            active.as_ref().map(|info| SessionSummary {
                core_type: info.core_type,
                version: info.version.clone(),
                mixed_port: info.mixed_port,
                clash_api_port: info.clash_api_port,
                uptime_secs: info.started.elapsed().as_secs(),
            })
        })
    }

    pub(super) fn exit_snapshot(&self) -> Option<CoreExit> {
        self.unexpected_exit
            .lock()
            .ok()
            .and_then(|exit| exit.clone())
    }
}
