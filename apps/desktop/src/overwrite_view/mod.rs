//! sing-box 配置切片工作台：`overwrite/sing-box/*.json` 双模式编辑（GUI 表单 + JSON 编辑器）
//! 以及 `app.toml` 受管入站端口与控制 API 表单。

mod dns_form;
mod dns_presets_panel;
mod dns_servers;
mod editor;
mod forms;
mod forms_lists;
mod managed_card;
mod outbounds_editor;
mod outbounds_form;
mod state;

use lanpath_client::singbox_overwrite::SingboxSliceKind;
use lanpath_common::CoreType;
use nsh_base::theme;
use nsh_core::prelude::*;

use crate::singbox_overwrite::SingboxOverwriteService;
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::layout::{
    AlertTone, SurfaceLevel, alert_banner, page_header, section_title, surface,
};
use crate::ui::select::{segmented_group, segmented_radio};
use state::{EditMode, OverwriteState};

pub(crate) fn slice_label(kind: SingboxSliceKind) -> String {
    match kind {
        SingboxSliceKind::Log => t!("overwrite.slice.log"),
        SingboxSliceKind::Dns => t!("overwrite.slice.dns"),
        SingboxSliceKind::Route => t!("overwrite.slice.route"),
        SingboxSliceKind::Outbounds => t!("overwrite.slice.outbounds"),
        SingboxSliceKind::Experimental => t!("overwrite.slice.experimental"),
    }
}

pub(crate) fn overwrite_page() -> impl IntoElement {
    let state = OverwriteState::new();
    let guard_state = state.clone();
    crate::navigation::protect(move || {
        if guard_state.is_any_dirty() {
            crate::navigation::LeaveState::Dirty
        } else {
            crate::navigation::LeaveState::Clean
        }
    });

    let active_slice = state.active_slice;
    let edit_mode = state.edit_mode;
    let error = state.error;
    let notice = state.notice;
    let confirm_reset = state.confirm_reset;
    let reset_confirm_state = state.clone();
    let save_state = state.clone();
    let format_state = state.clone();
    let dirty_state = state.clone();
    let customized_badge_state = state.clone();
    let slice_tabs_state = state.clone();
    let form_state = state.clone();
    let editor_state = state.clone();
    let bindings_state = state.clone();
    let dir_hint = state
        .service
        .as_ref()
        .map(SingboxOverwriteService::overwrite_dir_display)
        .unwrap_or_else(|| "overwrite/sing-box".to_owned());

    view()
        .flex_1()
        .flex_col()
        .min_h(px(0.0))
        .child(
            scroll().flex_1().min_h(px(0.0)).child(
                view()
                    .flex_col()
                    .gap_5()
                    .child(page_header(
                        || t!("overwrite.title"),
                        || t!("overwrite.subtitle"),
                    ))
                    .child(when(
                        move || {
                            crate::app_state::current()
                                .active_core
                                .get()
                                .is_some_and(|c| c.core_type == CoreType::Mihomo)
                        },
                        || {
                            alert_banner(
                                AlertTone::Warning,
                                text(|| t!("overwrite.mihomo_notice"))
                                    .text_size(px(12.5))
                                    .text_color(move || theme().foreground),
                            )
                        },
                    ))
                    .child(managed_card::managed_bindings_card(bindings_state))
                    .child(
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
                                    .child(
                                        view()
                                            .flex_row()
                                            .flex_wrap()
                                            .items_center()
                                            .gap_2()
                                            .child(section_title(|| t!("overwrite.slices_title")))
                                            .child(crate::ui::badge(
                                                crate::ui::BadgeVariant::Outline,
                                                move || {
                                                    format!(
                                                        "overwrite/sing-box/{}",
                                                        active_slice.get().file_name()
                                                    )
                                                },
                                            ))
                                            .child(when(
                                                move || {
                                                    let idx = OverwriteState::slice_index(
                                                        customized_badge_state.active_slice.get(),
                                                    );
                                                    customized_badge_state.customized.with(|f| {
                                                        f.get(idx).copied().unwrap_or(false)
                                                    })
                                                },
                                                || {
                                                    crate::ui::badge(
                                                        crate::ui::BadgeVariant::Default,
                                                        || t!("overwrite.customized"),
                                                    )
                                                },
                                            )),
                                    )
                                    .child(segmented_group(
                                        "overwrite-mode-group",
                                        vec![
                                            segmented_radio(
                                                move || edit_mode.get() == EditMode::Form,
                                                || t!("overwrite.mode.form"),
                                                "overwrite-mode-form",
                                                move || edit_mode.set(EditMode::Form),
                                            )
                                            .into_element(),
                                            segmented_radio(
                                                move || edit_mode.get() == EditMode::Json,
                                                || t!("overwrite.mode.json"),
                                                "overwrite-mode-json",
                                                move || edit_mode.set(EditMode::Json),
                                            )
                                            .into_element(),
                                        ],
                                    )),
                            )
                            .child(
                                text(move || t!("overwrite.dir_hint", path = dir_hint.clone()))
                                    .text_size(px(12.0))
                                    .text_color(move || theme().muted_foreground)
                                    .selectable(true),
                            )
                            .child(segmented_group(
                                "overwrite-slice-group",
                                SingboxSliceKind::ALL
                                    .into_iter()
                                    .map(|kind| {
                                        let st = slice_tabs_state.clone();
                                        let idx = OverwriteState::slice_index(kind);
                                        segmented_radio(
                                            move || st.active_slice.get() == kind,
                                            move || {
                                                let mark = if st
                                                    .customized
                                                    .with(|f| f.get(idx).copied().unwrap_or(false))
                                                {
                                                    " •"
                                                } else {
                                                    ""
                                                };
                                                format!("{}{mark}", slice_label(kind))
                                            },
                                            format!("overwrite-slice-{}", kind.id()),
                                            move || st.active_slice.set(kind),
                                        )
                                        .into_element()
                                    })
                                    .collect(),
                            ))
                            .child(when(
                                move || edit_mode.get() == EditMode::Form,
                                move || forms::slice_form(form_state.clone()),
                            ))
                            .child(when(
                                move || edit_mode.get() == EditMode::Json,
                                move || editor::json_editor(editor_state.clone()),
                            ))
                            .child(crate::ui::separator())
                            .child(
                                view()
                                    .flex_row()
                                    .flex_wrap()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        button(ButtonVariant::Primary)
                                            .size(ButtonSize::Md)
                                            .test_id("overwrite-save")
                                            .label(|| t!("overwrite.save_slice"))
                                            .on_press(move || save_state.save_active_slice()),
                                    )
                                    .child(when(
                                        move || edit_mode.get() == EditMode::Json,
                                        move || {
                                            let fmt = format_state.clone();
                                            button(ButtonVariant::Outline)
                                                .size(ButtonSize::Sm)
                                                .test_id("overwrite-format")
                                                .label(|| t!("overwrite.format_json"))
                                                .on_press(move || fmt.format_active_slice())
                                        },
                                    ))
                                    .child(
                                        button(ButtonVariant::Outline)
                                            .size(ButtonSize::Sm)
                                            .test_id("overwrite-reset")
                                            .prefix(|| icon_sized(Icon::Refresh, Tone::Muted, 13.0))
                                            .label(|| t!("overwrite.reset_slice"))
                                            .on_press(move || confirm_reset.set(true)),
                                    )
                                    .child(when(
                                        move || dirty_state.is_active_slice_dirty(),
                                        || {
                                            crate::ui::badge(
                                                crate::ui::BadgeVariant::Warning,
                                                || t!("navigation.dirty_title"),
                                            )
                                        },
                                    )),
                            )
                            .child(when(
                                move || error.get().is_some(),
                                move || {
                                    alert_banner(
                                        AlertTone::Danger,
                                        text(move || error.get().unwrap_or_default())
                                            .test_id("overwrite-error")
                                            .text_size(px(12.5))
                                            .text_color(move || theme().destructive)
                                            .selectable(true),
                                    )
                                },
                            ))
                            .child(when(
                                move || notice.get().is_some(),
                                move || {
                                    alert_banner(
                                        AlertTone::Success,
                                        text(move || notice.get().unwrap_or_default())
                                            .test_id("overwrite-notice")
                                            .text_size(px(12.5))
                                            .text_color(move || theme().foreground),
                                    )
                                },
                            )),
                    ),
            ),
        )
        .child(
            crate::ui::confirm_dialog(move || confirm_reset.get())
                .destructive()
                .test_id("overwrite-reset-dialog")
                .confirm_test_id("overwrite-reset-confirm")
                .title(move || {
                    t!(
                        "overwrite.reset_title",
                        file = active_slice.get().file_name()
                    )
                })
                .description(|| t!("overwrite.reset_hint"))
                .confirm_label(|| t!("overwrite.reset_confirm"))
                .on_cancel(move || confirm_reset.set(false))
                .on_confirm(move || reset_confirm_state.reset_active_slice()),
        )
}
