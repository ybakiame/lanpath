//! 核心页渲染：页面结构、卡片、库存行、单选与删除确认弹窗。状态与后台
//! 操作在 `super`（core_view.rs）；本模块只读 Signal、把交互转发给
//! `spawn_*` 后台操作。操作反馈（notice/error/stage）一律只显示版本，
//! 不重复完整路径。

use nsh_base::{button, dialog, radio, radio_group, theme};
use nsh_core::prelude::*;

use super::{
    CHANNELS, CorePageState, LocalCoreRow, TYPES, core_tag, filtered_rows, spawn_check_remote,
    spawn_delete, spawn_download, spawn_select,
};
use lanpath_client::cores::CoreChannel;
use lanpath_common::CoreType;

pub(crate) fn core_page() -> impl IntoElement {
    let state = CorePageState::new();
    let selection = state.clone();
    let inventory = state.clone();
    let notice_state = state.clone();
    let error_state = state.clone();
    let dialog_state = state;
    view()
        .flex_1()
        .flex_col()
        .gap_4()
        .child(text("核心").text_size(px(24.0)))
        .child(
            text("仅管理本数据根 cores/ 下的核心文件：下载、选择、删除 · 不启动核心/系统代理/TUN")
                .text_color(move || theme().muted_foreground),
        )
        .child(selection_card(selection))
        .child(inventory_card(inventory))
        .child(
            text(move || notice_state.notice.get().unwrap_or_default())
                .text_color(move || theme().muted_foreground),
        )
        .child(
            text(move || error_state.error.get().unwrap_or_default())
                .text_color(move || theme().destructive),
        )
        .child(when(
            move || dialog_state.pending_delete.get().is_some(),
            move || delete_dialog(dialog_state.clone()),
        ))
}

/// 实际选中卡片：独立于浏览类型；为空显示“尚未选择核心”。
fn selection_card(state: CorePageState) -> impl IntoElement {
    let summary_state = state;
    view()
        .flex_col()
        .gap_2()
        .p_3()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(text("当前使用").text_size(px(15.0)))
        .child(text(move || {
            summary_state
                .active
                .get()
                .map(|summary| format!("{} · {}", summary.core_type, core_tag(&summary.version)))
                .unwrap_or_else(|| "尚未选择核心".to_owned())
        }))
}

/// 库存卡片：类型/通道切换、远端查询与下载、本地版本行。
fn inventory_card(state: CorePageState) -> impl IntoElement {
    let type_state = state.clone();
    let channel_state = state.clone();
    let check_state = state.clone();
    let rows_state = state.clone();
    let empty_state = state.clone();
    view()
        .flex_col()
        .gap_3()
        .p_3()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(text("库存").text_size(px(15.0)))
        .child(
            radio_group().flex_row().gap_3().children(
                TYPES
                    .iter()
                    .map(|(kind, label)| type_radio(type_state.clone(), *kind, label)),
            ),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child(radio_group().flex_row().gap_3().children(
                    CHANNELS.iter().map(|(channel, label)| {
                        channel_radio(channel_state.clone(), *channel, label)
                    }),
                ))
                .child(view().flex_1())
                .child({
                    let press_state = check_state.clone();
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .border(px(1.0))
                        .border_color(move || theme().border)
                        .on_press(move || spawn_check_remote(press_state.clone()))
                        .child(
                            view()
                                .test_id("core-check-remote")
                                .child(text("检查更新").text_size(px(12.0))),
                        )
                }),
        )
        .child(remote_status_row(state))
        .child(when(
            move || filtered_rows(&empty_state).is_empty() && empty_state.error.get().is_none(),
            || {
                text("此通道尚无本地版本，可切换通道或检查更新后下载。")
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground)
            },
        ))
        .child(each(
            rows_state.local,
            |row: &LocalCoreRow| row.path.clone(),
            move |item, _index| core_row(rows_state.clone(), item),
        ))
}

/// 远端状态行：本地最新 / 远程版本 / busy 阶段 / 下载按钮。
fn remote_status_row(state: CorePageState) -> impl IntoElement {
    let local_latest_state = state.clone();
    let remote_text_state = state.clone();
    let busy_state = state.clone();
    let stage_state = state.clone();
    let download_cond_state = state.clone();
    let download_button_state = state;
    view()
        .flex_col()
        .gap_2()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .child(text(move || {
                    let rows = filtered_rows(&local_latest_state);
                    match rows.first() {
                        Some(row) => format!("本地：{}", core_tag(&row.version)),
                        None => "本地：未安装".to_owned(),
                    }
                }))
                .child(text(move || {
                    let channel = remote_text_state.channel.get().as_str();
                    match remote_text_state
                        .remote
                        .get()
                        .iter()
                        .find(|row| row.channel == channel)
                    {
                        Some(row) => format!("远程：{}", core_tag(&row.version)),
                        None => "远程：尚未检查".to_owned(),
                    }
                }))
                .child(view().flex_1())
                .child(when(
                    move || busy_state.busy.get(),
                    move || {
                        text(move || stage_state.stage.get().unwrap_or_default())
                            .text_size(px(12.0))
                            .text_color(move || theme().muted_foreground)
                    },
                )),
        )
        .child(when(
            move || {
                download_cond_state.remote_checked.get()
                    && download_cond_state.current_remote().is_some()
            },
            move || download_button(download_button_state.clone()),
        ))
}

fn download_button(state: CorePageState) -> AnyElement {
    let Some(remote) = state.current_remote() else {
        return view().into_element();
    };
    let version = remote.version.clone();
    let installed = state.is_installed(&version);
    let label = if installed {
        format!("重新校验并选中 {}", core_tag(&version))
    } else {
        format!("下载 {} 并选中", core_tag(&version))
    };
    let press_state = state;
    button()
        .px_3()
        .py_1()
        .rounded(move || theme().radius)
        .bg(move || theme().primary)
        .on_press(move || spawn_download(press_state.clone(), version.clone()))
        .child(
            view()
                .test_id("core-download")
                .child(text(label).text_color(move || theme().primary_foreground)),
        )
        .into_element()
}

/// 本地版本行：版本 + active 标记；非 active 行提供使用/删除（删除需确认）。
fn core_row(state: CorePageState, item: ReadSignal<LocalCoreRow>) -> impl IntoElement {
    let use_state = state.clone();
    let delete_state = state;
    let row = item.get();
    let version = row.version.clone();
    let use_version = row.version.clone();
    let delete_version = row.version.clone();
    let use_path = row.path.clone();
    let delete_path = row.path.clone();
    let core_type = row.core_type;
    let row_id = format!("core-row-{}-{version}", core_type.binary_name());
    let use_id = format!("core-use-{}-{version}", core_type.binary_name());
    let delete_id = format!("core-delete-{}-{version}", core_type.binary_name());
    let show_actions = !row.active;
    view()
        .test_id(&row_id)
        .flex_row()
        .items_center()
        .gap_3()
        .py_2()
        .child(text(core_tag(&version)))
        .child(when(
            move || item.get().active,
            || {
                text("当前使用")
                    .text_size(px(12.0))
                    .text_color(move || theme().primary)
            },
        ))
        .child(view().flex_1())
        .child(when(
            move || show_actions,
            move || {
                let press_state = use_state.clone();
                let press_version = use_version.clone();
                let press_path = use_path.clone();
                button()
                    .px_3()
                    .py_1()
                    .rounded(move || theme().radius)
                    .border(px(1.0))
                    .border_color(move || theme().border)
                    .on_press(move || {
                        spawn_select(
                            press_state.clone(),
                            core_type,
                            press_version.clone(),
                            press_path.clone(),
                        )
                    })
                    .child(
                        view()
                            .test_id(&use_id)
                            .child(text("使用").text_size(px(12.0))),
                    )
            },
        ))
        .child(when(
            move || show_actions,
            move || {
                let dialog_state = delete_state.clone();
                let dialog_version = delete_version.clone();
                let dialog_path = delete_path.clone();
                button()
                    .px_3()
                    .py_1()
                    .rounded(move || theme().radius)
                    .border(px(1.0))
                    .border_color(move || theme().destructive)
                    .on_press(move || {
                        dialog_state
                            .pending_delete
                            .set(Some((dialog_version.clone(), dialog_path.clone())));
                    })
                    .child(
                        view().test_id(&delete_id).child(
                            text("删除")
                                .text_size(px(12.0))
                                .text_color(move || theme().destructive),
                        ),
                    )
            },
        ))
}

fn type_radio(state: CorePageState, target: CoreType, label: &str) -> impl IntoElement {
    let checked = move || state.browsing.get() == target;
    let switch_state = state;
    let test_id = format!("core-type-{}", target.binary_name());
    radio()
        .checked(checked)
        .on_change(move |_: &bool| switch_state.switch_type(target))
        .child(
            view()
                .test_id(&test_id)
                .flex_row()
                .items_center()
                .gap_2()
                .child(radio_dot(checked))
                .child(text(label.to_owned())),
        )
}

fn channel_radio(state: CorePageState, target: CoreChannel, label: &str) -> impl IntoElement {
    let checked = move || state.channel.get() == target;
    let switch_state = state;
    let test_id = format!("core-channel-{}", target.as_str());
    radio()
        .checked(checked)
        .on_change(move |_: &bool| {
            if !switch_state.busy.get() {
                switch_state.channel.set(target);
            }
        })
        .child(
            view()
                .test_id(&test_id)
                .flex_row()
                .items_center()
                .gap_2()
                .child(radio_dot(checked))
                .child(text(label.to_owned())),
        )
}

fn radio_dot(checked: impl Fn() -> bool + 'static) -> impl IntoElement {
    view()
        .size(px(14.0))
        .rounded_full()
        .border(px(1.0))
        .border_color(move || theme().muted_foreground)
        .bg(move || {
            if checked() {
                theme().primary
            } else {
                theme().card
            }
        })
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
