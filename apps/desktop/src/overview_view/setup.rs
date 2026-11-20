use super::*;

pub(super) fn setup_steps(state: AppState, page: Signal<Page>) -> impl IntoElement {
    let subscription = state.active_subscription;
    let selection = state.active_core;
    let steps = [
        ("overview.steps.add", StepAction::Subscriptions),
        ("overview.steps.refresh", StepAction::Subscriptions),
        ("overview.steps.core", StepAction::Core),
        ("overview.steps.start", StepAction::Start),
    ];
    surface(SurfaceLevel::Card)
        .p_6()
        .flex_col()
        .gap_4()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    view()
                        .size(px(40.0))
                        .rounded(move || theme().radius)
                        .bg(move || theme().accent)
                        .items_center()
                        .justify_center()
                        .child(icon(Icon::Rules, Tone::Primary)),
                )
                .child(
                    view()
                        .flex_col()
                        .gap_1()
                        .child(
                            text(|| t!("overview.welcome"))
                                .text_size(px(20.0))
                                .font_weight(FontWeight::SEMIBOLD),
                        )
                        .child(
                            text(|| t!("overview.welcome_hint"))
                                .text_size(px(13.0))
                                .text_color(move || theme().muted_foreground),
                        ),
                ),
        )
        .child(ui::separator())
        .child(section_title(|| t!("overview.steps.title")))
        .child(
            view()
                .flex_col()
                .gap_2()
                .children(
                    steps
                        .into_iter()
                        .enumerate()
                        .map(move |(index, (key, action))| {
                            let subscription = subscription;
                            let selection = selection;
                            let state = state.clone();
                            let page = page;
                            let current = move || match (subscription.get(), selection.get()) {
                                (None, _) => 0,
                                (Some(subscription), _)
                                    if !subscription.enabled || subscription.node_count == 0 =>
                                {
                                    1
                                }
                                (Some(_), None) => 2,
                                (Some(_), Some(_)) => 3,
                            };
                            step_row(
                                index + 1,
                                move || t!(key),
                                move || current() == index,
                                move || index < current(),
                                action,
                                page,
                                state,
                            )
                        }),
                ),
        )
}

#[derive(Clone, Copy)]
enum StepAction {
    Subscriptions,
    Core,
    Start,
}

fn step_row(
    number: usize,
    label: impl Fn() -> String + 'static,
    actionable: impl Fn() -> bool + 'static,
    done: impl Fn() -> bool + 'static,
    action: StepAction,
    page: Signal<Page>,
    state: AppState,
) -> impl IntoElement {
    let actionable: std::rc::Rc<dyn Fn() -> bool> = std::rc::Rc::new(actionable);
    let done: std::rc::Rc<dyn Fn() -> bool> = std::rc::Rc::new(done);
    let status = state.status;
    let command = state.command;
    let action_element: std::rc::Rc<dyn Fn(&AppState) -> AnyElement> =
        std::rc::Rc::new(move |state: &AppState| match action {
            StepAction::Subscriptions => button(ButtonVariant::Primary)
                .size(ButtonSize::Md)
                .test_id("overview-step-action")
                .label(|| t!("overview.steps.go_subscriptions"))
                .on_press(move || page.set(Page::Subscription))
                .into_element(),
            StepAction::Core => button(ButtonVariant::Primary)
                .size(ButtonSize::Md)
                .test_id("overview-step-action")
                .label(|| t!("overview.steps.go_core"))
                .on_press(move || page.set(Page::Core))
                .into_element(),
            StepAction::Start => button(ButtonVariant::Primary)
                .size(ButtonSize::Md)
                .test_id("overview-step-action")
                .label(|| t!("overview.runtime.start"))
                .disabled(move || {
                    let status = status.get();
                    status.running || status.operation.is_some() || command.get().busy
                })
                .on_press({
                    let state = state.clone();
                    move || app_state::spawn_start(&state)
                })
                .into_element(),
        });
    let row_active = actionable.clone();
    let row_border_active = actionable.clone();
    let badge_done = done.clone();
    let badge_active = actionable.clone();
    let text_done = done.clone();
    let text_active = actionable.clone();
    let label_active = actionable.clone();
    let show_done = done.clone();
    let show_action = actionable.clone();
    view()
        .flex_row()
        .items_center()
        .gap_3()
        .px_4()
        .py_3()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || {
            if row_border_active() {
                theme().primary
            } else {
                theme().border
            }
        })
        .bg(move || {
            if row_active() {
                theme().accent.opacity(0.45)
            } else {
                theme().muted.opacity(0.25)
            }
        })
        .child(
            view()
                .size(px(28.0))
                .flex_none()
                .rounded_full()
                .bg(move || {
                    if badge_done() {
                        theme().color("success-surface").unwrap_or(theme().accent)
                    } else if badge_active() {
                        theme().primary
                    } else {
                        theme().muted
                    }
                })
                .items_center()
                .justify_center()
                .child(
                    text(move || {
                        if done() {
                            "✓".to_owned()
                        } else {
                            number.to_string()
                        }
                    })
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(move || {
                        if text_done() {
                            theme().color("success").unwrap_or(theme().primary)
                        } else if text_active() {
                            theme().primary_foreground
                        } else {
                            theme().muted_foreground
                        }
                    }),
                ),
        )
        .child(
            text(label)
                .text_size(px(14.0))
                .style_when(
                    {
                        let active = label_active.clone();
                        move || active()
                    },
                    |s| s.font_weight(FontWeight::MEDIUM),
                )
                .text_color(move || {
                    if label_active() {
                        theme().foreground
                    } else {
                        theme().muted_foreground
                    }
                }),
        )
        .child(view().flex_1())
        .child(when(
            move || show_done(),
            || ui::badge(ui::BadgeVariant::Success, || t!("overview.steps.done")),
        ))
        .child(when(move || show_action(), move || action_element(&state)))
}
