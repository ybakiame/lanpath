//! 已管理规则集列表面板：搜索、分类筛选、缓存状态徽章与单条操作。

use lanpath_client::rulesets::RuleSetSource;
use nsh_base::theme;
use nsh_core::prelude::*;

use super::state::{CATEGORIES, RulesetsState, RulesetsTab, category_label};
use crate::rulesets::RuleSetRow;
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::input::input;
use crate::ui::layout::{SurfaceLevel, surface};
use crate::ui::{BadgeVariant, badge};

pub(super) fn managed_panel(state: RulesetsState) -> AnyElement {
    let search = state.search;
    let category = state.category;
    let items = state.items;

    let filtered = computed(move || {
        let query = search.get().trim().to_lowercase();
        let cat = category.get();
        items.with(|list| {
            list.iter()
                .filter(|row| {
                    let cat_ok = cat == "all"
                        || row.entry.category.eq_ignore_ascii_case(&cat)
                        || (cat == "custom" && row.entry.category.is_empty());
                    let query_ok = query.is_empty()
                        || row.entry.tag.to_lowercase().contains(&query)
                        || row.entry.name.to_lowercase().contains(&query);
                    cat_ok && query_ok
                })
                .cloned()
                .collect::<Vec<_>>()
        })
    });

    let empty_market = state.clone();
    let empty_add = state.clone();
    let list_state = state;

    view()
        .flex_col()
        .gap_4()
        .child(filter_toolbar(search, category))
        .child(when(
            move || filtered.with(Vec::is_empty),
            move || {
                let open_market = empty_market.clone();
                let open_add = empty_add.clone();
                surface(SurfaceLevel::Card)
                    .p_6()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .child(icon_sized(Icon::Shield, Tone::Muted, 28.0))
                    .child(
                        text(move || {
                            if items.with(Vec::is_empty) {
                                t!("rulesets.empty")
                            } else {
                                t!("rulesets.empty_filtered")
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
                                    .size(ButtonSize::Md)
                                    .label(|| t!("rulesets.go_market"))
                                    .on_press(move || open_market.tab.set(RulesetsTab::Market)),
                            )
                            .child(
                                button(ButtonVariant::Secondary)
                                    .size(ButtonSize::Md)
                                    .label(|| t!("rulesets.add"))
                                    .on_press(move || open_add.open_add()),
                            ),
                    )
            },
        ))
        .child(view().flex_col().gap_3().child(each(
            move || filtered.get(),
            |row: &RuleSetRow| {
                format!(
                    "{}:{}:{}:{}",
                    row.entry.id, row.entry.tag, row.has_file, row.entry.remote_state.checked_at
                )
            },
            move |row, _| ruleset_card(list_state.clone(), row.get_untracked()),
        )))
        .into_element()
}

pub(super) fn filter_toolbar(search: Signal<String>, category: Signal<String>) -> AnyElement {
    let mut chips = view().flex_row().flex_wrap().items_center().gap_1();
    for &(id, label_key) in CATEGORIES {
        let active = move || category.get() == id;
        chips = chips.child(
            button(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .selected(active)
                .label(move || t!(label_key))
                .on_press(move || category.set(id.to_owned())),
        );
    }

    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap_3()
        .child(
            view().w(px(260.0)).child(
                input(search)
                    .test_id("rulesets-search")
                    .placeholder(|| t!("rulesets.search_placeholder"))
                    .on_change(move |value| search.set(value)),
            ),
        )
        .child(chips)
        .into_element()
}

fn ruleset_card(state: RulesetsState, row: RuleSetRow) -> AnyElement {
    let entry = row.entry;
    let id = entry.id.clone();
    let tag = entry.tag.clone();
    let is_remote = matches!(entry.source, RuleSetSource::Remote { .. });
    let format_label = match entry.file_format() {
        lanpath_client::rulesets::RuleSetFormat::Binary => "SRS",
        lanpath_client::rulesets::RuleSetFormat::Source => "JSON",
    };
    let source_summary = match &entry.source {
        RuleSetSource::Remote { url, .. } => url.clone(),
        RuleSetSource::Manual { .. } => row.rel_path.clone(),
    };
    let status_text = if !is_remote {
        t!("rulesets.status.manual")
    } else if row.has_file {
        t!("rulesets.status.cached", size = format_bytes(row.file_size))
    } else {
        t!("rulesets.status.missing")
    };
    let status_variant = if row.has_file {
        BadgeVariant::Success
    } else {
        BadgeVariant::Warning
    };

    let update_state = state.clone();
    let update_id = id.clone();
    let update_tag = tag.clone();
    let busy_check = state.clone();
    let busy_id_check = id.clone();

    let edit_state = state.clone();
    let edit_entry = entry.clone();

    let del_state = state;
    let del_id = id;
    let del_tag = tag.clone();

    let display_name = entry.display_name().to_owned();
    let cat_text = category_label(&entry.category);
    let tag_badge = tag.clone();

    surface(SurfaceLevel::Card)
        .p_4()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_4()
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .flex_col()
                .gap(px(6.0))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(
                            text(display_name)
                                .text_size(px(14.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(move || theme().foreground),
                        )
                        .child(badge(BadgeVariant::Outline, move || tag_badge.clone()))
                        .child(badge(BadgeVariant::Default, move || cat_text.clone()))
                        .child(badge(BadgeVariant::Default, move || {
                            format_label.to_owned()
                        }))
                        .child(badge(status_variant, move || status_text.clone())),
                )
                .child(
                    text(source_summary)
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground)
                        .truncate(),
                ),
        )
        .child(
            view()
                .flex_row()
                .flex_none()
                .items_center()
                .gap_2()
                .child(when(
                    move || is_remote,
                    move || {
                        let st = update_state.clone();
                        let uid = update_id.clone();
                        let utag = update_tag.clone();
                        let utag_id = update_tag.clone();
                        let b_check = busy_check.clone();
                        let b_label = busy_check.clone();
                        let b_id_label = busy_id_check.clone();
                        button(ButtonVariant::Secondary)
                            .test_id(format!("ruleset-update-{utag_id}"))
                            .size(ButtonSize::Sm)
                            .disabled(move || b_check.is_busy())
                            .on_press(move || st.update_one(uid.clone(), utag.clone()))
                            .prefix(|| {
                                icon_sized(Icon::Refresh, Tone::Foreground, 13.0).into_element()
                            })
                            .label(move || {
                                if b_label.busy_id.get().as_deref() == Some(b_id_label.as_str()) {
                                    t!("rulesets.updating")
                                } else {
                                    t!("rulesets.update")
                                }
                            })
                    },
                ))
                .child(
                    button(ButtonVariant::Ghost)
                        .test_id(format!("ruleset-edit-{}", edit_entry.tag))
                        .size(ButtonSize::Sm)
                        .label(|| t!("rulesets.edit"))
                        .on_press(move || edit_state.open_edit(&edit_entry)),
                )
                .child(
                    button(ButtonVariant::Ghost)
                        .test_id(format!("ruleset-delete-{del_tag}"))
                        .size(ButtonSize::Sm)
                        .prefix(|| icon_sized(Icon::Trash, Tone::Danger, 13.0).into_element())
                        .accessibility_label(format!("Delete {del_tag}"))
                        .on_press(move || {
                            del_state
                                .confirm_delete
                                .set(Some((del_id.clone(), del_tag.clone())));
                        }),
                ),
        )
        .into_element()
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}
