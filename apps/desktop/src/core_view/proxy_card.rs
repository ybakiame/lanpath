//! 核心页“系统代理”分区渲染：接管开关（启用先弹一次性确认框；取消不
//! 写配置；确认保存偏好并授予进程内一次性接管授权；禁用清授权）、真实
//! ownership 展示（未接管/本应用持有/需要恢复/未知错误）与显式“恢复系
//! 统代理”操作（仅支持平台提供；不支持平台如实显示说明并隐藏恢复）。

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::app_state;
use crate::ui::icon::{Icon, Tone, icon};
use crate::ui::layout::{AlertTone, SurfaceLevel, alert_banner, section_title, surface};

use super::CorePageState;
use super::runtime::{
    cancel_enable_proxy, confirm_enable_proxy, format_proxy_ownership, spawn_restore_proxy,
    spawn_toggle_proxy,
};

pub(super) fn proxy_card(state: CorePageState) -> impl IntoElement {
    surface(SurfaceLevel::Card)
        .p_5()
        .flex_col()
        .gap_3()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .child(icon(Icon::Shield, Tone::Primary))
                .child(section_title(|| t!("product.proxy.title"))),
        )
        .child(
            // 支持性来自运行时真实状态（守护装配）：非 Windows 如实说明。
            when(
                move || !app_state::current().status.get().proxy_supported,
                move || {
                    alert_banner(
                        AlertTone::Info,
                        text(|| t!("product.proxy.unsupported"))
                            .text_size(px(12.5))
                            .text_color(move || theme().muted_foreground),
                    )
                },
            ),
        )
        .child(proxy_controls(state))
}

/// 接管控件：开关 + 状态 + 显式恢复（恢复仅支持平台提供）。
fn proxy_controls(state: CorePageState) -> impl IntoElement {
    let toggle_state = state.clone();
    let switch_display = state.clone();
    let restore_state = state;
    view()
        .flex_col()
        .gap_3()
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_4()
                .child(
                    view()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex_col()
                        .gap_1()
                        .child(
                            text(|| t!("product.proxy.preference"))
                                .text_size(px(13.5))
                                .font_weight(FontWeight::MEDIUM),
                        )
                        .child(
                            text(|| t!("product.proxy.preference_hint"))
                                .text_size(px(12.5))
                                .text_color(move || theme().muted_foreground),
                        ),
                )
                .child(view().test_id("proxy-toggle").flex_none().child(
                    crate::ui::select::switch_labeled(
                        move || switch_display.proxy_enabled.get(),
                        || t!("core.proxy.toggle_a11y").into(),
                        move |enabled| spawn_toggle_proxy(toggle_state.clone(), enabled),
                    ),
                )),
        )
        .child(when(
            move || {
                let shared = app_state::current();
                shared.command.get().busy || shared.status.get().operation.is_some()
            },
            || {
                alert_banner(
                    AlertTone::Info,
                    text(|| t!("core.proxy.change_busy"))
                        .test_id("proxy-change-busy")
                        .text_size(px(12.5))
                        .text_color(move || theme().muted_foreground),
                )
            },
        ))
        .child(when(
            move || app_state::current().status.get().proxy_supported,
            move || {
                let restore = restore_state.clone();
                view()
                    .flex_col()
                    .gap_3()
                    .child(crate::ui::separator())
                    .child(
                        text(move || {
                            format!(
                                "当前状态：{}",
                                format_proxy_ownership(
                                    &app_state::current().status.get().proxy_ownership
                                )
                            )
                        })
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM),
                    )
                    .child(
                        view()
                            .flex_row()
                            .flex_wrap()
                            .items_center()
                            .gap_3()
                            .child({
                                let press_state = restore.clone();
                                crate::ui::button(crate::ui::ButtonVariant::Outline)
                                    .test_id("proxy-restore")
                                    .size(crate::ui::ButtonSize::Sm)
                                    .disabled(move || {
                                        let shared = app_state::current();
                                        shared.command.get().busy
                                            || shared.status.get().operation.is_some()
                                    })
                                    .on_press(move || spawn_restore_proxy(press_state.clone()))
                                    .label(|| t!("product.action.restore_proxy"))
                            })
                            .child(
                                text(|| t!("product.proxy.restore_hint"))
                                    .text_size(px(12.5))
                                    .text_color(move || theme().muted_foreground),
                            ),
                    )
            },
        ))
}

/// 启用系统代理的一次性确认框：取消不写配置；确认才保存偏好并授权。
pub(super) fn proxy_confirm_dialog(state: CorePageState) -> AnyElement {
    let open = state.proxy_confirm;
    let cancel_state = state.clone();
    let accept_state = state;
    crate::ui::confirm_dialog(move || open.get())
        .test_id("proxy-confirm-dialog")
        .cancel_test_id("proxy-confirm-cancel")
        .confirm_test_id("proxy-confirm-accept")
        .title(|| t!("product.proxy.consent_title"))
        .description(|| t!("product.proxy.consent_hint"))
        .cancel_label(|| t!("product.action.cancel"))
        .confirm_label(|| t!("product.action.takeover"))
        .on_cancel(move || cancel_enable_proxy(cancel_state.clone()))
        .on_confirm(move || confirm_enable_proxy(accept_state.clone()))
        .into_element()
}
