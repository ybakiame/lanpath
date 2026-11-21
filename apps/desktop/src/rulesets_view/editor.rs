//! 新增/编辑规则集表单卡片：支持远程 URL（`.srs`/`.json`）与手动 sing-box JSON 源码。

use lanpath_client::rulesets::RuleSetFormat;
use nsh_base::{SelectItem, editor, theme};
use nsh_core::prelude::*;

use super::state::{DraftSourceKind, RulesetsState};
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::input::{field, input};
use crate::ui::layout::{SurfaceLevel, section_title, surface};
use crate::ui::select::{segmented_group, segmented_radio, select};

pub(super) fn editor_card(state: RulesetsState) -> AnyElement {
    let editing_id = state.editing_id;
    let draft_tag = state.draft_tag;
    let draft_name = state.draft_name;
    let draft_category = state.draft_category;
    let draft_source_kind = state.draft_source_kind;
    let draft_url = state.draft_url;
    let draft_format = state.draft_format;
    let draft_manual = state.draft_manual;

    let save_state = state.clone();
    let cancel_state = state;

    let format_value = computed(move || match draft_format.get() {
        RuleSetFormat::Binary => "binary".to_owned(),
        RuleSetFormat::Source => "source".to_owned(),
    });

    surface(SurfaceLevel::Card)
        .p_5()
        .flex_col()
        .gap_4()
        .border(px(1.5))
        .border_color(move || theme().primary.opacity(0.45))
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_3()
                .child(section_title(move || {
                    if editing_id.get().is_some() {
                        t!("rulesets.editor.edit_title")
                    } else {
                        t!("rulesets.editor.add_title")
                    }
                }))
                .child(segmented_group(
                    "ruleset-source-kind",
                    vec![
                        segmented_radio(
                            move || draft_source_kind.get() == DraftSourceKind::Remote,
                            || t!("rulesets.source.remote"),
                            "ruleset-source-remote",
                            move || draft_source_kind.set(DraftSourceKind::Remote),
                        ),
                        segmented_radio(
                            move || draft_source_kind.get() == DraftSourceKind::Manual,
                            || t!("rulesets.source.manual"),
                            "ruleset-source-manual",
                            move || draft_source_kind.set(DraftSourceKind::Manual),
                        ),
                    ],
                )),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_start()
                .gap_4()
                .child(
                    view().w(px(220.0)).child(
                        field()
                            .label(|| t!("rulesets.field.tag"))
                            .hint(|| t!("rulesets.field.tag_hint"))
                            .control(
                                input(draft_tag)
                                    .test_id("ruleset-draft-tag")
                                    .placeholder(|| "geosite-custom".to_owned())
                                    .on_change(move |value| draft_tag.set(value)),
                            ),
                    ),
                )
                .child(
                    view().w(px(220.0)).child(
                        field().label(|| t!("rulesets.field.name")).control(
                            input(draft_name)
                                .test_id("ruleset-draft-name")
                                .placeholder(|| t!("rulesets.field.name_placeholder"))
                                .on_change(move |value| draft_name.set(value)),
                        ),
                    ),
                )
                .child(
                    view().w(px(180.0)).child(
                        field()
                            .label(|| t!("rulesets.field.category"))
                            .control(select(
                                vec![
                                    SelectItem::new("cn", t!("rulesets.cat.cn")),
                                    SelectItem::new("proxy", t!("rulesets.cat.proxy")),
                                    SelectItem::new("ads", t!("rulesets.cat.ads")),
                                    SelectItem::new("ai", t!("rulesets.cat.ai")),
                                    SelectItem::new("media", t!("rulesets.cat.media")),
                                    SelectItem::new("custom", t!("rulesets.cat.custom")),
                                ],
                                move || Some(draft_category.get().into()),
                                move |value| {
                                    if let Some(value) = value {
                                        draft_category.set(value.to_string());
                                    }
                                },
                            )),
                    ),
                ),
        )
        .child(when(
            move || draft_source_kind.get() == DraftSourceKind::Remote,
            move || {
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_start()
                    .gap_4()
                    .child(
                        view().flex_1().min_w(px(300.0)).child(
                            field().label(|| t!("rulesets.field.url")).control(
                                input(draft_url)
                                    .test_id("ruleset-draft-url")
                                    .placeholder(|| "https://example.com/ruleset.srs".to_owned())
                                    .on_change(move |value| draft_url.set(value)),
                            ),
                        ),
                    )
                    .child(
                        view().w(px(180.0)).child(
                            field()
                                .label(|| t!("rulesets.field.format"))
                                .control(select(
                                    vec![
                                        SelectItem::new("binary", "Binary (.srs)"),
                                        SelectItem::new("source", "Source (.json)"),
                                    ],
                                    move || Some(format_value.get().into()),
                                    move |value| {
                                        if let Some(value) = value {
                                            draft_format.set(if value == "source" {
                                                RuleSetFormat::Source
                                            } else {
                                                RuleSetFormat::Binary
                                            });
                                        }
                                    },
                                )),
                        ),
                    )
            },
        ))
        .child(when(
            move || draft_source_kind.get() == DraftSourceKind::Manual,
            move || {
                view()
                    .flex_col()
                    .gap_2()
                    .child(
                        text(|| t!("rulesets.field.manual_hint"))
                            .text_size(px(12.0))
                            .text_color(move || theme().muted_foreground),
                    )
                    .child(
                        editor()
                            .test_id("ruleset-draft-manual")
                            .value(move || Some(draft_manual.get().into()))
                            .language("json")
                            .line_numbers(true)
                            .highlighting(true)
                            .auto_close(true)
                            .auto_indent(true)
                            .font_size(px(13.0))
                            .line_height(px(20.0))
                            .on_change(move |value: &SharedString| {
                                draft_manual.set(value.to_string());
                            })
                            .h(px(220.0))
                            .w_full()
                            .border(px(1.0))
                            .border_color(move || theme().input)
                            .rounded(move || theme().radius),
                    )
            },
        ))
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_end()
                .gap_2()
                .child(
                    button(ButtonVariant::Ghost)
                        .test_id("ruleset-editor-cancel")
                        .size(ButtonSize::Md)
                        .label(|| t!("common.cancel"))
                        .on_press(move || cancel_state.close_editor()),
                )
                .child(
                    button(ButtonVariant::Primary)
                        .test_id("ruleset-editor-save")
                        .size(ButtonSize::Md)
                        .prefix(|| icon_sized(Icon::Check, Tone::OnPrimary, 14.0).into_element())
                        .label(|| t!("rulesets.editor.save"))
                        .on_press(move || save_state.save_editor()),
                ),
        )
        .into_element()
}
