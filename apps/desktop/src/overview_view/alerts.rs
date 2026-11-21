use super::*;
use crate::ui::layout::{AlertTone, alert_banner};

pub(super) fn summary_problems(state: AppState, page: Signal<Page>) -> impl IntoElement {
    let subscription_error = state.subscription_error;
    let core_error = state.core_error;
    let subscriptions_page = page;
    let core_page = page;
    when(
        move || subscription_error.get().is_some() || core_error.get().is_some(),
        move || {
            alert_banner(
                AlertTone::Danger,
                view()
                    .flex_col()
                    .gap_2()
                    .child(
                        text(|| t!("overview.summary.problem_title"))
                            .text_size(px(13.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(move || theme().destructive),
                    )
                    .child(when(
                        move || subscription_error.get().is_some(),
                        move || {
                            text(move || {
                                subscription_error
                                    .get()
                                    .map(|error| app_state::subscription_error_line(&error))
                                    .unwrap_or_default()
                            })
                            .test_id("overview-subscription-error")
                            .text_size(px(12.5))
                            .text_color(move || theme().destructive)
                        },
                    ))
                    .child(when(
                        move || core_error.get().is_some(),
                        move || {
                            text(move || {
                                core_error
                                    .get()
                                    .map(|error| app_state::core_error_line(&error))
                                    .unwrap_or_default()
                            })
                            .test_id("overview-core-error")
                            .text_size(px(12.5))
                            .text_color(move || theme().destructive)
                        },
                    ))
                    .child(
                        view()
                            .flex_row()
                            .gap_2()
                            .child(
                                button(ButtonVariant::Outline)
                                    .size(ButtonSize::Sm)
                                    .test_id("overview-open-subscriptions")
                                    .label(|| t!("overview.subscription.manage"))
                                    .on_press(move || subscriptions_page.set(Page::Subscription)),
                            )
                            .child(
                                button(ButtonVariant::Outline)
                                    .size(ButtonSize::Sm)
                                    .test_id("overview-open-core")
                                    .label(|| t!("overview.core.choose"))
                                    .on_press(move || core_page.set(Page::Core)),
                            ),
                    ),
            )
        },
    )
}

pub(super) fn proxy_summary(state: AppState, page: Signal<Page>) -> impl IntoElement {
    let status = state.status;
    let preference = state.proxy_preference;
    let command = state.command;
    let show_cond = state.clone();
    let core_page = page;
    when(
        move || show_cond.status.get().proxy_supported,
        move || {
            let status_line = status;
            let preference_line = preference;
            let command_state = command;
            let restore_state = state.clone();
            view()
                .flex_col()
                .gap_3()
                .p_3()
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().border)
                .bg(move || theme().muted.opacity(0.35))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(icon_sized(Icon::Shield, Tone::Muted, 15.0))
                                .child(
                                    text(move || {
                                        t!(
                                            "overview.proxy.state",
                                            preference = app_state::proxy_preference_line(
                                                preference_line.get()
                                            ),
                                            ownership = app_state::proxy_ownership_line(
                                                &status_line.get().proxy_ownership
                                            )
                                        )
                                    })
                                    .text_size(px(12.5))
                                    .text_color(move || theme().muted_foreground),
                                ),
                        )
                        .child(
                            button(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .test_id("overview-go-core-proxy")
                                .label(|| t!("overview.proxy.go_core"))
                                .on_press(move || core_page.set(Page::Core)),
                        ),
                )
                .child(when(
                    move || app_state::proxy_recovery_visible(&status.get()),
                    move || {
                        let restore = restore_state.clone();
                        let restore_command = command_state;
                        alert_banner(
                            AlertTone::Warning,
                            view()
                                .flex_row()
                                .flex_wrap()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(
                                    text(|| t!("overview.proxy.recovery_warning"))
                                        .text_size(px(12.5))
                                        .text_color(move || theme().destructive),
                                )
                                .child(
                                    button(ButtonVariant::Primary)
                                        .size(ButtonSize::Sm)
                                        .test_id("overview-proxy-recover")
                                        .label(|| t!("overview.proxy.recover"))
                                        .disabled(move || restore_command.get().busy)
                                        .on_press(move || {
                                            let _ = app_state::spawn_restore_proxy(&restore);
                                        }),
                                ),
                        )
                    },
                ))
        },
    )
}
