//! `outbounds.json` 策略组内联新增 / 编辑表单：支持配置分组类型、默认成员、urltest 参数、
//! 引用其他策略组/出站成员、开启 `$nodes` 并附加 `_filter` 包含/排除正则及实时匹配预览。

use lanpath_client::singbox_overwrite::{
    NODES_PLACEHOLDER, NodeFilterSpec, OUTBOUND_FILTER_KEY, SingboxSliceKind,
};
use nsh_base::{SelectItem, theme};
use nsh_core::prelude::*;
use serde_json::{Map, Value, json};

use super::forms::mutate_section;
use super::outbounds_form::OutboundEditorDraft;
use super::state::OverwriteState;
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::input::{field, input};
use crate::ui::layout::{AlertTone, SurfaceLevel, alert_banner, section_title, surface};
use crate::ui::select::{checkbox, select};

pub(super) fn outbound_editor_card(
    state: OverwriteState,
    draft: OutboundEditorDraft,
    items_memo: Memo<Vec<Value>>,
    cached_nodes: Vec<String>,
) -> AnyElement {
    let available_tags = computed(move || {
        let self_tag = draft.tag.get();
        items_memo.with(|list| {
            list.iter()
                .filter_map(|v| v.get("tag").and_then(Value::as_str).map(str::to_owned))
                .filter(|t| !t.is_empty() && t != &self_tag)
                .collect::<Vec<_>>()
        })
    });
    let total_nodes = cached_nodes.len();
    let preview_nodes = cached_nodes;
    let filter_preview = computed(move || {
        if !draft.include_nodes.get() {
            return None;
        }
        let spec = NodeFilterSpec {
            include: draft.filter_include.get(),
            exclude: draft.filter_exclude.get(),
        };
        match spec.compile() {
            Ok(compiled) => {
                if total_nodes == 0 {
                    Some((true, t!("overwrite.outbounds.filter_no_cache")))
                } else {
                    let matched = preview_nodes.iter().filter(|n| compiled.matches(n)).count();
                    Some((
                        true,
                        t!(
                            "overwrite.outbounds.filter_preview",
                            matched = matched,
                            total = total_nodes
                        ),
                    ))
                }
            }
            Err(err) => Some((false, err.to_string())),
        }
    });

    let save_state = state;

    surface(SurfaceLevel::Card)
        .p_4()
        .flex_col()
        .gap_3()
        .border(px(1.5))
        .border_color(move || theme().primary.opacity(0.45))
        .child(section_title(move || {
            if draft.editing_index.get().is_some() {
                t!("overwrite.outbounds.edit_title")
            } else {
                t!("overwrite.outbounds.add_title")
            }
        }))
        .child(when(
            move || draft.error.get().is_some(),
            move || {
                alert_banner(
                    AlertTone::Danger,
                    text(move || draft.error.get().unwrap_or_default())
                        .text_size(px(12.0))
                        .text_color(move || theme().destructive),
                )
            },
        ))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_start()
                .gap_3()
                .child(
                    view().w(px(200.0)).child(
                        field().label(|| t!("overwrite.outbounds.new_tag")).control(
                            input(draft.tag)
                                .test_id("overwrite-outbound-draft-tag")
                                .placeholder(|| "AI / HK / Streaming".to_owned())
                                .on_change(move |v| draft.tag.set(v)),
                        ),
                    ),
                )
                .child(
                    view().w(px(150.0)).child(
                        field()
                            .label(|| t!("overwrite.outbounds.type"))
                            .control(select(
                                vec![
                                    SelectItem::new("selector", "selector"),
                                    SelectItem::new("urltest", "urltest"),
                                ],
                                move || Some(draft.kind.get().into()),
                                move |v| {
                                    if let Some(v) = v {
                                        draft.kind.set(v.to_string());
                                    }
                                },
                            )),
                    ),
                )
                .child(when(
                    move || draft.kind.get() == "selector",
                    move || {
                        view().w(px(180.0)).child(
                            field().label(|| t!("overwrite.outbounds.default")).control(
                                input(draft.default_member)
                                    .test_id("overwrite-outbound-draft-default")
                                    .placeholder(|| t!("overwrite.outbounds.default_placeholder"))
                                    .on_change(move |v| draft.default_member.set(v)),
                            ),
                        )
                    },
                )),
        )
        .child(when(
            move || draft.kind.get() == "urltest",
            move || {
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_start()
                    .gap_3()
                    .child(
                        view().flex_1().min_w(px(220.0)).child(
                            field()
                                .label(|| t!("overwrite.outbounds.test_url"))
                                .control(
                                    input(draft.url)
                                        .test_id("overwrite-outbound-draft-url")
                                        .on_change(move |v| draft.url.set(v)),
                                ),
                        ),
                    )
                    .child(
                        view().w(px(110.0)).child(
                            field()
                                .label(|| t!("overwrite.outbounds.interval"))
                                .control(
                                    input(draft.interval)
                                        .test_id("overwrite-outbound-draft-interval")
                                        .placeholder(|| "5m".to_owned())
                                        .on_change(move |v| draft.interval.set(v)),
                                ),
                        ),
                    )
                    .child(
                        view().w(px(110.0)).child(
                            field()
                                .label(|| t!("overwrite.outbounds.tolerance"))
                                .control(
                                    input(draft.tolerance)
                                        .test_id("overwrite-outbound-draft-tolerance")
                                        .placeholder(|| "150".to_owned())
                                        .on_change(move |v| draft.tolerance.set(v)),
                                ),
                        ),
                    )
            },
        ))
        .child(checkbox(
            draft.include_nodes,
            || t!("overwrite.outbounds.include_nodes"),
            move |v| draft.include_nodes.set(v),
        ))
        .child(when(
            move || draft.include_nodes.get(),
            move || {
                view()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded(move || theme().radius)
                    .bg(move || theme().muted.opacity(0.3))
                    .border(px(1.0))
                    .border_color(move || theme().border)
                    .child(
                        view()
                            .flex_row()
                            .flex_wrap()
                            .items_start()
                            .gap_3()
                            .child(
                                view().flex_1().min_w(px(200.0)).child(
                                    field()
                                        .label(|| t!("overwrite.outbounds.filter_include"))
                                        .control(
                                            input(draft.filter_include)
                                                .test_id("overwrite-outbound-filter-include")
                                                .placeholder(|| "香港|HK|Hong\\s*Kong".to_owned())
                                                .on_change(move |v| draft.filter_include.set(v)),
                                        ),
                                ),
                            )
                            .child(
                                view().flex_1().min_w(px(200.0)).child(
                                    field()
                                        .label(|| t!("overwrite.outbounds.filter_exclude"))
                                        .control(
                                            input(draft.filter_exclude)
                                                .test_id("overwrite-outbound-filter-exclude")
                                                .placeholder(|| "倍率|过期|剩余流量".to_owned())
                                                .on_change(move |v| draft.filter_exclude.set(v)),
                                        ),
                                ),
                            ),
                    )
                    .child(when(
                        move || filter_preview.get().is_some(),
                        move || {
                            text(move || {
                                filter_preview.get().map(|(_, msg)| msg).unwrap_or_default()
                            })
                            .text_size(px(12.0))
                            .text_color(move || {
                                if filter_preview.get().is_some_and(|(ok, _)| !ok) {
                                    theme().destructive
                                } else {
                                    theme().muted_foreground
                                }
                            })
                        },
                    ))
            },
        ))
        .child(
            view()
                .flex_col()
                .gap_2()
                .child(
                    text(|| t!("overwrite.outbounds.members_label"))
                        .text_size(px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(move || theme().foreground),
                )
                .child(view().flex_row().flex_wrap().gap_2().child(each(
                    available_tags,
                    |tag: &String| tag.clone(),
                    move |tag_sig, _| {
                        let tag = tag_sig.get_untracked();
                        let tag_lbl = tag.clone();
                        let tag_chk = tag.clone();
                        let tag_id = tag.clone();
                        let selected = move || {
                            draft
                                .members
                                .with(|list| list.iter().any(|t| t == &tag_chk))
                        };
                        button(ButtonVariant::Outline)
                            .test_id(format!("overwrite-outbound-member-{tag_id}"))
                            .size(ButtonSize::Sm)
                            .selected(selected)
                            .label(move || tag_lbl.clone())
                            .on_press(move || draft.toggle_member(&tag))
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
                        .test_id("overwrite-outbound-editor-cancel")
                        .size(ButtonSize::Sm)
                        .label(|| t!("common.cancel"))
                        .on_press(move || draft.close()),
                )
                .child(
                    button(ButtonVariant::Primary)
                        .test_id("overwrite-outbound-editor-save")
                        .size(ButtonSize::Sm)
                        .prefix(|| icon_sized(Icon::Check, Tone::OnPrimary, 13.0))
                        .label(|| t!("overwrite.outbounds.save_group"))
                        .on_press(move || save_outbound_draft(&save_state, draft)),
                ),
        )
        .into_element()
}

fn save_outbound_draft(state: &OverwriteState, draft: OutboundEditorDraft) {
    let tag = draft.tag.get().trim().to_owned();
    if tag.is_empty() {
        draft
            .error
            .set(Some(t!("overwrite.outbounds.error_empty_tag")));
        return;
    }
    let filter = NodeFilterSpec {
        include: draft.filter_include.get(),
        exclude: draft.filter_exclude.get(),
    };
    if draft.include_nodes.get()
        && let Err(err) = filter.compile()
    {
        draft.error.set(Some(err.to_string()));
        return;
    }

    let mut outbounds: Vec<Value> = draft
        .members
        .get()
        .into_iter()
        .filter(|m| m != &tag)
        .map(Value::String)
        .collect();
    if draft.include_nodes.get() {
        outbounds.push(Value::String(NODES_PLACEHOLDER.to_owned()));
    }
    if outbounds.is_empty() {
        draft
            .error
            .set(Some(t!("overwrite.outbounds.error_empty_members")));
        return;
    }

    let kind = draft.kind.get();
    let mut obj = Map::new();
    obj.insert("type".to_owned(), Value::String(kind.clone()));
    obj.insert("tag".to_owned(), Value::String(tag.clone()));
    obj.insert("outbounds".to_owned(), Value::Array(outbounds));
    if kind == "urltest" {
        let url = draft.url.get().trim().to_owned();
        if !url.is_empty() {
            obj.insert("url".to_owned(), Value::String(url));
        }
        let interval = draft.interval.get().trim().to_owned();
        if !interval.is_empty() {
            obj.insert("interval".to_owned(), Value::String(interval));
        }
        if let Ok(tol) = draft.tolerance.get().trim().parse::<u64>() {
            obj.insert("tolerance".to_owned(), json!(tol));
        }
    } else {
        let def = draft.default_member.get().trim().to_owned();
        if !def.is_empty() {
            obj.insert("default".to_owned(), Value::String(def));
        }
    }
    if draft.include_nodes.get()
        && let Some(filter_val) = filter.to_json_value()
    {
        obj.insert(OUTBOUND_FILTER_KEY.to_owned(), filter_val);
    }

    let editing_idx = draft.editing_index.get();
    let mut duplicate = false;
    mutate_section(state, SingboxSliceKind::Outbounds, |section| {
        let Some(arr) = section.as_array_mut() else {
            return;
        };
        let clash = arr.iter().enumerate().any(|(i, existing)| {
            Some(i) != editing_idx && existing.get("tag").and_then(Value::as_str) == Some(&tag)
        });
        if clash {
            duplicate = true;
            return;
        }
        let value = Value::Object(obj);
        if let Some(idx) = editing_idx
            && idx < arr.len()
        {
            arr[idx] = value;
        } else {
            arr.push(value);
        }
    });
    if duplicate {
        draft
            .error
            .set(Some(t!("overwrite.outbounds.error_dup_tag", tag = tag)));
    } else {
        draft.close();
    }
}
