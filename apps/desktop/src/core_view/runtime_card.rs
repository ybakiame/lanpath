//! 核心页“运行状态”只读摘要卡片：展示来自 App 级 `app_state` 的真实
//! running/busy/结果/异常退出状态（与概览/全局状态栏同一来源）；启动与停止
//! 统一在概览页操作，避免重复入口。

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::app_state;
use crate::ui::icon::Icon;
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
                                || t!("runtime.badge.running"),
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
