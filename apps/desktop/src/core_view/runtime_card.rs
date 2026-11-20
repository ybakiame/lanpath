//! 核心页“运行”卡片渲染：真实 running/busy/结果/停止 kind/错误展示与
//! “校验并启动”“停止”按钮。状态与命令来自 App 级 `app_state` 共享信号
//! （与概览/全局状态栏同一来源，切页不重建、不互相矛盾）；所有文案由
//! 语义状态在展示层按当前语言生成。

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::app_state::{self, AppState};
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon};
use crate::ui::layout::{AlertTone, SurfaceLevel, alert_banner, section_title, surface};

pub(super) fn runtime_card() -> impl IntoElement {
    let shared = app_state::current();
    let status = shared.status;
    let command = shared.command;
    surface(SurfaceLevel::Card)
        .flex_1()
        .min_w(px(0.0))
        .p_5()
        .flex_col()
        .gap_3()
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_2()
                .child(section_title(|| t!("overview.runtime.title")))
                .child(
                    when(
                        move || status.get().running,
                        || {
                            crate::ui::badge_with_icon(
                                crate::ui::BadgeVariant::Success,
                                Some(Icon::Shield),
                                || t!("runtime.status.running"),
                            )
                        },
                    )
                    .otherwise(|| {
                        crate::ui::badge(crate::ui::BadgeVariant::Outline, || {
                            t!("runtime.status.stopped")
                        })
                    }),
                ),
        )
        .child(
            text(|| t!("core.runtime_hint"))
                .text_size(px(12.5))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            text(move || {
                let status = status.get();
                let command = command.get();
                if command.busy {
                    return app_state::command_pending_line(&command, &status);
                }
                app_state::runtime_status_line(&status)
            })
            .text_size(px(13.5))
            .font_weight(FontWeight::MEDIUM)
            .selectable(true),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_3()
                .child(start_button(shared.clone()))
                .child(stop_button(shared)),
        )
        .child(when(
            move || status.get().exit_error.is_some(),
            move || {
                alert_banner(
                    AlertTone::Danger,
                    text(move || app_state::exit_error_line(&status.get().exit_error))
                        .text_size(px(12.5))
                        .text_color(move || theme().destructive)
                        .selectable(true),
                )
            },
        ))
        .child(when(
            move || command.get().result.is_some(),
            move || {
                alert_banner(
                    AlertTone::Success,
                    text(move || app_state::command_result_line(&command.get().result))
                        .text_size(px(12.5))
                        .text_color(move || theme().primary)
                        .selectable(true),
                )
            },
        ))
        .child(when(
            move || command.get().error.is_some(),
            move || {
                alert_banner(
                    AlertTone::Danger,
                    text(move || app_state::command_error_line(&command.get().error))
                        .text_size(px(12.5))
                        .text_color(move || theme().destructive)
                        .selectable(true),
                )
            },
        ))
}

/// “校验并启动”：运行中/在途/命令 busy 时禁用（共享命令单飞）。
fn start_button(shared: AppState) -> AnyElement {
    let status = shared.status;
    let command = shared.command;
    button(ButtonVariant::Primary)
        .size(ButtonSize::Md)
        .test_id("core-start")
        .prefix(|| icon(Icon::Play, Tone::OnPrimary))
        .label(|| t!("overview.runtime.start"))
        .loading(move || command.get().busy && !status.get().running)
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
        .size(ButtonSize::Md)
        .test_id("core-stop")
        .prefix(|| icon(Icon::Stop, Tone::Muted))
        .label(|| t!("overview.runtime.stop"))
        .loading(move || command.get().busy && status.get().running)
        .disabled(move || {
            let status = status.get();
            !status.running || status.operation.is_some() || command.get().busy
        })
        .on_press(move || app_state::spawn_stop(&shared))
        .into_element()
}
