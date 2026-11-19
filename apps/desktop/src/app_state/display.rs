//! App 级状态的展示映射：所有 `t!` 调用都在这里发生，业务状态保持语义化。
//! 这样长任务、已有结果与错误在语言切换后由绑定重新求值，不保留旧语言快照。

use lanpath_client::sysproxy::SystemProxyOwnership;
use nsh_core::prelude::*;

use super::model::{
    CommandError, CommandKind, CommandResult, CoreSelection, RuntimeView, SubscriptionSummary,
};
use crate::core_runtime::{CoreExit, CoreOperation, RuntimeStage, SessionSummary};

/// 展示用版本标签（`v1.2.3`；`alpha-` 前缀原样保留）。
pub(crate) fn core_tag(version: &str) -> String {
    let version = version.trim();
    if version.starts_with("alpha-") {
        version.to_string()
    } else {
        format!("v{}", version.trim_start_matches('v'))
    }
}

/// 运行阶段的用户文案。
fn stage_label(stage: RuntimeStage) -> String {
    t!(match stage {
        RuntimeStage::ReadSettings => "runtime.stage.read_settings",
        RuntimeStage::ValidateBinary => "runtime.stage.validate_binary",
        RuntimeStage::PreflightPorts => "runtime.stage.preflight_ports",
        RuntimeStage::ComposeConfig => "runtime.stage.compose_config",
        RuntimeStage::ValidateConfig => "runtime.stage.validate_config",
        RuntimeStage::StartProcess => "runtime.stage.start_process",
        RuntimeStage::WaitReady => "runtime.stage.wait_ready",
        RuntimeStage::RestoreProxyAndStop => "runtime.stage.restore_proxy_and_stop",
    })
}

/// 一次运行会话的摘要（类型、端口、已运行时间）。
fn session_summary(session: &SessionSummary) -> String {
    let api = session
        .clash_api_port
        .map(|port| t!("runtime.api_port", port = port))
        .unwrap_or_else(|| t!("runtime.api_off"));
    t!(
        "runtime.session_summary",
        core_type = session.core_type.to_string(),
        version = core_tag(&session.version),
        mixed = session.mixed_port,
        api = api,
        uptime = session.uptime_secs
    )
}

/// 运行状态一行文案（概览、核心页与全局状态栏共用）。
pub(crate) fn runtime_status_line(view: &RuntimeView) -> String {
    if let Some(operation) = view.operation {
        let operation = match operation {
            CoreOperation::Starting => t!("runtime.status.starting"),
            CoreOperation::Stopping => t!("runtime.status.stopping"),
        };
        return match view.stage {
            Some(stage) => t!(
                "runtime.status.operation_stage",
                operation = operation,
                stage = stage_label(stage)
            ),
            None => t!("runtime.status.operation", operation = operation),
        };
    }
    match &view.session {
        Some(session) => t!("runtime.status.running", summary = session_summary(session)),
        None => t!("runtime.status.stopped"),
    }
}

/// 异常退出事实的用户文案；原始核心输出不翻译。
pub(crate) fn exit_error_line(error: &Option<CoreExit>) -> String {
    match error {
        None => String::new(),
        Some(CoreExit::Exited) => t!("runtime.exit.exited"),
        Some(CoreExit::Failed(detail)) => t!("runtime.exit.failed", error = detail.clone()),
    }
}

/// 共享命令在途时的阶段文案；优先使用运行时真实阶段。
pub(crate) fn command_pending_line(
    command: &super::model::CommandView,
    status: &RuntimeView,
) -> String {
    if status.operation.is_some() {
        return runtime_status_line(status);
    }
    match command.kind {
        Some(CommandKind::Start) => t!("runtime.command.start"),
        Some(CommandKind::Stop) => t!("runtime.command.stop"),
        Some(CommandKind::Restart) => t!("runtime.command.restart"),
        Some(CommandKind::RestoreProxy) => t!("runtime.command.restore_proxy"),
        None => String::new(),
    }
}

/// 命令成功结果；回执明确已完成的阶段，不把运行态写进结果字符串。
pub(crate) fn command_result_line(result: &Option<CommandResult>) -> String {
    match result {
        None => String::new(),
        Some(CommandResult::Started(outcome)) => {
            let api = outcome
                .clash_api_port
                .map(|port| t!("runtime.api_port", port = port))
                .unwrap_or_else(|| t!("runtime.api_off"));
            let mut line = t!(
                "runtime.start_result",
                core_type = outcome.core_type.to_string(),
                version = core_tag(&outcome.version),
                mixed = outcome.mixed_port,
                api = api,
                nodes = outcome.node_count,
                warnings = outcome.warnings.len()
            );
            if !outcome.missing_tags.is_empty() {
                line.push_str(&t!(
                    "runtime.start_result_missing",
                    count = outcome.missing_tags.len()
                ));
            }
            line
        }
        Some(CommandResult::Stopped(kind)) => match kind {
            lanpath_common::ProcessStopKind::Forced => t!("runtime.stop.forced"),
            lanpath_common::ProcessStopKind::AlreadyStopped => t!("runtime.stop.already"),
        },
        Some(CommandResult::ProxyRestored) => t!("runtime.proxy.restored"),
    }
}

/// 命令失败原因；只给真实事实与下一步，错误原文保持原样。
pub(crate) fn command_error_line(error: &Option<CommandError>) -> String {
    match error {
        None => String::new(),
        Some(CommandError::RuntimeUnavailable) => t!("runtime.error.unavailable"),
        Some(CommandError::StartFailed(detail)) => {
            t!("runtime.error.start_failed", error = detail.clone())
        }
        Some(CommandError::StopFailed(detail)) => {
            t!("runtime.error.stop_failed", error = detail.clone())
        }
        Some(CommandError::RestartStartFailed(detail)) => {
            t!("runtime.error.restart_start_failed", error = detail.clone())
        }
        Some(CommandError::RestoreProxyFailed(detail)) => {
            t!("runtime.error.restore_proxy_failed", error = detail.clone())
        }
        Some(CommandError::ProxyUnsupported) => t!("runtime.error.proxy_unsupported"),
        Some(CommandError::TaskInterrupted) => t!("runtime.error.incomplete"),
    }
}

/// 系统代理 ownership 文案；错误是原始查询错误，不伪装成“未接管”。
pub(crate) fn proxy_ownership_line(
    ownership: &Option<Result<SystemProxyOwnership, String>>,
) -> String {
    match ownership {
        None => t!("runtime.proxy.unsupported"),
        Some(Ok(SystemProxyOwnership::Unowned)) => t!("runtime.proxy.unowned"),
        Some(Ok(SystemProxyOwnership::Owned)) => t!("runtime.proxy.owned"),
        Some(Ok(SystemProxyOwnership::RecoveryRequired)) => t!("runtime.proxy.recovery_required"),
        Some(Err(error)) => t!("runtime.proxy.unknown", error = error.clone()),
    }
}

/// 系统代理偏好文案（偏好与真实接管是两根轴）。
pub(crate) fn proxy_preference_line(preference: Option<bool>) -> String {
    match preference {
        Some(true) => t!("runtime.proxy.preference_on"),
        Some(false) => t!("runtime.proxy.preference_off"),
        None => t!("runtime.proxy.preference_unknown"),
    }
}

/// 读取当前生效订阅失败时的展示；失败不清空最后有效摘要。
pub(crate) fn subscription_error_line(error: &str) -> String {
    t!("status.subscription_read_failed", error = error.to_owned())
}

/// 读取实际选中核心失败时的展示；失败不清空最后有效摘要。
pub(crate) fn core_error_line(error: &str) -> String {
    t!("status.core_read_failed", error = error.to_owned())
}

/// 全局状态栏的短错误提示（详细错误在概览/核心页就近显示）。
pub(crate) fn summary_error_short_line() -> String {
    t!("status.summary_error_short")
}

/// 只有真实 `RecoveryRequired` 才提供恢复入口；未知查询错误先请用户检查
/// 核心页，不把查询失败说成已确认需要恢复。
pub(crate) fn proxy_recovery_visible(status: &RuntimeView) -> bool {
    status.proxy_supported
        && matches!(
            status.proxy_ownership,
            Some(Ok(SystemProxyOwnership::RecoveryRequired))
        )
}

/// 当前生效订阅摘要的展示文案。
pub(crate) fn subscription_summary_line(summary: &SubscriptionSummary) -> String {
    let nodes = if summary.node_count > 0 {
        t!("overview.subscription.nodes", count = summary.node_count)
    } else {
        t!("overview.subscription.nodes_unknown")
    };
    match summary.format {
        Some(format) => format!(
            "{} · {nodes} · {}",
            summary.name,
            t!(
                "overview.subscription.format",
                format = format!("{format:?}")
            )
        ),
        None => format!("{} · {nodes}", summary.name),
    }
}

/// 实际选中核心摘要的展示文案。
pub(crate) fn core_selection_line(selection: &CoreSelection) -> String {
    format!("{} · {}", selection.core_type, core_tag(&selection.version))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(
        supported: bool,
        ownership: Option<Result<SystemProxyOwnership, String>>,
    ) -> RuntimeView {
        RuntimeView {
            proxy_supported: supported,
            proxy_ownership: ownership,
            ..RuntimeView::default()
        }
    }

    #[test]
    fn recovery_entry_requires_supported_platform_and_confirmed_need() {
        assert!(proxy_recovery_visible(&status(
            true,
            Some(Ok(SystemProxyOwnership::RecoveryRequired))
        )));
        assert!(!proxy_recovery_visible(&status(
            true,
            Some(Ok(SystemProxyOwnership::Owned))
        )));
        assert!(!proxy_recovery_visible(&status(
            true,
            Some(Err("probe failed".into()))
        )));
        assert!(!proxy_recovery_visible(&status(
            false,
            Some(Ok(SystemProxyOwnership::RecoveryRequired))
        )));
    }
}
