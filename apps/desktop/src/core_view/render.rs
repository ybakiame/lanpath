//! 核心页渲染：页面结构、选中摘要与删除确认弹窗。库存卡片拆分在
//! `render/inventory.rs`；状态与后台操作在 `super`（core_view.rs），本模块
//! 只读 Signal、把交互转发给 `spawn_*` 后台操作。操作反馈（notice/error/
//! stage）一律只显示版本，不重复完整路径。

mod inventory;

use inventory::inventory_card;
use nsh_base::{button, dialog, theme};
use nsh_core::prelude::*;

use super::activity::spawn_inventory_activity_monitor;
use super::proxy_card::{proxy_card, proxy_confirm_dialog};
use super::runtime_card::runtime_card;
use super::{CorePageState, core_tag, spawn_delete};
use crate::app_state;

pub(crate) fn core_page() -> impl IntoElement {
    let state = CorePageState::new();
    spawn_inventory_activity_monitor(state.clone());
    let inventory = state.clone();
    let notice_state = state.clone();
    let error_state = state.clone();
    let proxy_state = state.clone();
    view()
        .flex_1()
        .flex_col()
        // 页面高于视口时内容区滚动；
        // 全局状态栏占高后卡片 + 库存列表仍完整可达。
        .child(
            scroll().flex_1().child(
                view()
                    .flex_col()
                    .gap_4()
                    .w_full()
                    .child(text("核心").text_size(px(24.0)))
                    .child(
                        text("管理本数据根 cores/ 下的核心文件 · 校验并启动为真实二进制校验后的用户态 loopback 运行；系统代理经独立守护精确接管/恢复（见下方分区），TUN/VPN 不接管")
                            .text_color(move || theme().muted_foreground),
                    )
                    // 操作反馈固定在页首（副标题下）：页面高于视口时结果
                    // 也不被挤出可视区，与全局状态栏共存时同样立即可见。
                    .child(
                        text(move || notice_state.notice.get().unwrap_or_default())
                            .text_color(move || theme().muted_foreground),
                    )
                    .child(
                        text(move || error_state.error.get().unwrap_or_default())
                            .text_color(move || theme().destructive),
                    )
                    .child(selection_card())
                    .child(runtime_card())
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
        .child(when(
            move || proxy_dialog_state.proxy_confirm.get(),
            move || proxy_confirm_dialog(proxy_dialog_state.clone()),
        ))
        .child(when(
            move || dialog_state.pending_delete.get().is_some(),
            move || delete_dialog(dialog_state.clone()),
        ))
}

/// 实际选中卡片：独立于浏览类型；为空显示“尚未选择核心”。与概览/全局
/// 状态栏共用 App 级 `active_core` 信号；读取失败保留最后有效值并显示
/// 真实错误，不把损坏配置显示成“尚未选择”。
fn selection_card() -> impl IntoElement {
    let state = crate::app_state::current();
    let selection = state.active_core;
    let error = state.core_error;
    view()
        .flex_col()
        .gap_2()
        .p_3()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(text("当前使用").text_size(px(15.0)))
        .child(text(move || {
            selection
                .get()
                .map(|selection| app_state::core_selection_line(&selection))
                .unwrap_or_else(|| "尚未选择核心".to_owned())
        }))
        .child(
            text(move || {
                error
                    .get()
                    .map(|error| app_state::core_error_line(&error))
                    .unwrap_or_default()
            })
            .test_id("core-selection-error")
            .text_size(px(12.0))
            .text_color(move || theme().destructive),
        )
}

/// 删除确认弹窗：取消只关弹窗；确认后后台删除并刷新列表。
fn delete_dialog(state: CorePageState) -> AnyElement {
    let Some((version, path)) = state.pending_delete.get() else {
        return view().into_element();
    };
    let on_cancel_state = state.clone();
    let cancel_button_state = state.clone();
    let confirm_state = state;
    dialog()
        .on_cancel(move || on_cancel_state.pending_delete.set(None))
        .child(
            view()
                .flex_col()
                .gap_3()
                .w(px(420.0))
                .p_4()
                .rounded(move || theme().radius)
                .bg(move || theme().card)
                .border(px(1.0))
                .border_color(move || theme().border)
                .child(text(format!("删除核心 {}？", core_tag(&version))))
                .child(
                    text("将删除该版本目录及其中的核心文件，无法撤销。")
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
                                .on_press(move || cancel_button_state.pending_delete.set(None))
                                .child(text("取消")),
                        )
                        .child(
                            button()
                                .px_3()
                                .py_1()
                                .rounded(move || theme().radius)
                                .bg(move || theme().destructive)
                                .on_press(move || {
                                    confirm_state.pending_delete.set(None);
                                    spawn_delete(
                                        confirm_state.clone(),
                                        version.clone(),
                                        path.clone(),
                                    );
                                })
                                .child(
                                    text("确认删除").text_color(move || theme().primary_foreground),
                                ),
                        ),
                )
                .into_element(),
        )
        .into_element()
}
