//! 核心页“系统代理”分区渲染：接管开关（启用先弹一次性确认框；取消不
//! 写配置；确认保存偏好并授予进程内一次性接管授权；禁用清授权）、真实
//! ownership 展示（未接管/本应用持有/需要恢复/未知错误）与显式“恢复系
//! 统代理”操作（仅支持平台提供；不支持平台如实显示说明并隐藏恢复）。

use nsh_base::{button, dialog, theme};
use nsh_core::prelude::*;

use crate::app_state;

use super::CorePageState;
use super::runtime::{
    cancel_enable_proxy, confirm_enable_proxy, format_proxy_ownership, spawn_restore_proxy,
    spawn_toggle_proxy,
};

pub(super) fn proxy_card(state: CorePageState) -> impl IntoElement {
    view()
        .flex_col()
        .gap_2()
        .p_3()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(text("系统代理").text_size(px(15.0)))
        .child(
            // 支持性来自运行时真实状态（守护装配）：非 Windows 如实说明。
            when(
                move || !app_state::current().status.get().proxy_supported,
                move || {
                    text("系统代理接管仅支持 Windows；下方开关只保存偏好（本机不接管），恢复操作不可用")
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground)
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
        .gap_2()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    view().test_id("proxy-toggle").child(
                        crate::ui::select::switch_labeled(
                            move || switch_display.proxy_enabled.get(),
                            || t!("core.proxy.toggle_a11y").into(),
                            move |enabled| spawn_toggle_proxy(toggle_state.clone(), enabled),
                        ),
                    ),
                )
                .child(
                    text("核心运行时代理系统代理（经独立守护精确接管/恢复）")
                        .text_size(px(12.0)),
                ),
        )
        .child(
            text("开关只保存偏好；启用前需确认一次性接管授权（不保存）。运行中切换先安全停止（守护恢复）再按新偏好启动，绝不绕过守护直写注册表。TUN 仍不接管。")
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            text(move || {
                let shared = app_state::current();
                if shared.command.get().busy || shared.status.get().operation.is_some() {
                    t!("core.proxy.change_busy").to_string()
                } else {
                    String::new()
                }
            })
            .test_id("proxy-change-busy")
            .text_size(px(12.0))
            .text_color(move || theme().muted_foreground),
        )
        .child(when(
            move || app_state::current().status.get().proxy_supported,
            move || {
                let restore = restore_state.clone();
                view()
                    .flex_col()
                    .gap_2()
                    .child(text(move || {
                        format!(
                            "当前状态：{}",
                            format_proxy_ownership(&app_state::current().status.get().proxy_ownership)
                        )
                    }))
                    .child(
                        view()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .child({
                                let press_state = restore.clone();
                                button()
                                    .px_3()
                                    .py_1()
                                    .rounded(move || theme().radius)
                                    .border(px(1.0))
                                    .border_color(move || theme().border)
                                    .disabled(move || {
                                        let shared = app_state::current();
                                        shared.command.get().busy
                                            || shared.status.get().operation.is_some()
                                    })
                                    .disabled_style(|s| s.opacity(0.5))
                                    .on_press(move || spawn_restore_proxy(press_state.clone()))
                                    .child(
                                        view()
                                            .test_id("proxy-restore")
                                            .child(text("恢复系统代理").text_size(px(12.0))),
                                    )
                            })
                            .child(
                                text("只恢复本应用持有的记录；外来或未知变化的代理不会被覆盖。")
                                    .text_size(px(12.0))
                                    .text_color(move || theme().muted_foreground),
                            ),
                    )
            },
        ))
}

/// 启用系统代理的一次性确认框：取消不写配置；确认才保存偏好并授权。
pub(super) fn proxy_confirm_dialog(state: CorePageState) -> AnyElement {
    let on_cancel_state = state.clone();
    let cancel_state = state.clone();
    let accept_state = state;
    dialog()
        .on_cancel(move || cancel_enable_proxy(on_cancel_state.clone()))
        .child(
            view()
                .test_id("proxy-confirm-dialog")
                .flex_col()
                .gap_3()
                .w(px(460.0))
                .p_4()
                .rounded(move || theme().radius)
                .bg(move || theme().card)
                .border(px(1.0))
                .border_color(move || theme().border)
                .child(text("启用系统代理接管？"))
                .child(
                    text("核心启动时将请求把系统代理指向 127.0.0.1:mixed。若系统已存在其他代理，仅本次接管（显式授权），停止核心或退出应用时按原值精确归还，不会覆盖外来代理；若原本直连则照常接管。该授权只生效一次、不保存，重启应用后需重新确认。")
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                )
                .child(
                    view()
                        .flex_row()
                        .gap_2()
                        .justify_end()
                        .child(
                            button()
                                .px_3()
                                .py_1()
                                .rounded(move || theme().radius)
                                .border(px(1.0))
                                .border_color(move || theme().border)
                                .on_press(move || cancel_enable_proxy(cancel_state.clone()))
                                .child(view().test_id("proxy-confirm-cancel").child(text("取消"))),
                        )
                        .child(
                            button()
                                .px_3()
                                .py_1()
                                .rounded(move || theme().radius)
                                .bg(move || theme().primary)
                                .on_press(move || confirm_enable_proxy(accept_state.clone()))
                                .child(
                                    view()
                                        .test_id("proxy-confirm-accept")
                                        .child(text("确认接管").text_color(move || theme().primary_foreground)),
                                ),
                        ),
                )
                .into_element(),
        )
        .into_element()
}
