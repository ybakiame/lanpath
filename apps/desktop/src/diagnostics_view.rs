//! 诊断：目标、一次性检查、本地规则预估与快速规则交接。
use crate::{
    Page,
    rules::{RuleTrace, Rules, trace_rules},
    ui::{
        BadgeVariant, badge, badge_with_icon,
        button::{ButtonSize, ButtonVariant, button},
        empty_with_icon,
        icon::{Icon, Tone, icon_sized},
        input::{field, input},
        layout::{AlertTone, SurfaceLevel, alert_banner, page_header, section_title, surface},
        spinner,
    },
};
use lanpath_client::diagnose::{DiagReport, DiagStatus};
use nsh_base::theme;
use nsh_core::prelude::*;

#[derive(Clone)]
struct State {
    target: Signal<String>,
    busy: Signal<bool>,
    report: Signal<Option<DiagReport>>,
    checked_at: Signal<String>,
    trace: Signal<Option<RuleTrace>>,
    error: Signal<Option<String>>,
    task: Stored<Option<Spawned<()>>>,
}

pub(crate) fn diagnostics_page(page: Signal<Page>, seed: Signal<String>) -> impl IntoElement {
    let state = State {
        target: signal("example.com".into()),
        busy: signal(false),
        report: signal_with(None, SignalEquals::Never),
        checked_at: signal(String::new()),
        trace: signal(None),
        error: signal(None),
        task: stored(None),
    };
    let run = state.clone();
    let enter = state.clone();
    let local = state.clone();
    let add = state.clone();
    let summary = state.clone();
    let empty_check = state.clone();
    let report_state = state.clone();
    let target = state.target;
    let busy = state.busy;
    let error = state.error;
    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_col()
        .gap_4()
        .child(page_header(
            || t!("diagnostics.title"),
            || t!("diagnostics.subtitle"),
        ))
        .child(
            surface(SurfaceLevel::Card)
                .p_4()
                .flex_col()
                .gap_3()
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_end()
                        .gap_3()
                        .child(
                            view().flex_1().min_w(px(220.0)).child(
                                field().label(|| t!("diagnostics.target")).control(
                                    input(target)
                                        .test_id("diagnostics-target")
                                        .accessible_name(|| t!("diagnostics.target"))
                                        .disabled(move || busy.get())
                                        .on_change(move |v| target.set(v))
                                        .on_submit(move || run_checks(enter.clone())),
                                ),
                            ),
                        )
                        .child(
                            button(ButtonVariant::Primary)
                                .size(ButtonSize::Md)
                                .prefix(|| icon_sized(Icon::Activity, Tone::OnPrimary, 14.0))
                                .label(|| t!("diagnostics.run"))
                                .loading(move || busy.get())
                                .test_id("diagnostics-run")
                                .on_press(move || run_checks(run.clone())),
                        ),
                )
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(
                            button(ButtonVariant::Outline)
                                .size(ButtonSize::Sm)
                                .prefix(|| icon_sized(Icon::Rules, Tone::Muted, 14.0))
                                .label(|| t!("diagnostics.trace"))
                                .disabled(move || busy.get())
                                .on_press(move || {
                                    match crate::diagnostics::normalize_target(&local.target.get())
                                        .and_then(|target| {
                                            Rules::open()?
                                                .load()
                                                .map(|rules| trace_rules(&rules, &target))
                                        }) {
                                        Ok(trace) => {
                                            local.trace.set(Some(trace));
                                            local.error.set(None);
                                        }
                                        Err(error) => local.error.set(Some(error.to_string())),
                                    }
                                }),
                        )
                        .child(
                            button(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .prefix(|| icon_sized(Icon::Plus, Tone::Muted, 14.0))
                                .label(|| t!("diagnostics.add_rule"))
                                .disabled(move || busy.get())
                                .on_press(move || {
                                    match crate::diagnostics::normalize_target(&add.target.get()) {
                                        Ok(target) => {
                                            seed.set(target);
                                            page.set(Page::Rules);
                                        }
                                        Err(error) => add.error.set(Some(error.to_string())),
                                    }
                                }),
                        ),
                ),
        )
        .child(when(
            move || error.get().is_some(),
            move || {
                alert_banner(
                    AlertTone::Danger,
                    text(move || error.get().unwrap_or_default())
                        .text_size(px(12.5))
                        .text_color(move || theme().destructive),
                )
            },
        ))
        .child(when(
            move || busy.get(),
            || {
                alert_banner(
                    AlertTone::Info,
                    view()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(spinner())
                        .child(
                            text(|| t!("diagnostics.running"))
                                .text_size(px(12.5))
                                .text_color(move || theme().foreground),
                        ),
                )
            },
        ))
        .child(when(
            move || summary.report.get().is_some(),
            move || {
                let summary = summary.clone();
                alert_banner(
                    AlertTone::Info,
                    text(move || {
                        summary
                            .report
                            .get()
                            .map(|report| {
                                t!(
                                    "diagnostics.summary",
                                    target = report.domain,
                                    time = summary.checked_at.get(),
                                    failed = report.failed_steps.to_string()
                                )
                            })
                            .unwrap_or_default()
                    })
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(move || theme().foreground),
                )
            },
        ))
        .child(
            scroll().flex_1().min_h(px(0.0)).child(
                view()
                    .flex_col()
                    .gap_3()
                    .child(when(
                        move || {
                            empty_check.report.get().is_none()
                                && empty_check.trace.get().is_none()
                                && !empty_check.busy.get()
                        },
                        || {
                            empty_with_icon(
                                Some(Icon::Activity),
                                || t!("diagnostics.title"),
                                || t!("diagnostics.empty"),
                            )
                        },
                    ))
                    .child(when(
                        move || state.trace.get().is_some(),
                        move || {
                            let trace = state.trace.get();
                            surface(SurfaceLevel::Card)
                                .p_4()
                                .flex_col()
                                .gap_2()
                                .child(section_title(|| t!("diagnostics.local_title")))
                                .child(
                                    text(move || {
                                        trace
                                            .as_ref()
                                            .map(|r| {
                                                t!(
                                                    "diagnostics.local_result",
                                                    outbound = r.outbound.clone().unwrap_or_else(
                                                        || t!("diagnostics.no_match")
                                                    ),
                                                    uncertain = r.uncertain_before.to_string()
                                                )
                                            })
                                            .unwrap_or_default()
                                    })
                                    .text_size(px(13.5))
                                    .font_weight(FontWeight::MEDIUM),
                                )
                                .child(
                                    text({
                                        let trace = state.trace;
                                        move || {
                                            trace
                                                .get()
                                                .map(|r| {
                                                    format!(
                                                        "{} · {}",
                                                        r.target,
                                                        r.matched.unwrap_or_else(|| {
                                                            t!("diagnostics.no_match")
                                                        })
                                                    )
                                                })
                                                .unwrap_or_default()
                                        }
                                    })
                                    .text_size(px(12.5))
                                    .selectable(true),
                                )
                                .child(
                                    text(|| t!("diagnostics.local_hint"))
                                        .text_size(px(12.0))
                                        .text_color(move || theme().muted_foreground),
                                )
                        },
                    ))
                    .child(when(
                        move || report_state.report.get().is_some(),
                        move || {
                            view().flex_col().gap_3().children(
                                report_state
                                    .report
                                    .get()
                                    .map(|r| r.steps)
                                    .unwrap_or_default()
                                    .into_iter()
                                    .map(|step| {
                                        let (status_key, variant, badge_icon) = match step.status {
                                            DiagStatus::Ok => (
                                                "diagnostics.ok",
                                                BadgeVariant::Success,
                                                Some(Icon::Check),
                                            ),
                                            DiagStatus::Fail => (
                                                "diagnostics.fail",
                                                BadgeVariant::Destructive,
                                                Some(Icon::Warning),
                                            ),
                                            DiagStatus::Skip => {
                                                ("diagnostics.skip", BadgeVariant::Outline, None)
                                            }
                                            DiagStatus::Info => {
                                                ("diagnostics.info", BadgeVariant::Default, None)
                                            }
                                        };
                                        let duration = format!("{} ms", step.duration_ms);
                                        surface(SurfaceLevel::Card)
                                            .p_4()
                                            .flex_col()
                                            .gap_2()
                                            .child(
                                                view()
                                                    .flex_row()
                                                    .flex_wrap()
                                                    .items_center()
                                                    .justify_between()
                                                    .gap_2()
                                                    .child(
                                                        view()
                                                            .flex_row()
                                                            .items_center()
                                                            .gap_2()
                                                            .child(badge_with_icon(
                                                                variant,
                                                                badge_icon,
                                                                move || t!(status_key),
                                                            ))
                                                            .child(
                                                                text(step.name)
                                                                    .text_size(px(14.0))
                                                                    .font_weight(
                                                                        FontWeight::SEMIBOLD,
                                                                    ),
                                                            ),
                                                    )
                                                    .child(badge(
                                                        BadgeVariant::Outline,
                                                        move || duration.clone(),
                                                    )),
                                            )
                                            .child(
                                                text(step.summary)
                                                    .text_size(px(13.0))
                                                    .selectable(true),
                                            )
                                            .child(
                                                text(step.detail)
                                                    .text_size(px(12.0))
                                                    .text_color(move || theme().muted_foreground)
                                                    .selectable(true),
                                            )
                                            .into_element()
                                    }),
                            )
                        },
                    )),
            ),
        )
}

fn run_checks(state: State) {
    if state.busy.get() {
        return;
    }
    let target = match crate::diagnostics::normalize_target(&state.target.get()) {
        Ok(target) => target,
        Err(error) => {
            state.error.set(Some(error.to_string()));
            return;
        }
    };
    state.busy.set(true);
    state.error.set(None);
    let done = state.clone();
    state.task.with(|slot| {
        slot.replace(spawn(async move {
            let result = background(async move { crate::diagnostics::run(target).await }).await;
            done.busy.set(false);
            match result {
                Ok(Ok(report)) => {
                    done.report.set(Some(report));
                    done.checked_at
                        .set(chrono::Local::now().format("%H:%M:%S").to_string());
                }
                Ok(Err(error)) => done.error.set(Some(error.to_string())),
                Err(error) => done.error.set(Some(error.to_string())),
            }
        }))
    });
}
