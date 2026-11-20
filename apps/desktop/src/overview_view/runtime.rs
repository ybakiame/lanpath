use super::*;
use crate::ui::layout::{AlertTone, alert_banner};

pub(super) fn header(state: AppState) -> AnyElement {
    let status = state.status;
    let command = state.command;
    let start = state.clone();
    let stop = state.clone();
    view()
        .flex_col()
        .gap_3()
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_4()
                .child(
                    view()
                        .size(px(48.0))
                        .items_center()
                        .justify_center()
                        .rounded(move || theme().radius)
                        .bg(move || {
                            if status.get().running {
                                theme().color("success-surface").unwrap_or(theme().accent)
                            } else if command.get().error.is_some()
                                || status.get().exit_error.is_some()
                            {
                                theme()
                                    .color("danger-surface")
                                    .unwrap_or(theme().destructive.opacity(0.14))
                            } else {
                                theme().accent
                            }
                        })
                        .child(indicator(status, command)),
                )
                .child(
                    view()
                        .flex_1()
                        .min_w(px(200.0))
                        .flex_col()
                        .gap_1()
                        .child(
                            text(move || {
                                if command.get().busy {
                                    app_state::command_pending_line(&command.get(), &status.get())
                                } else if status.get().running {
                                    t!("overview.headline.running")
                                } else if command.get().error.is_some()
                                    || status.get().exit_error.is_some()
                                {
                                    t!("overview.headline.failed")
                                } else {
                                    t!("overview.headline.ready")
                                }
                            })
                            .test_id("overview-headline")
                            .text_size(px(22.0))
                            .font_weight(FontWeight::SEMIBOLD),
                        )
                        .child(
                            text(|| t!("overview.runtime_hint"))
                                .text_size(px(12.5))
                                .text_color(move || theme().muted_foreground),
                        ),
                )
                .child(
                    when(
                        move || !status.get().running,
                        move || start_button(start.clone()),
                    )
                    .otherwise(move || stop_button(stop.clone())),
                ),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    text(|| t!("overview.runtime.title"))
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(move || theme().muted_foreground),
                )
                .child(
                    text(move || app_state::runtime_status_line(&status.get()))
                        .test_id("overview-runtime-status")
                        .text_size(px(12.5))
                        .selectable(true),
                ),
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
            move || command.get().error.is_some(),
            move || {
                alert_banner(
                    AlertTone::Danger,
                    text(move || app_state::command_error_line(&command.get().error))
                        .test_id("overview-command-error")
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
                    AlertTone::Info,
                    text(move || app_state::command_result_line(&command.get().result))
                        .text_size(px(12.5))
                        .text_color(move || theme().foreground)
                        .selectable(true),
                )
            },
        ))
        .into_element()
}

fn start_button(state: AppState) -> AnyElement {
    let status = state.status;
    let command = state.command;
    button(ButtonVariant::Primary)
        .size(ButtonSize::Lg)
        .test_id("overview-start")
        .label(|| t!("overview.runtime.start"))
        .prefix(|| icon(Icon::Play, Tone::OnPrimary))
        .disabled(move || status.get().running || status.get().operation.is_some())
        .loading(move || command.get().busy)
        .on_press(move || app_state::spawn_start(&state))
        .into_element()
}

fn stop_button(state: AppState) -> AnyElement {
    let status = state.status;
    let command = state.command;
    button(ButtonVariant::Outline)
        .size(ButtonSize::Lg)
        .test_id("overview-stop")
        .label(|| t!("overview.runtime.stop"))
        .prefix(|| icon(Icon::Stop, Tone::Muted))
        .disabled(move || !status.get().running || status.get().operation.is_some())
        .loading(move || command.get().busy)
        .on_press(move || app_state::spawn_stop(&state))
        .into_element()
}

fn indicator(
    status: Signal<app_state::RuntimeView>,
    command: Signal<app_state::CommandView>,
) -> AnyElement {
    when(move || command.get().busy, ui::spinner)
        .otherwise(move || {
            when(
                move || status.get().running,
                || ui::icon::status_icon(Icon::Shield, Tone::Success),
            )
            .otherwise(move || {
                when(
                    move || command.get().error.is_some() || status.get().exit_error.is_some(),
                    || ui::icon::status_icon(Icon::Warning, Tone::Danger),
                )
                .otherwise(|| ui::icon::status_icon(Icon::Play, Tone::Primary))
            })
        })
        .into_element()
}
