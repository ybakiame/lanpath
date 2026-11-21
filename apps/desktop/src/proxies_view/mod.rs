//! 运行时「策略与节点」工作台：展示运行中核心的出站策略组与节点、热切换节点并记忆、
//! 支持单节点与组内并发延迟测速，以及 Rule / Global / Direct 运行模式切换。

mod cards;
mod state;

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::Page;
use crate::app_state;
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::layout::{AlertTone, SurfaceLevel, alert_banner, page_header, surface};
use crate::ui::select::{segmented_group, segmented_radio};
use state::ProxiesState;

pub(crate) fn proxies_page(page: Signal<Page>) -> impl IntoElement {
    let state = ProxiesState::new();
    let initial = state.clone();
    let status = app_state::current().status;
    let last_running = signal(false);

    // 随共享运行态自动同步：核心启动或刚进页面且运行中时自动拉取策略组；停止时清空在线态。
    effect(move || {
        let running = status.get().running;
        if running && !last_running.get_untracked() {
            last_running.set(true);
            initial.refresh();
        } else if !running && last_running.get_untracked() {
            last_running.set(false);
            initial.clear_live();
        }
    });

    let mode = state.mode;
    let loading = state.loading;
    let live_ready = state.live_ready;
    let error = state.error;

    let mode_rule = state.clone();
    let mode_global = state.clone();
    let mode_direct = state.clone();
    let refresh_state = state.clone();
    let empty_state = state.clone();
    let live_state = state;

    scroll().flex_1().min_h(px(0.0)).child(
        view()
            .flex_col()
            .gap_5()
            .child(page_header(
                || t!("proxies.title"),
                || t!("proxies.subtitle"),
            ))
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(segmented_group(
                        "proxies-mode",
                        vec![
                            segmented_radio(
                                move || mode.get() == "rule",
                                || t!("proxies.mode.rule"),
                                "proxies-mode-rule",
                                move || mode_rule.set_mode("rule"),
                            ),
                            segmented_radio(
                                move || mode.get() == "global",
                                || t!("proxies.mode.global"),
                                "proxies-mode-global",
                                move || mode_global.set_mode("global"),
                            ),
                            segmented_radio(
                                move || mode.get() == "direct",
                                || t!("proxies.mode.direct"),
                                "proxies-mode-direct",
                                move || mode_direct.set_mode("direct"),
                            ),
                        ],
                    ))
                    .child(
                        view()
                            .flex_row()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .child(
                                button(ButtonVariant::Outline)
                                    .test_id("proxies-goto-outbounds")
                                    .size(ButtonSize::Md)
                                    .prefix(|| {
                                        icon_sized(Icon::Layers, Tone::Foreground, 14.0)
                                            .into_element()
                                    })
                                    .label(|| t!("proxies.configure_groups"))
                                    .on_press(move || page.set(Page::Overwrite)),
                            )
                            .child(
                                button(ButtonVariant::Secondary)
                                    .test_id("proxies-refresh")
                                    .size(ButtonSize::Md)
                                    .disabled(move || loading.get() || !status.get().running)
                                    .prefix(|| {
                                        icon_sized(Icon::Refresh, Tone::Foreground, 14.0)
                                            .into_element()
                                    })
                                    .label(move || {
                                        if loading.get() {
                                            t!("proxies.refreshing")
                                        } else {
                                            t!("proxies.refresh")
                                        }
                                    })
                                    .on_press(move || refresh_state.refresh()),
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
                move || !status.get().running || !live_ready.get(),
                move || {
                    let api_enabled = empty_state.clash_api_configured();
                    let is_running = status.get().running;
                    surface(SurfaceLevel::Card)
                        .p_6()
                        .flex_col()
                        .items_center()
                        .gap_3()
                        .child(icon_sized(Icon::Proxies, Tone::Muted, 30.0))
                        .child(
                            text(move || {
                                if !is_running {
                                    t!("proxies.empty_stopped")
                                } else if !api_enabled {
                                    t!("proxies.empty_api_disabled")
                                } else {
                                    t!("proxies.empty_loading")
                                }
                            })
                            .text_size(px(13.5))
                            .text_color(move || theme().muted_foreground),
                        )
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(
                                    button(ButtonVariant::Primary)
                                        .test_id("proxies-go-overview")
                                        .size(ButtonSize::Md)
                                        .label(|| t!("proxies.go_overview"))
                                        .on_press(move || page.set(Page::Overview)),
                                )
                                .child(
                                    button(ButtonVariant::Outline)
                                        .size(ButtonSize::Md)
                                        .label(|| t!("proxies.configure_groups"))
                                        .on_press(move || page.set(Page::Overwrite)),
                                ),
                        )
                },
            ))
            .child(when(
                move || status.get().running && live_ready.get(),
                move || cards::live_proxies_content(live_state.clone()),
            )),
    )
}
