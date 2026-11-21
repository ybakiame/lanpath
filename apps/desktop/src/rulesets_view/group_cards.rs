//! 规则集分组侧边面板内的分组编辑表单与分组卡片视图。

use lanpath_client::rulesets::RuleSetGroup;
use nsh_base::theme;
use nsh_core::prelude::*;

use super::groups::GroupSheetController;
use crate::rulesets::RuleSetRow;
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::input::{field, input};
use crate::ui::layout::{SurfaceLevel, section_title, surface};
use crate::ui::{BadgeVariant, badge};

pub(super) fn group_editor_card(ctrl: GroupSheetController) -> AnyElement {
    let editing_id = ctrl.editing_id;
    let draft_tag = ctrl.draft_tag;
    let draft_name = ctrl.draft_name;
    let draft_desc = ctrl.draft_desc;
    let draft_members = ctrl.draft_members;
    let items = ctrl.items;

    let toggle_ctrl = ctrl.clone();
    let save_ctrl = ctrl.clone();
    let cancel_ctrl = ctrl;

    surface(SurfaceLevel::Card)
        .p_4()
        .flex_col()
        .gap_3()
        .border(px(1.5))
        .border_color(move || theme().primary.opacity(0.45))
        .child(section_title(move || {
            if editing_id.get().is_some() {
                t!("rulesets.group.edit_title")
            } else {
                t!("rulesets.group.add_title")
            }
        }))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_start()
                .gap_3()
                .child(
                    view().flex_1().min_w(px(190.0)).child(
                        field()
                            .label(|| t!("rulesets.group.tag"))
                            .hint(|| t!("rulesets.group.tag_hint"))
                            .control(
                                input(draft_tag)
                                    .test_id("ruleset-group-draft-tag")
                                    .placeholder(|| "direct-cn".to_owned())
                                    .on_change(move |v| draft_tag.set(v)),
                            ),
                    ),
                )
                .child(
                    view().flex_1().min_w(px(190.0)).child(
                        field().label(|| t!("rulesets.group.name")).control(
                            input(draft_name)
                                .test_id("ruleset-group-draft-name")
                                .placeholder(|| t!("rulesets.group.name_placeholder"))
                                .on_change(move |v| draft_name.set(v)),
                        ),
                    ),
                ),
        )
        .child(
            field().label(|| t!("rulesets.group.desc")).control(
                input(draft_desc)
                    .test_id("ruleset-group-draft-desc")
                    .placeholder(|| t!("rulesets.group.desc_placeholder"))
                    .on_change(move |v| draft_desc.set(v)),
            ),
        )
        .child(
            view()
                .flex_col()
                .gap_2()
                .child(
                    text(move || {
                        t!(
                            "rulesets.group.members_label",
                            count = draft_members.with(Vec::len)
                        )
                    })
                    .text_size(px(12.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(move || theme().foreground),
                )
                .child(when(
                    move || items.with(Vec::is_empty),
                    || {
                        text(|| t!("rulesets.group.no_items"))
                            .text_size(px(12.0))
                            .text_color(move || theme().muted_foreground)
                    },
                ))
                .child(view().flex_row().flex_wrap().gap_2().child(each(
                    move || items.get(),
                    |row: &RuleSetRow| row.entry.id.clone(),
                    move |row, _| {
                        let tag = row.get_untracked().entry.tag;
                        let tag_label = tag.clone();
                        let tag_check = tag.clone();
                        let tag_id = tag.clone();
                        let st = toggle_ctrl.clone();
                        let selected = move || {
                            draft_members.with(|list| {
                                list.iter().any(|t| t.eq_ignore_ascii_case(&tag_check))
                            })
                        };
                        button(ButtonVariant::Outline)
                            .test_id(format!("ruleset-group-member-{tag_id}"))
                            .size(ButtonSize::Sm)
                            .selected(selected)
                            .label(move || tag_label.clone())
                            .on_press(move || st.toggle_member(&tag))
                    },
                ))),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_end()
                .gap_2()
                .child(
                    button(ButtonVariant::Ghost)
                        .test_id("ruleset-group-editor-cancel")
                        .size(ButtonSize::Sm)
                        .label(|| t!("common.cancel"))
                        .on_press(move || cancel_ctrl.close_editor()),
                )
                .child(
                    button(ButtonVariant::Primary)
                        .test_id("ruleset-group-editor-save")
                        .size(ButtonSize::Sm)
                        .prefix(|| icon_sized(Icon::Check, Tone::OnPrimary, 13.0).into_element())
                        .label(|| t!("rulesets.group.save"))
                        .on_press(move || save_ctrl.save_editor()),
                ),
        )
        .into_element()
}

pub(super) fn group_card(ctrl: GroupSheetController, group: RuleSetGroup) -> AnyElement {
    let display = group.display_name().to_owned();
    let token = group.ref_token();
    let count = group.member_tags.len();
    let desc = group.description.clone();
    let has_desc = !desc.is_empty();
    let members = group.member_tags.clone();

    let edit_ctrl = ctrl.clone();
    let edit_group = group.clone();
    let del_ctrl = ctrl.clone();
    let pending_delete = ctrl.pending_delete;
    let id_for_check = group.id.clone();
    let id_for_ask = group.id.clone();
    let id_for_confirm = group.id.clone();
    let del_tag = group.tag.clone();
    let del_tag_id = group.tag.clone();
    let confirm_tag_id = group.tag.clone();

    let mut pills = view().flex_row().flex_wrap().items_center().gap_1();
    for member in members {
        pills = pills.child(badge(BadgeVariant::Default, move || member.clone()));
    }

    let confirming =
        computed(move || pending_delete.get().as_deref() == Some(id_for_check.as_str()));

    surface(SurfaceLevel::Card)
        .p_4()
        .flex_col()
        .gap_3()
        .child(
            view()
                .flex_row()
                .items_start()
                .justify_between()
                .gap_3()
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
                                    text(display)
                                        .text_size(px(14.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(move || theme().foreground),
                                )
                                .child(badge(BadgeVariant::Outline, move || token.clone()))
                                .child(badge(BadgeVariant::Success, move || {
                                    t!("rulesets.group.member_count", count = count)
                                })),
                        )
                        .child(when(
                            move || has_desc,
                            move || {
                                let d = desc.clone();
                                text(d)
                                    .text_size(px(12.0))
                                    .text_color(move || theme().muted_foreground)
                            },
                        )),
                )
                .child(
                    view()
                        .flex_row()
                        .flex_none()
                        .items_center()
                        .gap_1()
                        .child(when(
                            move || !confirming.get(),
                            move || {
                                let edit_ctrl = edit_ctrl.clone();
                                let edit_group = edit_group.clone();
                                let id_for_ask = id_for_ask.clone();
                                let del_tag = del_tag.clone();
                                let del_tag_id = del_tag_id.clone();
                                view()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        button(ButtonVariant::Ghost)
                                            .test_id(format!(
                                                "ruleset-group-edit-{}",
                                                edit_group.tag
                                            ))
                                            .size(ButtonSize::Sm)
                                            .label(|| t!("rulesets.edit"))
                                            .on_press(move || edit_ctrl.open_edit(&edit_group)),
                                    )
                                    .child(
                                        button(ButtonVariant::Ghost)
                                            .test_id(format!("ruleset-group-delete-{del_tag_id}"))
                                            .size(ButtonSize::Sm)
                                            .prefix(|| {
                                                icon_sized(Icon::Trash, Tone::Danger, 13.0)
                                                    .into_element()
                                            })
                                            .accessibility_label(format!("Delete group {del_tag}"))
                                            .on_press(move || {
                                                pending_delete.set(Some(id_for_ask.clone()));
                                            }),
                                    )
                            },
                        ))
                        .child(when(
                            move || confirming.get(),
                            move || {
                                let del_ctrl = del_ctrl.clone();
                                let id_for_confirm = id_for_confirm.clone();
                                let confirm_tag_id = confirm_tag_id.clone();
                                view()
                                    .flex_row()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        button(ButtonVariant::Ghost)
                                            .size(ButtonSize::Sm)
                                            .label(|| t!("common.cancel"))
                                            .on_press(move || pending_delete.set(None)),
                                    )
                                    .child(
                                        button(ButtonVariant::Destructive)
                                            .test_id(format!(
                                                "ruleset-group-delete-confirm-{confirm_tag_id}"
                                            ))
                                            .size(ButtonSize::Sm)
                                            .label(|| t!("rulesets.delete_confirm"))
                                            .on_press(move || {
                                                del_ctrl.confirm_remove(&id_for_confirm);
                                            }),
                                    )
                            },
                        )),
                ),
        )
        .child(pills)
        .into_element()
}
