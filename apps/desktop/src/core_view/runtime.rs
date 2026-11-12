//! 核心页“运行”卡片：真实运行状态视图模型、校验并启动/停止后台操作、
//! 状态轮询（发现核心异常退出并更新）。渲染在 `runtime_card.rs`。

use std::time::Duration;

use lanpath_common::ProcessStopKind;

use crate::core_runtime::{CoreStatus, StartOutcome};

use super::{CorePageState, WorkOutcome, core_tag, spawn_work};

/// 状态轮询间隔：运行中/异常退出都由这里的真实 `status()` poll 发现，
/// 页面不只记按钮状态。
const STATUS_POLL: Duration = Duration::from_millis(500);

/// 运行卡片视图模型（由 `CoreStatus` 转换，跨页共享的真实状态）。
#[derive(Clone, Default, PartialEq)]
pub(super) struct RuntimeView {
    pub(super) operation: Option<&'static str>,
    pub(super) stage: Option<String>,
    pub(super) running: bool,
    pub(super) summary: Option<String>,
    pub(super) exit_error: Option<String>,
}

impl RuntimeView {
    pub(super) fn running(&self) -> bool {
        self.running
    }

    fn from_status(status: &CoreStatus) -> Self {
        let summary = status.session.as_ref().map(|session| {
            let api = session
                .clash_api_port
                .map(|port| format!("API {port}"))
                .unwrap_or_else(|| "API 关闭".to_owned());
            format!(
                "{} {} · mixed {} · {} · 已运行 {} 秒",
                session.core_type,
                core_tag(&session.version),
                session.mixed_port,
                api,
                session.uptime_secs
            )
        });
        Self {
            operation: status.operation,
            stage: status.stage.clone(),
            running: status.running,
            summary,
            exit_error: status.unexpected_exit.clone(),
        }
    }
}

/// 页面级状态轮询：随页面 owner 销毁（离页取消），重进页面重新拉起，
/// 不永久 busy；运行状态经按根共享的 `CoreRuntime` 跨页一致。
pub(super) fn spawn_status_poll(state: &CorePageState) {
    let Some(runtime) = state.runtime.clone() else {
        return;
    };
    let polling = state.clone();
    let task_owner = state.clone();
    task_owner.owner.run(move || {
        task_owner.runtime_task.with(|slot| {
            slot.replace(nsh_core::prelude::spawn(async move {
                loop {
                    let status = runtime.status().await;
                    polling.status.set(RuntimeView::from_status(&status));
                    tokio::time::sleep(STATUS_POLL).await;
                }
            }));
        });
    });
}

/// “校验并启动”：busy 单飞 + 未在运行才可发起；结果必须明确完成二进制校验。
pub(super) fn spawn_start(state: CorePageState) {
    tracing::info!(target: "lanpath_desktop_nsh::core_runtime", "收到校验并启动操作");
    if state.status.get().running() || state.status.get().operation.is_some() {
        return;
    }
    tracing::info!(target: "lanpath_desktop_nsh::core_runtime", "启动操作调度检查：busy={} runtime={}", state.busy.get(), state.runtime.is_some());
    if !state.begin_runtime_work("正在校验并启动核心…") {
        return;
    }
    let Some(runtime) = state.runtime.clone() else {
        return;
    };
    state.start_result.set(None);
    state.stop_result.set(None);
    spawn_work(
        state,
        "正在校验并启动核心…",
        None,
        move |_slot| async move {
            runtime
                .start()
                .await
                .map(WorkOutcome::Started)
                .map_err(|error| format!("启动核心失败:{error}"))
        },
    );
}

/// “停止”：仅在运行中（或 busy 外的真实状态）可发起；回执展示真实停止 kind。
pub(super) fn spawn_stop(state: CorePageState) {
    if !state.status.get().running() || state.status.get().operation.is_some() {
        return;
    }
    if !state.begin_runtime_work("正在停止核心…") {
        return;
    }
    let Some(runtime) = state.runtime.clone() else {
        return;
    };
    state.start_result.set(None);
    state.stop_result.set(None);
    spawn_work(state, "正在停止核心…", None, move |_slot| async move {
        runtime
            .stop()
            .await
            .map(WorkOutcome::Stopped)
            .map_err(|error| format!("停止核心失败:{error}"))
    });
}

/// 启动结果文案：明确“已完成二进制校验”，只含类型/版本/端口/计数。
pub(super) fn format_start(outcome: &StartOutcome) -> String {
    let api = outcome
        .clash_api_port
        .map(|port| format!("API {port}"))
        .unwrap_or_else(|| "API 关闭".to_owned());
    let mut text = format!(
        "已完成二进制校验并启动：{} {} · mixed {} · {} · {} 个节点 · {} 条警告",
        outcome.core_type,
        core_tag(&outcome.version),
        outcome.mixed_port,
        api,
        outcome.node_count,
        outcome.warnings.len(),
    );
    if !outcome.missing_tags.is_empty() {
        text.push_str(&format!(" · 缺失规则集 {} 个", outcome.missing_tags.len()));
    }
    text
}

/// 停止结果文案：真实停止 kind。
pub(super) fn format_stop(kind: ProcessStopKind) -> &'static str {
    match kind {
        ProcessStopKind::Forced => "已停止核心（强制终止并回收本次进程）",
        ProcessStopKind::AlreadyStopped => "核心进程早已退出，无需强制（已清理状态）",
    }
}
