//! 核心页“系统代理”分区的交互：偏好切换的安全启停周期与显式恢复。
//! 运行状态、命令与恢复任务统一由 App 级 `crate::app_state` 持有；本模块
//! 只做核心页按钮到共享命令的薄适配。

use crate::app_state;
use nsh_core::prelude::*;

use super::CorePageState;

/// ownership 展示文案：未接管/本应用持有/需要恢复/未知错误。
pub(super) fn format_proxy_ownership(
    ownership: &Option<Result<lanpath_client::sysproxy::SystemProxyOwnership, String>>,
) -> String {
    app_state::proxy_ownership_line(ownership)
}

/// 运行中的系统代理偏好变更必须和共享命令互斥：在写配置或授予授权前
/// 拒绝，避免核心已按旧快照启动而界面显示新偏好。
fn reject_while_command_busy(state: &CorePageState) -> bool {
    let shared = app_state::current();
    if shared.command.get().busy || shared.status.get().operation.is_some() {
        state
            .error
            .set(Some(t!("core.proxy.change_busy").to_string()));
        return true;
    }
    false
}

/// “接管系统代理”开关：启用先弹一次性确认框（取消不写配置）；确认后
/// 保存偏好并授予进程内一次性接管授权；禁用保存偏好并清除未消费授权。
pub(super) fn spawn_toggle_proxy(state: CorePageState, enabled: bool) {
    if state.busy.get() || reject_while_command_busy(&state) {
        return;
    }
    if enabled {
        state.proxy_confirm.set(true);
        return;
    }
    apply_proxy_enabled(state, false);
}

/// 确认框确认：关闭弹窗后应用启用（保存偏好 + 一次性授权）。
pub(super) fn confirm_enable_proxy(state: CorePageState) {
    state.proxy_confirm.set(false);
    apply_proxy_enabled(state, true);
}

/// 确认框取消：不写配置、不授权，开关保持关闭。
pub(super) fn cancel_enable_proxy(state: CorePageState) {
    state.proxy_confirm.set(false);
}

fn apply_proxy_enabled(state: CorePageState, enabled: bool) {
    if reject_while_command_busy(&state) {
        return;
    }
    let Some(runtime) = state.runtime.clone() else {
        state
            .error
            .set(Some(t!("runtime.error.unavailable").to_string()));
        return;
    };
    let previous = state.proxy_enabled.get();
    if let Err(error) = runtime.save_proxy_preference(enabled) {
        state.error.set(Some(
            t!("core.proxy.save_failed", error = error.to_string()).to_string(),
        ));
        return;
    }
    if enabled {
        runtime.grant_takeover_consent();
    } else {
        runtime.clear_takeover_consent();
    }
    state.proxy_enabled.set(enabled);
    app_state::refresh_proxy_preference(&app_state::current());
    state.error.set(None);
    if !app_state::current().status.get().running {
        crate::ui::feedback::show_toast(if enabled {
            t!("core.proxy.saved_next_start")
        } else {
            t!("core.proxy.saved_off")
        });
        return;
    }
    // 运行中切换走共享命令的安全启停周期（App 级单飞，切页不取消）。
    let shared = app_state::current();
    if !app_state::spawn_restart(&shared) {
        // 预检与共享命令之间没有 UI 并发，正常不可达；若仍发生，恢复偏好
        // 与开关到最后确认值，绝不让保存值与运行中的核心相互矛盾。
        let _ = runtime.save_proxy_preference(previous);
        runtime.clear_takeover_consent();
        state.proxy_enabled.set(previous);
        app_state::refresh_proxy_preference(&shared);
        state
            .error
            .set(Some(t!("core.proxy.restart_unavailable").to_string()));
    }
}

/// 显式“恢复系统代理”：统一走 App 级共享命令，供核心页、概览和状态栏
/// 看到同一阶段与结果；只允许本应用 owner 的精确记录。
pub(super) fn spawn_restore_proxy(_state: CorePageState) {
    let _ = app_state::spawn_restore_proxy(&app_state::current());
}
