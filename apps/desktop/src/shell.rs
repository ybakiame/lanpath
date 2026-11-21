//! Windows 工作区外壳；内容与控件按循径规范组织。
//! 侧栏收缩按钮位于右侧标题栏（参考官方 NSH Gallery），侧栏展开时简洁呈现分组与导航项，
//! 收起时彻底释放水平空间，应用图标与恢复入口常驻标题栏。

use crate::{Page, app_state, ui};
use nsh_base::theme;
use nsh_core::prelude::*;
use ui::icon::{Icon, Tone, icon_sized};

pub(crate) const SIDEBAR_WIDTH: f32 = 216.0;

/// 全局状态栏：左侧实时运行指示点与状态，右侧生效订阅与所选核心。
pub(crate) fn global_status_bar() -> impl IntoElement {
    let state = app_state::current();
    let status = state.status;
    let command = state.command;
    let subscription = state.active_subscription;
    let core = state.active_core;
    let subscription_error = state.subscription_error;
    let core_error = state.core_error;
    let dot_status = status;
    let dot_command = command;
    ui::feedback::status_bar(
        view()
            .flex_1()
            .min_w(px(0.0))
            .flex_row()
            .items_center()
            .gap_2()
            .child(view().size(px(7.0)).flex_none().rounded_full().bg(move || {
                let st = dot_status.get();
                let cmd = dot_command.get();
                if app_state::proxy_recovery_visible(&st)
                    || st.exit_error.is_some()
                    || cmd.error.is_some()
                {
                    theme().destructive
                } else if cmd.busy {
                    theme().color("warning").unwrap_or(theme().primary)
                } else if st.running {
                    theme().color("success").unwrap_or(theme().primary)
                } else {
                    theme().muted_foreground.opacity(0.6)
                }
            }))
            .child(
                text(move || {
                    let status = status.get();
                    let command = command.get();
                    let mut line = if command.busy {
                        app_state::command_pending_line(&command, &status)
                    } else {
                        app_state::runtime_status_line(&status)
                    };
                    if app_state::proxy_recovery_visible(&status) {
                        line.push_str(&t!("status.proxy_recovery_short"));
                    }
                    if subscription_error.get().is_some() || core_error.get().is_some() {
                        line.push_str(&app_state::summary_error_short_line());
                    }
                    line
                })
                .flex_1()
                .min_w(px(0.0))
                .text_color(move || theme().foreground)
                .truncate(),
            ),
        text(move || {
            let subscription = subscription
                .get()
                .map(|subscription| subscription.name)
                .unwrap_or_else(|| t!("status.none"));
            let core = core
                .get()
                .map(|selection| {
                    format!(
                        "{} {}",
                        selection.core_type,
                        app_state::core_tag(&selection.version)
                    )
                })
                .unwrap_or_else(|| t!("status.none"));
            t!("status.summary", subscription = subscription, core = core)
        })
        .text_color(move || theme().muted_foreground)
        .max_w(move || px(window().width.0 * 0.5))
        .truncate(),
    )
}

/// 启动失败全窗提示：数据根打不开时不进入任何业务页，真实错误与处置建议直接上屏。
pub(crate) fn startup_failure(error: Signal<Option<String>>) -> impl IntoElement {
    view()
        .flex_1()
        .flex_col()
        .gap_3()
        .p_6()
        .bg(ui::prefs::window_bg)
        .text_color(move || theme().foreground)
        .child(
            text("Lanpath NSH Desktop")
                .text_size(px(24.0))
                .font_weight(FontWeight::SEMIBOLD),
        )
        .child(text(|| t!("product.startup.title")).text_color(move || theme().destructive))
        .child(text(move || error.get().unwrap_or_default()))
        .child(text(|| t!("product.startup.hint")).text_color(move || theme().muted_foreground))
}

fn page_icon(page: Page) -> Icon {
    match page {
        Page::Overview => Icon::Overview,
        Page::Subscription => Icon::Subscription,
        Page::Core => Icon::Core,
        Page::Overwrite => Icon::Layers,
        Page::Rules => Icon::Rules,
        Page::RuleSets => Icon::Shield,
        Page::Profiles => Icon::Code,
        Page::RequestSettings => Icon::Sliders,
        Page::Config => Icon::File,
        Page::Diagnostics => Icon::Activity,
        Page::Logs => Icon::Logs,
        Page::Backup => Icon::Backup,
        Page::Appearance => Icon::Appearance,
    }
}

#[derive(Clone, Copy)]
struct Sidebar {
    shown: Signal<bool>,
}
impl Default for Sidebar {
    fn default() -> Self {
        Self {
            shown: signal(true),
        }
    }
}

pub(crate) fn sidebar_shown() -> bool {
    global::<Sidebar>().get().shown.get()
}

pub(crate) fn sidebar_width() -> f32 {
    if sidebar_shown() { SIDEBAR_WIDTH } else { 0.0 }
}

/// 供标题栏右侧内容区调用的侧栏展开/收起切换按钮。
pub(crate) fn sidebar_toggle_button() -> AnyElement {
    let shown = global::<Sidebar>().get().shown;
    crate::ui::icon_button()
        .tool_size()
        .test_id("sidebar-toggle")
        .accessibility_label(move || {
            if shown.get() {
                t!("sidebar.hide")
            } else {
                t!("sidebar.show")
            }
            .into()
        })
        .child(icon_sized(Icon::Sidebar, Tone::Muted, 16.0))
        .on_press(move || shown.set(!shown.get()))
        .into_element()
}

pub(crate) fn sidebar(active: Signal<Page>, requested: Signal<Page>) -> AnyElement {
    let shown = global::<Sidebar>().get().shown;
    when(
        move || shown.get(),
        move || expanded_sidebar(active, requested),
    )
    .into_element()
}

fn expanded_sidebar(active: Signal<Page>, requested: Signal<Page>) -> AnyElement {
    let positions = signal(std::collections::BTreeMap::<usize, (f32, f32)>::new());
    let selection = signal((0.0_f32, 36.0_f32));
    effect(move || {
        if let Some(bounds) =
            positions.with(|positions| positions.get(&(active.get() as usize)).copied())
            && selection.get_untracked() != bounds
        {
            selection.set(bounds);
        }
    });
    let reduced = ui::prefs::prefs().reduced;
    view()
        .w(px(SIDEBAR_WIDTH))
        .flex_none()
        .h_full()
        .min_h(px(0.0))
        .flex_col()
        .test_id("app-sidebar")
        .bg(ui::prefs::sidebar_bg)
        .border_r(px(1.0))
        .border_color(move || theme().sidebar_border)
        .child(
            scroll().flex_1().min_h(px(0.0)).child(
                view()
                    .w_full()
                    .relative()
                    .flex_col()
                    .px_3()
                    .py_2()
                    .gap(px(2.0))
                    .child(when(
                        move || active.get() != Page::Appearance,
                        move || {
                            view()
                                .test_id("sidebar-selection")
                                .absolute()
                                .left(px(12.0))
                                .right(px(12.0))
                                .top(px(0.0))
                                .h(move || px(selection.get().1))
                                .translate_y(move || px(selection.get().0))
                                .rounded(move || theme().radius)
                                .bg(move || theme().sidebar_accent)
                                .transition_transform()
                                .duration(move || {
                                    ui::motion::motion_duration(ui::motion::BASE_MS, reduced.get())
                                })
                                .accessibility_elements_hidden(true)
                        },
                    ))
                    .child(group("nav.group.workspace", true))
                    .child(nav_item(active, requested, positions, Page::Overview))
                    .child(group("nav.group.core_config", false))
                    .child(nav_item(active, requested, positions, Page::Core))
                    .child(nav_item(active, requested, positions, Page::Overwrite))
                    .child(nav_item(active, requested, positions, Page::Rules))
                    .child(nav_item(active, requested, positions, Page::RuleSets))
                    .child(nav_item(
                        active,
                        requested,
                        positions,
                        Page::RequestSettings,
                    ))
                    .child(group("nav.group.tools", false))
                    .child(nav_item(active, requested, positions, Page::Diagnostics))
                    .child(nav_item(active, requested, positions, Page::Logs))
                    .child(nav_item(active, requested, positions, Page::Backup)),
            ),
        )
        .child(
            view()
                .flex_none()
                .p_3()
                .border_t(px(1.0))
                .border_color(move || theme().sidebar_border)
                .child(nav_item(active, requested, positions, Page::Appearance)),
        )
        .into_element()
}

fn group(key: &'static str, first: bool) -> AnyElement {
    text(move || t!(key))
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(move || theme().muted_foreground)
        .px_3()
        .pt(px(if first { 6.0 } else { 14.0 }))
        .pb_1()
        .into_element()
}

fn nav_item(
    active: Signal<Page>,
    requested: Signal<Page>,
    positions: Signal<std::collections::BTreeMap<usize, (f32, f32)>>,
    target: Page,
) -> AnyElement {
    let selected = move || active.get() == target;
    let reduced = ui::prefs::prefs().reduced;
    let icon_tone = move || {
        if selected() {
            Tone::Primary
        } else {
            Tone::Muted
        }
    };
    let item = nsh_base::button()
        .selected(selected)
        .w_full()
        .min_h(px(34.0))
        .flex_row()
        .items_center()
        .gap_3()
        .px_3()
        .py(px(6.0))
        .rounded(move || theme().radius)
        .cursor(Cursor::Default)
        .transition_colors()
        .duration(move || ui::motion::motion_duration(ui::motion::INSTANT_MS, reduced.get()))
        .bg(move || {
            if selected() && target == Page::Appearance {
                theme().sidebar_accent
            } else {
                transparent_black().into()
            }
        })
        .text_color(move || {
            if selected() {
                theme().sidebar_accent_foreground
            } else {
                theme().sidebar_foreground
            }
        })
        .hover(|s| {
            s.bg(move || {
                if selected() {
                    theme().sidebar_accent
                } else {
                    theme().muted.opacity(0.75)
                }
            })
        })
        .active(|s| s.bg(move || theme().accent))
        .focus_visible(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .on_press(move || requested.set(target))
        .child(
            when(selected, move || {
                icon_sized(page_icon(target), icon_tone(), 16.0)
            })
            .otherwise(move || icon_sized(page_icon(target), Tone::Muted, 16.0)),
        )
        .child(
            text(move || t!(target.label_key()))
                .text_size(px(13.5))
                .style_when(selected, |s| s.font_weight(FontWeight::MEDIUM)),
        );
    view()
        .test_id(target.test_id())
        .on_layout(move |bounds: &Bounds<Points>| {
            let next = (bounds.origin.y.0, bounds.size.height.0);
            if positions.with_untracked(|p| p.get(&(target as usize)).copied()) != Some(next) {
                positions.update(|p| {
                    p.insert(target as usize, next);
                });
            }
        })
        .child(item)
        .into_element()
}
