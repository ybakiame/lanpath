//! CoreRuntime 的真实状态 poll：`status()` 每次调用都核验核心进程活性；
//! 异常退出在此被发现并记录（不依赖按钮状态）。系统代理 ownership 随
//! 同一快照如实上报（守护真实状态，守护已退出是 RecoveryRequired）。

use std::sync::atomic::Ordering;

use super::CoreRuntime;
use super::types::{CoreStatus, SessionSummary};
use crate::core_runtime::{PHASE_STARTING, PHASE_STOPPING};

impl CoreRuntime {
    /// 真实状态 poll：每次调用都核验核心进程活性；异常退出在此被发现并
    /// 记录（不依赖按钮状态）。系统代理 ownership 同一快照如实上报。
    pub(crate) async fn status(&self) -> CoreStatus {
        let operation = match self.phase.load(Ordering::Acquire) {
            PHASE_STARTING => Some("正在启动"),
            PHASE_STOPPING => Some("正在停止"),
            _ => None,
        };
        let stage = self.stage.lock().ok().and_then(|slot| slot.clone());
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
            let message = if detail.is_empty() {
                "核心进程已退出".to_owned()
            } else {
                format!("核心进程异常退出：{detail}")
            };
            if let Ok(mut exit) = self.unexpected_exit.lock() {
                *exit = Some(message);
            }
            *slot = None;
            if let Ok(mut active) = self.active.lock() {
                *active = None;
            }
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

    pub(super) fn exit_snapshot(&self) -> Option<String> {
        self.unexpected_exit
            .lock()
            .ok()
            .and_then(|exit| exit.clone())
    }
}
