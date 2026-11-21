//! 循径概览：配置依赖、运行会话、代理归属分别呈现，共用唯一 AppState。
mod alerts;
mod analytics;
mod runtime;
mod setup;
mod shortcuts;
mod visuals;

use crate::{
    Page,
    app_state::{self, AppState},
    ui,
};
use nsh_base::theme;
use nsh_core::prelude::*;
use ui::icon::{Icon, Tone, icon, icon_sized};
use ui::{ButtonSize, ButtonVariant, SurfaceLevel, button, page_header, section_title, surface};

pub(crate) fn overview_page(page: Signal<Page>) -> AnyElement {
    let state = app_state::current();
    let configured_state = state.clone();
    let unconfigured = state.clone();
    let hero = state.clone();
    scroll()
        .flex_1()
        .min_h(px(0.0))
        .child(
            view()
                .flex_col()
                .gap_5()
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_end()
                        .justify_between()
                        .gap_4()
                        .child(page_header(
                            || t!("overview.title"),
                            || t!("overview.subtitle"),
                        ))
                        .child(quick_actions(page)),
                )
                .child(alerts::summary_problems(state.clone(), page))
                .child(when(
                    move || {
                        configured(&configured_state)
                            || configured_state.status.get().running
                            || configured_state.command.get().busy
                    },
                    move || workspace(hero.clone(), page),
                ))
                .child(when(
                    move || {
                        !configured(&unconfigured)
                            && !unconfigured.status.get().running
                            && !unconfigured.command.get().busy
                    },
                    move || setup::setup_steps(state.clone(), page),
                )),
        )
        .into_element()
}

fn quick_actions(page: Signal<Page>) -> impl IntoElement {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_2()
        .child(
            button(ButtonVariant::Outline)
                .size(ButtonSize::Sm)
                .test_id("nav-subscriptions")
                .prefix(|| icon_sized(Icon::Subscription, Tone::Muted, 14.0))
                .label(|| t!("nav.subscriptions"))
                .on_press(move || page.set(Page::Subscription)),
        )
        .child(
            button(ButtonVariant::Outline)
                .size(ButtonSize::Sm)
                .test_id("nav-profiles")
                .prefix(|| icon_sized(Icon::Code, Tone::Muted, 14.0))
                .label(|| t!("profile.editor"))
                .on_press(move || page.set(Page::Profiles)),
        )
        .child(
            button(ButtonVariant::Outline)
                .size(ButtonSize::Sm)
                .test_id("nav-config")
                .prefix(|| icon_sized(Icon::File, Tone::Muted, 14.0))
                .label(|| t!("nav.config"))
                .disabled(move || {
                    !app_state::current()
                        .active_subscription
                        .get()
                        .is_some_and(|s| s.enabled)
                })
                .on_press(move || page.set(Page::Config)),
        )
}

fn configured(state: &AppState) -> bool {
    state
        .active_subscription
        .get()
        .is_some_and(|s| s.enabled && s.node_count > 0)
        && state.active_core.get().is_some()
}

fn workspace(state: AppState, page: Signal<Page>) -> AnyElement {
    let runtime = state.clone();
    let status = state.status;
    view()
        .flex_col()
        .gap_5()
        .child(
            surface(SurfaceLevel::Card)
                .relative()
                .overflow_hidden()
                .flex_col()
                .child(visuals::hero_aura_shader(move || status.get().running))
                .child(
                    view()
                        .p_5()
                        .flex_col()
                        .gap_5()
                        .child(runtime::header(runtime))
                        .child(configuration_path(state.clone(), page))
                        .child(alerts::proxy_summary(state.clone(), page)),
                )
                .child(session_facts(state.clone())),
        )
        .child(analytics::dashboard(state, page))
        .into_element()
}

fn configuration_path(state: AppState, page: Signal<Page>) -> AnyElement {
    let subscription = state.active_subscription;
    let core = state.active_core;
    let status = state.status;
    view()
        .test_id("overview-configuration-path")
        .gap_3()
        .items_center()
        .style_with(move |s| {
            if window().width.0 < 1040.0 || font_scale() > 1.3 {
                s.flex_col().items_stretch()
            } else {
                s.flex_row().items_stretch()
            }
        })
        .child(path_node(
            Icon::Subscription,
            "overview.subscription.title",
            move || {
                subscription
                    .get()
                    .map(|s| app_state::subscription_summary_line(&s))
                    .unwrap_or_else(|| t!("overview.subscription.none"))
            },
            button(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .test_id("overview-manage-subscriptions")
                .label(|| t!("overview.subscription.manage"))
                .on_press(move || page.set(Page::Subscription))
                .into_element(),
        ))
        .child(connector())
        .child(path_node(
            Icon::Core,
            "overview.core.title",
            move || {
                core.get()
                    .map(|c| format!("{} {}", c.core_type, app_state::core_tag(&c.version)))
                    .unwrap_or_else(|| t!("overview.core.none"))
            },
            button(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .test_id("overview-choose-core")
                .label(|| t!("overview.core.choose"))
                .on_press(move || page.set(Page::Core))
                .into_element(),
        ))
        .child(connector())
        .child(path_node(
            Icon::Port,
            "overview.local_proxy",
            move || {
                status
                    .get()
                    .session
                    .map(|s| format!("127.0.0.1:{}", s.mixed_port))
                    .unwrap_or_else(|| t!("overview.await_start"))
            },
            text(|| t!("overview.path_hint"))
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground)
                .into_element(),
        ))
        .into_element()
}

fn path_node(
    kind: Icon,
    key: &'static str,
    value: impl Fn() -> String + 'static,
    action: AnyElement,
) -> AnyElement {
    view()
        .flex_1()
        .min_w(px(0.0))
        .flex_col()
        .justify_between()
        .gap_2()
        .p_3()
        .rounded(move || theme().radius)
        .bg(move || theme().muted.opacity(0.45))
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(
            view()
                .flex_col()
                .gap_2()
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            view()
                                .size(px(24.0))
                                .rounded(px(6.0))
                                .bg(move || theme().accent)
                                .items_center()
                                .justify_center()
                                .child(icon_sized(kind, Tone::Primary, 14.0)),
                        )
                        .child(
                            text(move || t!(key))
                                .text_size(px(12.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(move || theme().muted_foreground),
                        ),
                )
                .child(
                    text(value)
                        .text_size(px(14.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .selectable(true),
                ),
        )
        .child(view().flex_row().items_center().child(action))
        .into_element()
}

fn connector() -> AnyElement {
    view()
        .items_center()
        .justify_center()
        .flex_none()
        .child(icon_sized(Icon::Arrow, Tone::Muted, 14.0))
        .into_element()
}

fn session_facts(state: AppState) -> AnyElement {
    let status = state.status;
    view()
        .px_5()
        .py_3()
        .bg(ui::prefs::chrome_bg)
        .border_t(px(1.0))
        .border_color(move || theme().border)
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_6()
        .child(
            text(|| t!("overview.session"))
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || theme().muted_foreground),
        )
        .child(fact_pair(
            || t!("overview.core.title"),
            move || {
                status
                    .get()
                    .session
                    .map(|s| format!("{} {}", s.core_type, app_state::core_tag(&s.version)))
                    .unwrap_or_else(|| t!("runtime.status.stopped"))
            },
        ))
        .child(fact_pair(
            || t!("overview.control_api"),
            move || {
                status
                    .get()
                    .session
                    .and_then(|s| s.clash_api_port)
                    .map(|p| format!("127.0.0.1:{p}"))
                    .unwrap_or_else(|| t!("runtime.api_off"))
            },
        ))
        .into_element()
}

fn fact_pair(
    label: impl Fn() -> String + 'static,
    value: impl Fn() -> String + 'static,
) -> AnyElement {
    view()
        .flex_row()
        .items_center()
        .gap_2()
        .child(
            text(label)
                .text_size(px(12.5))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            text(value)
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .selectable(true),
        )
        .into_element()
}
