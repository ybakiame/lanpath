//! 核心页“运行”卡片渲染：真实 running/busy/结果/停止 kind/错误展示与
//! “校验并启动”“停止”按钮。状态与命令来自 App 级 `app_state` 共享信号
//! （与概览/全局状态栏同一来源，切页不重建、不互相矛盾）；所有文案由
//! 语义状态在展示层按当前语言生成。

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::app_state::{self, AppState};
use crate::ui::button::{ButtonSize, ButtonVariant, button};

pub(super) fn runtime_card() -> impl IntoElement {
    let shared = app_state::current();
    let status = shared.status;
    let command = shared.command;
    view()
        .flex_col()
        .gap_2()
        .p_3()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(text("运行").text_size(px(15.0)))
        .child(
            text("校验并启动：先以真实核心二进制校验配置（sing-box check / Mihomo -t），再以用户态 loopback mixed + Clash API 启动 · TUN/VPN 不接管；系统代理经独立守护接管/恢复（见下方分区）")
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(text(move || {
            let status = status.get();
            let command = command.get();
            if command.busy {
                return app_state::command_pending_line(&command, &status);
            }
            app_state::runtime_status_line(&status)
        }))
        .child(
            text(move || app_state::exit_error_line(&status.get().exit_error))
                .text_size(px(12.0))
                .text_color(move || theme().destructive),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child(start_button(shared.clone()))
                .child(stop_button(shared)),
        )
        .child(
            text(move || app_state::command_result_line(&command.get().result))
                .text_size(px(12.0))
                .text_color(move || theme().primary),
        )
        .child(
            text(move || app_state::command_error_line(&command.get().error))
                .text_size(px(12.0))
                .text_color(move || theme().destructive),
        )
}

/// “校验并启动”：运行中/在途/命令 busy 时禁用（共享命令单飞）。
fn start_button(shared: AppState) -> AnyElement {
    let status = shared.status;
    let command = shared.command;
    button(ButtonVariant::Primary)
        .size(ButtonSize::Sm)
        .test_id("core-start")
        .label(|| "校验并启动".to_owned())
        .disabled(move || {
            let status = status.get();
            status.running || status.operation.is_some() || command.get().busy
        })
        .on_press(move || app_state::spawn_start(&shared))
        .into_element()
}

/// “停止”：仅在真实运行中可用；命令结果（含真实停止 kind）共享反馈。
fn stop_button(shared: AppState) -> AnyElement {
    let status = shared.status;
    let command = shared.command;
    button(ButtonVariant::Outline)
        .size(ButtonSize::Sm)
        .test_id("core-stop")
        .label(|| "停止".to_owned())
        .disabled(move || {
            let status = status.get();
            !status.running || status.operation.is_some() || command.get().busy
        })
        .on_press(move || app_state::spawn_stop(&shared))
        .into_element()
}
