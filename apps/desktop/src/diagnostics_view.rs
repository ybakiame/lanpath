//! 诊断：目标、一次性检查、本地规则预估与快速规则交接。
use crate::{
    Page,
    rules::{RuleTrace, Rules, trace_rules},
    ui::{
        button::{ButtonVariant, button},
        input::{field, input},
        layout::{SurfaceLevel, page_header, section_title, surface},
    },
};
use lanpath_client::diagnose::{DiagReport, DiagStatus};
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
    let report_state = state.clone();
    let target = state.target;
    let busy = state.busy;
    let error = state.error;
    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_col()
        .gap_3()
        .child(page_header(
            || t!("diagnostics.title"),
            || t!("diagnostics.subtitle"),
        ))
        .child(
            field().label(|| t!("diagnostics.target")).control(
                input(target)
                    .test_id("diagnostics-target")
                    .accessible_name(|| t!("diagnostics.target"))
                    .disabled(move || busy.get())
                    .on_change(move |v| target.set(v))
                    .on_submit(move || run_checks(enter.clone())),
            ),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .gap_3()
                .child(
                    button(ButtonVariant::Primary)
                        .label(|| t!("diagnostics.run"))
                        .loading(move || busy.get())
                        .test_id("diagnostics-run")
                        .on_press(move || run_checks(run.clone())),
                )
                .child(
                    button(ButtonVariant::Outline)
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
        )
        .child(when(
            move || error.get().is_some(),
            move || {
                text(move || error.get().unwrap_or_default())
                    .text_color(move || nsh_base::theme().destructive)
            },
        ))
        .child(when(
            move || busy.get(),
            || text(|| t!("diagnostics.running")),
        ))
        .child(text(move || {
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
                .unwrap_or_else(|| t!("diagnostics.empty"))
        }))
        .child(
            scroll().flex_1().min_h(px(0.0)).child(
                view()
                    .flex_col()
                    .gap_3()
                    .child(when(
                        move || state.trace.get().is_some(),
                        move || {
                            let trace = state.trace.get();
                            surface(SurfaceLevel::Card)
                                .p_3()
                                .flex_col()
                                .gap_2()
                                .child(section_title(|| t!("diagnostics.local_title")))
                                .child(text(move || {
                                    trace
                                        .as_ref()
                                        .map(|r| {
                                            t!(
                                                "diagnostics.local_result",
                                                outbound = r
                                                    .outbound
                                                    .clone()
                                                    .unwrap_or_else(|| t!("diagnostics.no_match")),
                                                uncertain = r.uncertain_before.to_string()
                                            )
                                        })
                                        .unwrap_or_default()
                                }))
                                .child(text({
                                    let trace = state.trace;
                                    move || {
                                        trace
                                            .get()
                                            .map(|r| {
                                                format!(
                                                    "{} · {}",
                                                    r.target,
                                                    r.matched.unwrap_or_else(|| t!(
                                                        "diagnostics.no_match"
                                                    ))
                                                )
                                            })
                                            .unwrap_or_default()
                                    }
                                }))
                                .child(text(|| t!("diagnostics.local_hint")))
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
                                        let status_key = match step.status {
                                            DiagStatus::Ok => "diagnostics.ok",
                                            DiagStatus::Fail => "diagnostics.fail",
                                            DiagStatus::Skip => "diagnostics.skip",
                                            DiagStatus::Info => "diagnostics.info",
                                        };
                                        surface(SurfaceLevel::Card)
                                            .p_3()
                                            .flex_col()
                                            .gap_2()
                                            .child(text(move || {
                                                format!(
                                                    "{} · {} · {} ms",
                                                    t!(status_key),
                                                    step.name,
                                                    step.duration_ms
                                                )
                                            }))
                                            .child(text(step.summary))
                                            .child(text(step.detail))
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
