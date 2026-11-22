//! 规则集工作台主视图：受管规则集列表、精选市场、新增/编辑表单与右侧分组管理面板。

mod editor;
mod group_cards;
mod groups;
mod managed;
mod market;
pub(crate) mod picker;
mod state;

pub(crate) use groups::{GroupSheetController, groups_sheet};

use lanpath_common::CoreType;
use nsh_base::theme;
use nsh_core::prelude::*;

use crate::rulesets::RuleSetsService;
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::layout::{AlertTone, SurfaceLevel, alert_banner, page_header, surface};
use crate::ui::select::{segmented_group, segmented_radio};
use state::{RulesetsState, RulesetsTab};

pub(crate) fn rulesets_page() -> impl IntoElement {
    let state = RulesetsState::new();
    let group_sheet = GroupSheetController::new(state.items, state.groups);
    let guard_state = state.clone();
    let guard_sheet = group_sheet.clone();
    crate::navigation::protect(move || {
        if guard_state.is_busy() {
            crate::navigation::LeaveState::Busy
        } else if guard_state.editor_open.get() || guard_sheet.is_dirty() {
            crate::navigation::LeaveState::Dirty
        } else {
            crate::navigation::LeaveState::Clean
        }
    });

    let tab = state.tab;
    let items = state.items;
    let group_list = state.groups;
    let error = state.error;
    let notice = state.notice;
    let editor_open = state.editor_open;
    let confirm_delete = state.confirm_delete;

    let add_state = state.clone();
    let open_groups_sheet = group_sheet.clone();
    let update_all_state = state.clone();
    let busy_all_state = state.clone();
    let managed_state = state.clone();
    let market_state = state.clone();
    let editor_state = state.clone();
    let delete_state = state.clone();
    let busy_all_label = state.busy_all;

    let dir_hint = state
        .service
        .as_ref()
        .map(RuleSetsService::rulesets_dir_display)
        .unwrap_or_else(|| "rulesets".to_owned());

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
                        || t!("rulesets.title"),
                        || t!("rulesets.subtitle"),
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
                                text(|| t!("rulesets.mihomo_notice"))
                                    .text_size(px(12.5))
                                    .text_color(move || theme().foreground),
                            )
                        },
                    ))
                    .child(
                        view()
                            .flex_row()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(segmented_group(
                                "rulesets-tabs",
                                vec![
                                    segmented_radio(
                                        move || tab.get() == RulesetsTab::Managed,
                                        move || {
                                            t!("rulesets.tab.managed", count = items.with(Vec::len))
                                        },
                                        "rulesets-tab-managed",
                                        move || tab.set(RulesetsTab::Managed),
                                    ),
                                    segmented_radio(
                                        move || tab.get() == RulesetsTab::Market,
                                        || t!("rulesets.tab.market"),
                                        "rulesets-tab-market",
                                        move || tab.set(RulesetsTab::Market),
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
                                            .test_id("rulesets-open-groups")
                                            .size(ButtonSize::Md)
                                            .on_press(move || open_groups_sheet.open_panel())
                                            .prefix(|| {
                                                icon_sized(Icon::Layers, Tone::Foreground, 14.0)
                                                    .into_element()
                                            })
                                            .label(move || {
                                                t!(
                                                    "rulesets.group.manage_btn",
                                                    count = group_list.with(Vec::len)
                                                )
                                            }),
                                    )
                                    .child(
                                        button(ButtonVariant::Secondary)
                                            .test_id("rulesets-update-all")
                                            .size(ButtonSize::Md)
                                            .disabled(move || {
                                                busy_all_state.is_busy()
                                                    || items.with(Vec::is_empty)
                                            })
                                            .on_press(move || update_all_state.update_all())
                                            .prefix(|| {
                                                icon_sized(Icon::Refresh, Tone::Foreground, 14.0)
                                                    .into_element()
                                            })
                                            .label(move || {
                                                if busy_all_label.get() {
                                                    t!("rulesets.updating")
                                                } else {
                                                    t!("rulesets.update_all")
                                                }
                                            }),
                                    )
                                    .child(
                                        button(ButtonVariant::Primary)
                                            .test_id("rulesets-add")
                                            .size(ButtonSize::Md)
                                            .on_press(move || {
                                                tab.set(RulesetsTab::Managed);
                                                add_state.open_add();
                                            })
                                            .prefix(|| {
                                                icon_sized(Icon::Plus, Tone::OnPrimary, 14.0)
                                                    .into_element()
                                            })
                                            .label(|| t!("rulesets.add")),
                                    ),
                            ),
                    )
                    .child(
                        surface(SurfaceLevel::Card)
                            .px_3()
                            .py_2()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                text(move || format!("{dir_hint}/manifest.json"))
                                    .text_size(px(11.5))
                                    .text_color(move || theme().muted_foreground)
                                    .truncate(),
                            )
                            .child(
                                text(|| t!("rulesets.reference_hint"))
                                    .text_size(px(11.5))
                                    .text_color(move || theme().muted_foreground),
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
                        move || notice.get().is_some(),
                        move || {
                            alert_banner(
                                AlertTone::Success,
                                text(move || notice.get().unwrap_or_default())
                                    .text_size(px(12.5))
                                    .text_color(move || theme().foreground),
                            )
                        },
                    ))
                    .child(when(
                        move || editor_open.get(),
                        move || editor::editor_card(editor_state.clone()),
                    ))
                    .child(when(
                        move || tab.get() == RulesetsTab::Managed,
                        move || managed::managed_panel(managed_state.clone()),
                    ))
                    .child(when(
                        move || tab.get() == RulesetsTab::Market,
                        move || market::market_panel(market_state.clone()),
                    )),
            ),
        )
        .child(groups_sheet(group_sheet))
        .child(
            crate::ui::confirm_dialog(move || confirm_delete.get().is_some())
                .test_id("rulesets-delete-dialog")
                .title(move || {
                    let tag = confirm_delete.get().map(|(_, tag)| tag).unwrap_or_default();
                    t!("rulesets.delete_title", tag = tag)
                })
                .description(|| t!("rulesets.delete_description"))
                .confirm_label(|| t!("rulesets.delete_confirm"))
                .cancel_label(|| t!("common.cancel"))
                .destructive()
                .on_cancel(move || confirm_delete.set(None))
                .on_confirm(move || {
                    if let Some((id, _)) = confirm_delete.get() {
                        delete_state.confirm_remove(id);
                    }
                }),
        )
}
