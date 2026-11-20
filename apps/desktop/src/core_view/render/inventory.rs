//! 核心库存卡片：类型/通道切换、远端查询与下载、本地版本行和展示排序。
//! 只读 `CorePageState` 的 Signal，把交互转发给 `core_view` 的后台操作。

use nsh_base::theme;
use nsh_core::prelude::*;

use super::super::{
    CHANNELS, CorePageState, LocalCoreRow, TYPES, core_tag, spawn_check_remote, spawn_download,
    spawn_select, supported_channel,
};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::layout::{SurfaceLevel, section_title, surface};
use crate::ui::select::{segmented_group, segmented_radio};
use lanpath_client::cores::{CoreChannel, channel_of_version};
use lanpath_common::CoreType;

/// 库存卡片：类型/通道切换、远端查询与下载、本地版本行。
pub(super) fn inventory_card(state: CorePageState) -> impl IntoElement {
    let type_state = state.clone();
    let channel_state = state.clone();
    let check_state = state.clone();
    let rows_state = state.clone();
    let empty_state = state.clone();
    surface(SurfaceLevel::Card)
        .p_5()
        .flex_col()
        .gap_4()
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_3()
                .child(section_title(|| t!("product.core.inventory")))
                .child({
                    let press_state = check_state.clone();
                    let busy_state = check_state;
                    crate::ui::button(crate::ui::ButtonVariant::Outline)
                        .test_id("core-check-remote")
                        .size(crate::ui::ButtonSize::Sm)
                        .prefix(|| icon_sized(Icon::Refresh, Tone::Muted, 14.0))
                        .loading(move || busy_state.busy.get())
                        .on_press(move || spawn_check_remote(press_state.clone()))
                        .label(|| t!("product.action.check_updates"))
                }),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_3()
                .child(segmented_group(
                    "core-type-group",
                    TYPES
                        .iter()
                        .map(|(kind, label)| {
                            type_radio(type_state.clone(), *kind, label).into_element()
                        })
                        .collect(),
                ))
                .child(segmented_group(
                    "core-channel-group",
                    CHANNELS
                        .iter()
                        .map(|(channel, label)| {
                            channel_radio(channel_state.clone(), *channel, label).into_element()
                        })
                        .collect(),
                )),
        )
        .child(remote_status_row(state))
        .child(when(
            move || filtered_rows(&empty_state).is_empty() && empty_state.error.get().is_none(),
            || {
                crate::ui::empty_with_icon(
                    Some(Icon::Core),
                    || t!("product.core.inventory"),
                    || t!("product.core.empty"),
                )
            },
        ))
        .child({
            let card_state = rows_state.clone();
            let rows = computed(move || filtered_rows(&rows_state));
            crate::ui::cards::card_grid(
                rows,
                112.0,
                Some(2),
                |row: &LocalCoreRow| row.path.to_string_lossy().to_string(),
                move |item| core_row(card_state.clone(), item).into_element(),
            )
        })
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
        .gap_3()
        .p_3()
        .rounded(move || theme().radius)
        .bg(move || theme().muted.opacity(0.4))
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(crate::ui::badge(
                    crate::ui::BadgeVariant::Outline,
                    move || {
                        let rows = filtered_rows(&local_latest_state);
                        match rows.first() {
                            Some(row) => format!("本地：{}", core_tag(&row.version)),
                            None => "本地：未安装".to_owned(),
                        }
                    },
                ))
                .child(crate::ui::badge(
                    crate::ui::BadgeVariant::Outline,
                    move || {
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
                    },
                ))
                .child(view().flex_1())
                .child(when(
                    move || busy_state.busy.get(),
                    move || {
                        view()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(crate::ui::spinner())
                            .child(
                                text(move || stage_state.stage.get().unwrap_or_default())
                                    .text_size(px(12.5))
                                    .text_color(move || theme().muted_foreground),
                            )
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
    crate::ui::button(crate::ui::ButtonVariant::Primary)
        .test_id("core-download")
        .size(crate::ui::ButtonSize::Md)
        .prefix(|| icon_sized(Icon::Download, Tone::OnPrimary, 14.0))
        .on_press(move || spawn_download(press_state.clone(), version.clone()))
        .label(move || label.clone())
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
    crate::ui::cards::selection_card(move || item.get().active, move || item.get().active)
        .test_id(&row_id)
        .label(format!("{core_type} {}", core_tag(&version)))
        .min_h(px(88.0))
        .flex_col()
        .justify_between()
        .gap_3()
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    text(core_tag(&version))
                        .text_size(px(15.0))
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .child(when(
                    move || item.get().active,
                    || {
                        crate::ui::badge(crate::ui::BadgeVariant::Success, || {
                            t!("product.core.selected")
                        })
                    },
                )),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_end()
                .gap_2()
                .child(when(
                    move || show_actions,
                    move || {
                        let press_state = use_state.clone();
                        let press_version = use_version.clone();
                        let press_path = use_path.clone();
                        crate::ui::button(crate::ui::ButtonVariant::Outline)
                            .test_id(&use_id)
                            .size(crate::ui::ButtonSize::Sm)
                            .on_press(move || {
                                spawn_select(
                                    press_state.clone(),
                                    core_type,
                                    press_version.clone(),
                                    press_path.clone(),
                                )
                            })
                            .label(|| t!("product.action.use"))
                    },
                ))
                .child(when(
                    move || show_actions,
                    move || {
                        let dialog_state = delete_state.clone();
                        let dialog_version = delete_version.clone();
                        let dialog_path = delete_path.clone();
                        crate::ui::button(crate::ui::ButtonVariant::Ghost)
                            .test_id(&delete_id)
                            .size(crate::ui::ButtonSize::Sm)
                            .on_press(move || {
                                dialog_state
                                    .pending_delete
                                    .set(Some((dialog_version.clone(), dialog_path.clone())));
                            })
                            .label(|| t!("product.action.delete"))
                    },
                )),
        )
}

fn type_radio(state: CorePageState, target: CoreType, label: &'static str) -> impl IntoElement {
    let checked = move || state.browsing.get() == target;
    let switch_state = state;
    let test_id = format!("core-type-{}", target.binary_name());
    segmented_radio(
        checked,
        move || label.to_owned(),
        test_id,
        move || switch_state.switch_type(target),
    )
}

fn channel_radio(
    state: CorePageState,
    target: CoreChannel,
    label: &'static str,
) -> impl IntoElement {
    let checked = move || state.channel.get() == target;
    let switch_state = state;
    let test_id = format!("core-channel-{}", target.as_str());
    segmented_radio(
        checked,
        move || label.to_owned(),
        test_id,
        move || {
            if !switch_state.busy.get() {
                switch_state.channel.set(target);
            }
        },
    )
}

/// 展示用版本降序（与共享 compare_core_versions 同一语义：数字段数值比较，
/// 同基数的预发布低于稳定版）；仅影响行顺序，不参与任何校验规则。
fn compare_versions_desc(left: &str, right: &str) -> std::cmp::Ordering {
    fn key(version: &str) -> (u64, u64, u64, u8, String) {
        let (stable_marker, core) = match version.find('-') {
            Some(index)
                if !version[..index].is_empty()
                    && version[..index]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_digit()) =>
            {
                (&version[index + 1..], &version[..index])
            }
            _ => ("", version),
        };
        let mut segments = core.split('.');
        let mut number = |default: u64| {
            segments
                .next()
                .and_then(|part| part.parse::<u64>().ok())
                .unwrap_or(default)
        };
        let (major, minor, patch) = (number(0), number(0), number(0));
        let stable = if stable_marker.is_empty() { 1 } else { 0 };
        (
            major,
            minor,
            patch,
            stable,
            stable_marker.to_ascii_lowercase(),
        )
    }
    key(right).cmp(&key(left))
}

/// 通道过滤后的本地行（按展示顺序降序）。
fn filtered_rows(state: &CorePageState) -> Vec<LocalCoreRow> {
    let channel = supported_channel(state.channel.get());
    let mut rows: Vec<_> = state
        .local
        .get()
        .into_iter()
        .filter(|row| supported_channel(channel_of_version(&row.version)) == channel)
        .collect();
    rows.sort_by(|a, b| compare_versions_desc(&a.version, &b.version));
    rows
}
