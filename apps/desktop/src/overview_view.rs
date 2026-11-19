//! 概览页（阶段 1，DESIGN.md 2.2/9）：主路径首页。围绕
//! 当前订阅 → 核心 → 校验/启动 → 真实状态 组织；显示真实当前生效订阅、
//! 实际选中的核心类型/版本、真实 CoreRuntime 状态与可执行下一步。
//! 未配置时按“添加订阅 → 刷新节点 → 选择核心 → 校验并启动”列出步骤，
//! 只高亮当前可执行的一步。不展示尚未实现的流量/节点出口，不做假进度。
//! 运行状态与启动/停止命令复用 App 级 `app_state` 共享状态（与核心页、
//! 全局状态栏同一来源）；系统代理一次性确认保留核心页流程，这里只引导。

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::Page;
use crate::app_state::{self, AppState};
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::layout::{SurfaceLevel, page_header, section_title, surface};

/// 概览页：默认首页；`page` 用于“管理订阅/选择核心”等操作导航到现有页。
pub(crate) fn overview_page(page: Signal<Page>) -> impl IntoElement {
    let state = app_state::current();
    let summary_cond = state.clone();
    let summary_body = state.clone();
    let steps_cond = state.clone();
    let proxy_state = state.clone();
    let steps_body = state;
    view()
        .flex_1()
        .flex_col()
        .gap_4()
        .child(page_header(
            || t!("overview.title"),
            || t!("overview.subtitle"),
        ))
        .child(summary_problems(summary_body.clone(), page))
        .child(proxy_summary(proxy_state, page))
        .child(when(
            move || configured(&summary_cond),
            move || configured_summary(summary_body.clone(), page),
        ))
        .child(when(
            move || !configured(&steps_cond),
            move || setup_steps(steps_body.clone(), page),
        ))
}

/// 摘要读取失败：保留最后有效值，单独显示真实错误和可执行入口。
fn summary_problems(state: AppState, page: Signal<Page>) -> impl IntoElement {
    let subscription_error = state.subscription_error;
    let core_error = state.core_error;
    let subscriptions_page = page;
    let core_page = page;
    when(
        move || subscription_error.get().is_some() || core_error.get().is_some(),
        move || {
            view()
                .flex_col()
                .gap_2()
                .p_3()
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().destructive)
                .child(
                    text(|| t!("overview.summary.problem_title"))
                        .text_size(px(14.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(move || theme().destructive),
                )
                .child(
                    text(move || {
                        subscription_error
                            .get()
                            .map(|error| app_state::subscription_error_line(&error))
                            .unwrap_or_default()
                    })
                    .test_id("overview-subscription-error")
                    .text_size(px(12.0))
                    .text_color(move || theme().destructive),
                )
                .child(
                    text(move || {
                        core_error
                            .get()
                            .map(|error| app_state::core_error_line(&error))
                            .unwrap_or_default()
                    })
                    .test_id("overview-core-error")
                    .text_size(px(12.0))
                    .text_color(move || theme().destructive),
                )
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
                )
        },
    )
}

/// 已配置：存在带节点的生效订阅且已选择核心（响应式读取共享信号）。
fn configured(state: &AppState) -> bool {
    matches!(
        state.active_subscription.get(),
        Some(ref subscription) if subscription.node_count > 0
    ) && state.active_core.get().is_some()
}

/// 已配置主路径：当前订阅 / 核心 / 运行状态三张真实摘要卡。
fn configured_summary(state: AppState, page: Signal<Page>) -> impl IntoElement {
    view()
        .flex_col()
        .gap_3()
        .child(subscription_card(state.clone(), page))
        .child(core_card(state.clone(), page))
        .child(runtime_card(state))
}

/// 当前订阅卡：真实生效订阅（名称/节点/格式）；管理操作导航到订阅页
/// （不复制订阅 CRUD）。
fn subscription_card(state: AppState, page: Signal<Page>) -> impl IntoElement {
    let summary = state.active_subscription;
    let manage_page = page;
    surface(SurfaceLevel::Card)
        .flex_col()
        .gap_2()
        .p_3()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child(text(|| t!("overview.subscription.title")).text_size(px(15.0)))
                .child(view().flex_1())
                .child(
                    button(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .test_id("overview-manage-subscriptions")
                        .label(|| t!("overview.subscription.manage"))
                        .on_press(move || manage_page.set(Page::Subscription)),
                ),
        )
        .child(text(move || {
            let Some(subscription) = summary.get() else {
                return t!("overview.subscription.none");
            };
            app_state::subscription_summary_line(&subscription)
        }))
}

/// 核心卡：实际选中的核心类型/版本（与全局状态栏同一信号）；选择操作
/// 导航到核心页（不复制库存/确认流程）。
fn core_card(state: AppState, page: Signal<Page>) -> impl IntoElement {
    let selection = state.active_core;
    let choose_page = page;
    surface(SurfaceLevel::Card)
        .flex_col()
        .gap_2()
        .p_3()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child(text(|| t!("overview.core.title")).text_size(px(15.0)))
                .child(view().flex_1())
                .child(
                    button(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .test_id("overview-choose-core")
                        .label(|| t!("overview.core.choose"))
                        .on_press(move || choose_page.set(Page::Core)),
                ),
        )
        .child(text(move || {
            selection
                .get()
                .map(|selection| {
                    format!(
                        "{} {}",
                        selection.core_type,
                        app_state::core_tag(&selection.version)
                    )
                })
                .unwrap_or_else(|| t!("overview.core.none"))
        }))
}

/// 运行状态卡：真实 CoreRuntime 状态与可执行下一步（校验并启动/停止与
/// 核心页是同一命令）；系统代理四轴状态与恢复入口就近展示，不藏到仅核心页。
fn runtime_card(state: AppState) -> impl IntoElement {
    let status = state.status;
    let command = state.command;
    view()
        .flex_col()
        .gap_2()
        .p_3()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(text(|| t!("overview.runtime.title")).text_size(px(15.0)))
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
                .child(start_button(state.clone()))
                .child(stop_button(state.clone())),
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

/// 系统代理摘要：偏好与实际 ownership 分开；仅确认需要恢复时提供同命令
/// 恢复按钮，未知查询错误只提示前往核心页检查。
fn proxy_summary(state: AppState, page: Signal<Page>) -> impl IntoElement {
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
                .gap_2()
                .p_3()
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().border)
                .child(
                    text(move || {
                        t!(
                            "overview.proxy.state",
                            preference = app_state::proxy_preference_line(preference_line.get()),
                            ownership =
                                app_state::proxy_ownership_line(&status_line.get().proxy_ownership)
                        )
                    })
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground),
                )
                .child(when(
                    move || app_state::proxy_recovery_visible(&status.get()),
                    move || {
                        let restore = restore_state.clone();
                        let restore_command = command_state;
                        view()
                            .flex_col()
                            .gap_2()
                            .child(
                                text(|| t!("overview.proxy.recovery_warning"))
                                    .text_size(px(12.0))
                                    .text_color(move || theme().destructive),
                            )
                            .child(
                                button(ButtonVariant::Primary)
                                    .size(ButtonSize::Sm)
                                    .test_id("overview-proxy-recover")
                                    .label(|| t!("overview.proxy.recover"))
                                    .disabled(move || {
                                        let command = restore_command.get();
                                        command.busy
                                    })
                                    .on_press(move || {
                                        let _ = app_state::spawn_restore_proxy(&restore);
                                    }),
                            )
                    },
                ))
                .child(
                    button(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .test_id("overview-go-core-proxy")
                        .label(|| t!("overview.proxy.go_core"))
                        .on_press(move || core_page.set(Page::Core)),
                )
        },
    )
}

/// 全局启动：与核心页同一共享命令（真实 CoreRuntime 启停，任务跨页存活）。
fn start_button(state: AppState) -> AnyElement {
    let status = state.status;
    let command = state.command;
    button(ButtonVariant::Primary)
        .size(ButtonSize::Sm)
        .test_id("overview-start")
        .label(|| t!("overview.runtime.start"))
        .disabled(move || {
            let status = status.get();
            status.running || status.operation.is_some() || command.get().busy
        })
        .on_press(move || app_state::spawn_start(&state))
        .into_element()
}

/// 全局停止：仅在真实运行中可用。
fn stop_button(state: AppState) -> AnyElement {
    let status = state.status;
    let command = state.command;
    button(ButtonVariant::Outline)
        .size(ButtonSize::Sm)
        .test_id("overview-stop")
        .label(|| t!("overview.runtime.stop"))
        .disabled(move || {
            let status = status.get();
            !status.running || status.operation.is_some() || command.get().busy
        })
        .on_press(move || app_state::spawn_stop(&state))
        .into_element()
}

/// 未配置步骤：按顺序列出 添加订阅 → 刷新节点 → 选择核心 → 校验并启动，
/// 只高亮当前可执行的一步；完成的步骤如实标记，后续步骤只读提示。
/// 当前步骤随共享订阅/核心信号响应式推进。
fn setup_steps(state: AppState, page: Signal<Page>) -> impl IntoElement {
    let subscription = state.active_subscription;
    let selection = state.active_core;
    let steps = [
        ("overview.steps.add", StepAction::Subscriptions),
        ("overview.steps.refresh", StepAction::Subscriptions),
        ("overview.steps.core", StepAction::Core),
        ("overview.steps.start", StepAction::Start),
    ];
    view()
        .flex_col()
        .gap_2()
        .child(section_title(|| t!("overview.steps.title")))
        .children(
            steps
                .into_iter()
                .enumerate()
                .map(move |(index, (key, action))| {
                    let subscription = subscription;
                    let selection = selection;
                    let state = state.clone();
                    let page = page;
                    // 当前可执行步骤：无生效订阅 → 添加订阅；有订阅但无节点 → 刷新
                    // 节点；有节点但无核心 → 选择核心（响应式，随真实配置推进）。
                    let current = move || match (subscription.get(), selection.get()) {
                        (None, _) => 0,
                        (Some(subscription), _) if subscription.node_count == 0 => 1,
                        (Some(_), None) => 2,
                        (Some(_), Some(_)) => 3,
                    };
                    step_row(
                        move || t!(key),
                        move || current() == index,
                        move || index < current(),
                        action,
                        page,
                        state,
                    )
                }),
        )
}

/// 步骤动作：导航到现有真实页面，或最后一步直接复用共享启动命令。
#[derive(Clone, Copy)]
enum StepAction {
    Subscriptions,
    Core,
    Start,
}

/// 单个步骤行：完成的弱化并标记；当前可执行的高亮并给出真实动作；
/// 后续步骤只读。
fn step_row(
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
                .size(ButtonSize::Sm)
                .test_id("overview-step-action")
                .label(|| t!("overview.steps.go_subscriptions"))
                .on_press(move || page.set(Page::Subscription))
                .into_element(),
            StepAction::Core => button(ButtonVariant::Primary)
                .size(ButtonSize::Sm)
                .test_id("overview-step-action")
                .label(|| t!("overview.steps.go_core"))
                .on_press(move || page.set(Page::Core))
                .into_element(),
            StepAction::Start => button(ButtonVariant::Primary)
                .size(ButtonSize::Sm)
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
    view()
        .flex_row()
        .items_center()
        .gap_3()
        .py_1()
        .child({
            let actionable = actionable.clone();
            let done = done.clone();
            text(move || {
                if done() {
                    format!("✓ {}", label())
                } else {
                    label()
                }
            })
            .text_size(px(14.0))
            .text_color(move || {
                if actionable() {
                    theme().foreground
                } else {
                    theme().muted_foreground
                }
            })
        })
        .child(view().flex_1())
        .child(when(
            move || done(),
            || {
                text(|| t!("overview.steps.done"))
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground)
            },
        ))
        .child(when(move || actionable(), move || action_element(&state)))
}
