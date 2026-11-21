use super::*;
use crate::rulesets::RuleSetRow;
use crate::ui::{
    button::ButtonSize,
    icon::{Icon, Tone, icon_sized},
    input::{field, textarea},
    layout::{SurfaceLevel, surface},
    select::{checkbox, select, tab_bar},
};
use lanpath_client::rulesets::RuleSetGroup;
use nsh_base::{SelectItem, theme};
use nsh_core::prelude::*;

pub(super) fn editor(state: State) -> impl IntoElement {
    let form = state.clone();
    let enter = state.clone();
    let batch = state.batch;
    let mode = state.mode;
    let kind = state.kind;
    let target = state.target;
    let action = state.action;
    let rs_items = state.ruleset_items;
    let rs_groups = state.ruleset_groups;
    let open_groups = state.group_sheet.clone();
    let submit = state.clone();
    let cancel = state.clone();
    let preview_cond = state.clone();
    let preview = state.clone();
    surface(SurfaceLevel::Card)
        .p_4()
        .flex_col()
        .gap_3()
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_3()
                .child(tab_bar(
                    vec![
                        ("rules-form", "rules.quick"),
                        ("rules-batch-tab", "rules.batch"),
                    ],
                    mode,
                ))
                .child(
                    button(ButtonVariant::Outline)
                        .test_id("rules-open-groups")
                        .size(ButtonSize::Sm)
                        .prefix(|| icon_sized(Icon::Layers, Tone::Foreground, 13.0).into_element())
                        .label(move || {
                            t!(
                                "rulesets.group.manage_btn",
                                count = rs_groups.with(Vec::len)
                            )
                        })
                        .on_press(move || open_groups.open_panel()),
                ),
        )
        .child(when(
            move || mode.get() == 0,
            move || {
                let enter = enter.clone();
                let kind_items = [
                    ("domain", "rules.domain"),
                    ("suffix", "rules.suffix"),
                    ("keyword", "rules.keyword"),
                    ("cidr", "rules.cidr"),
                    ("process", "rules.process"),
                    ("port", "rules.port"),
                    ("ruleset", "rules.ruleset"),
                ];
                let action_items = [
                    ("proxy", "rules.proxy"),
                    ("direct", "rules.direct"),
                    ("reject", "rules.reject"),
                ];
                view()
                    .flex_col()
                    .gap_3()
                    .child(
                        view()
                            .flex_row()
                            .flex_wrap()
                            .items_start()
                            .gap_3()
                            .child(field().label(|| t!("rules.kind")).control(select(
                                kind_items
                                    .iter()
                                    .map(|(id, key)| SelectItem::new(*id, t!(*key)))
                                    .collect(),
                                move || Some(kind.get().into()),
                                move |v| {
                                    if let Some(v) = v {
                                        kind.set(v.to_string());
                                    }
                                },
                            )))
                            .child(
                                view().flex_1().min_w(px(180.0)).child(
                                    field().label(|| t!("rules.target")).control(
                                        input(target)
                                            .test_id("rules-target")
                                            .accessible_name(|| t!("rules.target"))
                                            .placeholder(move || {
                                                if kind.get() == "ruleset" {
                                                    t!("rules.ruleset_placeholder")
                                                } else {
                                                    "example.com".into()
                                                }
                                            })
                                            .on_change(move |v| target.set(v))
                                            .on_submit(move || enter.save()),
                                    ),
                                ),
                            )
                            .child(field().label(|| t!("rules.action")).control(select(
                                action_items
                                    .iter()
                                    .map(|(id, key)| SelectItem::new(*id, t!(*key)))
                                    .collect(),
                                move || Some(action.get().into()),
                                move |v| {
                                    if let Some(v) = v {
                                        action.set(v.to_string());
                                    }
                                },
                            ))),
                    )
                    .child(when(
                        move || kind.get() == "ruleset",
                        move || ruleset_picker(target, rs_groups, rs_items),
                    ))
            },
        ))
        .child(when(
            move || mode.get() == 1,
            move || {
                field()
                    .label(|| t!("rules.lines"))
                    .hint(|| {
                        "suffix example.com -> proxy\nruleset @direct-cn -> direct\nruleset geosite-cn,geoip-cn -> direct\ncidr 192.168.0.0/16 -> direct\nport 8000-9000 -> reject\n# comment"
                            .into()
                    })
                    .control(
                        textarea(batch)
                            .test_id("rules-batch")
                            .accessible_name(|| t!("rules.lines"))
                            .on_change(move |v| batch.set(v)),
                    )
            },
        ))
        .child(when(
            move || {
                !(preview_cond.mode.get() == 0 && preview_cond.target.get().trim().is_empty())
                    && !preview_cond.source().trim().is_empty()
            },
            move || {
                let preview = preview.clone();
                view()
                    .px_3()
                    .py_2()
                    .rounded(move || theme().radius)
                    .bg(move || theme().muted.opacity(0.45))
                    .border(px(1.0))
                    .border_color(move || theme().border)
                    .child(
                        text(move || match crate::rules::parse_lines(&preview.source()) {
                            Ok(rules) => rules
                                .iter()
                                .take(3)
                                .map(rule_line)
                                .collect::<Vec<_>>()
                                .join("\n"),
                            Err(error) => error.to_string(),
                        })
                        .text_size(px(12.5))
                        .text_color(move || theme().muted_foreground),
                    )
            },
        ))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_3()
                .child(checkbox(form.first, || t!("rules.first"), move |v| {
                    form.first.set(v)
                }))
                .child(view().flex_1())
                .child(
                    button(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .label(|| t!("rules.reset"))
                        .on_press(move || {
                            cancel.editing.set(None);
                            cancel.target.set(String::new());
                            cancel.batch.set(String::new());
                            cancel.error.set(None);
                        }),
                )
                .child(
                    button(ButtonVariant::Primary)
                        .size(ButtonSize::Md)
                        .label(move || {
                            if submit.editing.get().is_some() {
                                t!("rules.save_edit")
                            } else {
                                t!("rules.add")
                            }
                        })
                        .test_id("rules-save")
                        .on_press({
                            let submit = state.clone();
                            move || submit.save()
                        }),
                ),
        )
}

fn ruleset_picker(
    target: Signal<String>,
    groups: Signal<Vec<RuleSetGroup>>,
    items: Signal<Vec<RuleSetRow>>,
) -> AnyElement {
    let empty = move || groups.with(Vec::is_empty) && items.with(Vec::is_empty);
    view()
        .p_3()
        .rounded(move || theme().radius)
        .bg(move || theme().muted.opacity(0.35))
        .border(px(1.0))
        .border_color(move || theme().border)
        .flex_col()
        .gap_2()
        .child(
            text(|| t!("rules.ruleset_picker_hint"))
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(when(empty, || {
            text(|| t!("rules.ruleset_picker_empty"))
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground)
        }))
        .child(when(
            move || !groups.with(Vec::is_empty),
            move || {
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(each(
                        move || groups.get(),
                        |g: &RuleSetGroup| g.id.clone(),
                        move |group, _| {
                            let g = group.get_untracked();
                            let token = g.ref_token();
                            let token_check = token.clone();
                            let token_id = g.tag.clone();
                            let label = format!("{} ({})", token, g.member_tags.len());
                            button(ButtonVariant::Secondary)
                                .test_id(format!("rules-pick-group-{token_id}"))
                                .size(ButtonSize::Sm)
                                .selected(move || {
                                    target_contains_token(&target.get(), &token_check)
                                })
                                .label(move || label.clone())
                                .on_press(move || toggle_target_token(target, &token))
                        },
                    ))
            },
        ))
        .child(when(
            move || !items.with(Vec::is_empty),
            move || {
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(each(
                        move || items.get(),
                        |row: &RuleSetRow| row.entry.id.clone(),
                        move |row, _| {
                            let tag = row.get_untracked().entry.tag;
                            let tag_check = tag.clone();
                            let tag_label = tag.clone();
                            let tag_id = tag.clone();
                            button(ButtonVariant::Outline)
                                .test_id(format!("rules-pick-set-{tag_id}"))
                                .size(ButtonSize::Sm)
                                .selected(move || target_contains_token(&target.get(), &tag_check))
                                .label(move || tag_label.clone())
                                .on_press(move || toggle_target_token(target, &tag))
                        },
                    ))
            },
        ))
        .into_element()
}

fn target_contains_token(raw: &str, token: &str) -> bool {
    lanpath_client::rules::parse_rule_set_tags(raw)
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(token))
}

fn toggle_target_token(target: Signal<String>, token: &str) {
    let mut tags = lanpath_client::rules::parse_rule_set_tags(&target.get());
    if let Some(pos) = tags.iter().position(|t| t.eq_ignore_ascii_case(token)) {
        tags.remove(pos);
    } else {
        tags.push(token.to_owned());
    }
    target.set(tags.join(", "));
}
