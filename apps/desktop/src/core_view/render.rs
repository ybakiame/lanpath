//! 核心页渲染：页面结构、选中摘要与删除确认弹窗。库存卡片拆分在
//! `render/inventory.rs`；状态与后台操作在 `super`（core_view.rs），本模块
//! 只读 Signal、把交互转发给 `spawn_*` 后台操作。操作反馈（notice/error/
//! stage）一律只显示版本，不重复完整路径。

mod inventory;

use inventory::inventory_card;
use nsh_base::theme;
use nsh_core::prelude::*;

use super::activity::spawn_inventory_activity_monitor;
use super::proxy_card::{proxy_card, proxy_confirm_dialog};
use super::runtime_card::runtime_card;
use super::{CorePageState, core_tag, spawn_delete};
use crate::app_state;
use crate::ui::icon::{Icon, Tone, icon};
use crate::ui::layout::{AlertTone, SurfaceLevel, alert_banner, section_title, surface};

pub(crate) fn core_page() -> impl IntoElement {
    let state = CorePageState::new();
    spawn_inventory_activity_monitor(state.clone());
    let inventory = state.clone();
    let error_state = state.clone();
    let proxy_state = state.clone();
    view()
        .flex_1()
        .flex_col()
        .min_h(px(0.0))
        .child(
            scroll().flex_1().child(
                view()
                    .flex_col()
                    .gap_5()
                    .w_full()
                    .child(crate::ui::page_header(
                        || t!("nav.core"),
                        || t!("core.subtitle"),
                    ))
                    .child(when(
                        move || error_state.error.get().is_some(),
                        move || {
                            let error = error_state.error;
                            alert_banner(
                                AlertTone::Danger,
                                text(move || error.get().unwrap_or_default())
                                    .text_size(px(13.0))
                                    .text_color(move || theme().destructive),
                            )
                        },
                    ))
                    .child(
                        view()
                            .gap_5()
                            .style_with(move |s| {
                                if window().width.0 < 1040.0 || font_scale() > 1.3 {
                                    s.flex_col()
                                } else {
                                    s.flex_row()
                                }
                            })
                            .child(selection_card())
                            .child(runtime_card()),
                    )
                    .child(proxy_card(proxy_state))
                    .child(inventory_card(inventory)),
            ),
        )
        .child(core_page_dialogs(state))
}

/// 核心页弹层（系统代理一次性确认、库存删除确认）：保持在滚动内容之外，
/// 无论滚动位置都覆盖在窗口内。
fn core_page_dialogs(state: CorePageState) -> impl IntoElement {
    let proxy_dialog_state = state.clone();
    let dialog_state = state;
    view()
        .child(proxy_confirm_dialog(proxy_dialog_state))
        .child(delete_dialog(dialog_state))
}

/// 实际选中卡片：独立于浏览类型；为空显示“尚未选择核心”。与概览/全局
/// 状态栏共用 App 级 `active_core` 信号；读取失败保留最后有效值并显示
/// 真实错误，不把损坏配置显示成“尚未选择”。
fn selection_card() -> impl IntoElement {
    let state = crate::app_state::current();
    let selection = state.active_core;
    let error = state.core_error;
    surface(SurfaceLevel::Card)
        .flex_1()
        .min_w(px(0.0))
        .p_5()
        .flex_col()
        .gap_3()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .child(icon(Icon::Core, Tone::Primary))
                .child(section_title(|| t!("product.core.selected"))),
        )
        .child(
            text(move || {
                selection
                    .get()
                    .map(|selection| app_state::core_selection_line(&selection))
                    .unwrap_or_else(|| "尚未选择核心".to_owned())
            })
            .text_size(px(16.0))
            .font_weight(FontWeight::SEMIBOLD)
            .selectable(true),
        )
        .child(when(
            move || error.get().is_some(),
            move || {
                alert_banner(
                    AlertTone::Danger,
                    text(move || {
                        error
                            .get()
                            .map(|error| app_state::core_error_line(&error))
                            .unwrap_or_default()
                    })
                    .test_id("core-selection-error")
                    .text_size(px(12.5))
                    .text_color(move || theme().destructive),
                )
            },
        ))
}

/// 删除确认弹窗：取消只关弹窗；确认后后台删除并刷新列表。
fn delete_dialog(state: CorePageState) -> AnyElement {
    let open = state.pending_delete;
    let title_state = state.pending_delete;
    let cancel_state = state.clone();
    let confirm_state = state;
    crate::ui::confirm_dialog(move || open.get().is_some())
        .destructive()
        .title(move || {
            title_state
                .get()
                .map(|(version, _)| format!("删除核心 {}？", core_tag(&version)))
                .unwrap_or_default()
        })
        .description(|| t!("product.core.delete_hint"))
        .confirm_label(|| t!("product.action.confirm_delete"))
        .cancel_label(|| t!("product.action.cancel"))
        .on_cancel(move || cancel_state.pending_delete.set(None))
        .on_confirm(move || {
            if let Some((version, path)) = confirm_state.pending_delete.get() {
                confirm_state.pending_delete.set(None);
                spawn_delete(confirm_state.clone(), version, path);
            }
        })
        .into_element()
}
