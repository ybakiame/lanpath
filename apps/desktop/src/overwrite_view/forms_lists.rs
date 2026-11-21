//! sing-box 列表型切片 GUI 表单：`route.json`（路由策略 + 规则集分流）与
//! `outbounds.json`（动态 `$nodes` 分组列表）。

use lanpath_client::singbox_overwrite::{NODES_PLACEHOLDER, SingboxSliceKind};
use nsh_base::{SelectItem, theme};
use nsh_core::prelude::*;
use serde_json::{Value, json};

use super::forms::{mutate_section, parse_active_json, read_bool, read_str, toggle_row};
use super::state::OverwriteState;
use crate::rulesets::RuleSetsService;
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::input::{field, input};
use crate::ui::layout::{AlertTone, alert_banner};
use crate::ui::select::select;

pub(super) fn route_form(state: OverwriteState) -> impl IntoElement {
    let final_sig = signal(read_str(&state, "/route/final", "final"));
    let resolver_sig = signal(read_str(
        &state,
        "/route/default_domain_resolver/server",
        "local",
    ));
    let sync_state = state.clone();
    effect(move || {
        if sync_state.active_slice.get() == SingboxSliceKind::Route {
            let f = read_str(&sync_state, "/route/final", "");
            if final_sig.get_untracked() != f {
                final_sig.set(f);
            }
            let r = read_str(&sync_state, "/route/default_domain_resolver/server", "");
            if resolver_sig.get_untracked() != r {
                resolver_sig.set(r);
            }
        }
    });
    let final_write = state.clone();
    let resolver_write = state.clone();
    let auto_read = state.clone();
    let auto_write = state.clone();
    let rules_list_state = state.clone();
    let rules_add_state = state.clone();
    let rules_del_state = state.clone();
    let stats_state = state;

    let rs_snapshot = RuleSetsService::open()
        .ok()
        .and_then(|s| s.list_snapshot().ok())
        .unwrap_or_default();
    let mut rs_options: Vec<SelectItem> = rs_snapshot
        .groups
        .iter()
        .map(|g| {
            let tok = g.ref_token();
            SelectItem::new(tok.clone(), format!("{tok} · {}", g.display_name()))
        })
        .collect();
    for row in &rs_snapshot.items {
        rs_options.push(SelectItem::new(
            row.entry.tag.clone(),
            format!("{} · {}", row.entry.tag, row.entry.display_name()),
        ));
    }
    let selected_rs = signal(
        rs_options
            .first()
            .map(|o| o.value.to_string())
            .unwrap_or_default(),
    );
    let selected_out = signal("direct".to_owned());
    let route_rules_memo = computed(move || {
        parse_active_json(&rules_list_state)
            .and_then(|v| v.pointer("/route/rules").and_then(Value::as_array).cloned())
            .unwrap_or_default()
    });

    view()
        .flex_col()
        .gap_4()
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_start()
                .gap_4()
                .child(
                    view().w(px(220.0)).child(
                        field().label(|| t!("overwrite.route.final")).control(
                            input(final_sig)
                                .test_id("overwrite-route-final")
                                .accessible_name(|| t!("overwrite.route.final"))
                                .on_change(move |val| {
                                    final_sig.set(val.clone());
                                    mutate_section(&final_write, SingboxSliceKind::Route, |s| {
                                        if let Some(obj) = s.as_object_mut() {
                                            obj.insert("final".into(), Value::String(val));
                                        }
                                    });
                                }),
                        ),
                    ),
                )
                .child(
                    view().w(px(240.0)).child(
                        field().label(|| t!("overwrite.route.resolver")).control(
                            input(resolver_sig)
                                .test_id("overwrite-route-resolver")
                                .accessible_name(|| t!("overwrite.route.resolver"))
                                .on_change(move |val| {
                                    resolver_sig.set(val.clone());
                                    mutate_section(&resolver_write, SingboxSliceKind::Route, |s| {
                                        if let Some(obj) = s.as_object_mut() {
                                            if val.trim().is_empty() {
                                                obj.remove("default_domain_resolver");
                                            } else {
                                                obj.insert(
                                                    "default_domain_resolver".into(),
                                                    json!({ "server": val }),
                                                );
                                            }
                                        }
                                    });
                                }),
                        ),
                    ),
                ),
        )
        .child(toggle_row(
            move || read_bool(&auto_read, "/route/auto_detect_interface", true),
            || t!("overwrite.route.auto_detect"),
            "overwrite-route-auto-detect",
            move |v| {
                mutate_section(&auto_write, SingboxSliceKind::Route, |s| {
                    if let Some(obj) = s.as_object_mut() {
                        obj.insert("auto_detect_interface".into(), Value::Bool(v));
                    }
                });
            },
        ))
        .child(
            view()
                .flex_col()
                .gap_2()
                .child(
                    text(|| t!("overwrite.route.rulesets_title"))
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM),
                )
                .child(each(
                    route_rules_memo,
                    |r: &Value| r.to_string(),
                    move |rule, idx| {
                        let r = rule.get_untracked();
                        let st = rules_del_state.clone();
                        let summary = format_route_rule_summary(&r);
                        view()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .px_3()
                            .py_2()
                            .rounded(move || theme().radius)
                            .bg(move || theme().muted.opacity(0.35))
                            .border(px(1.0))
                            .border_color(move || theme().border)
                            .child(
                                text(summary)
                                    .text_size(px(12.5))
                                    .text_color(move || theme().foreground),
                            )
                            .child(
                                button(ButtonVariant::Ghost)
                                    .size(ButtonSize::Sm)
                                    .label(|| t!("rules.delete"))
                                    .on_press(move || {
                                        mutate_section(&st, SingboxSliceKind::Route, |s| {
                                            let i = idx.get();
                                            if let Some(arr) =
                                                s.get_mut("rules").and_then(Value::as_array_mut)
                                                && i < arr.len()
                                            {
                                                arr.remove(i);
                                            }
                                        });
                                    }),
                            )
                    },
                ))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(
                            view()
                                .flex_1()
                                .min_w(px(200.0))
                                .child(if rs_options.is_empty() {
                                    input(selected_rs)
                                        .test_id("overwrite-route-rule-rs")
                                        .placeholder(|| "@group / geosite-cn".into())
                                        .on_change(move |v| selected_rs.set(v))
                                        .into_element()
                                } else {
                                    select(
                                        rs_options,
                                        move || Some(selected_rs.get().into()),
                                        move |v| {
                                            if let Some(v) = v {
                                                selected_rs.set(v.to_string());
                                            }
                                        },
                                    )
                                    .into_element()
                                }),
                        )
                        .child(select(
                            vec![
                                SelectItem::new("direct", t!("rules.direct")),
                                SelectItem::new("proxy", t!("rules.proxy")),
                                SelectItem::new("reject", t!("rules.reject")),
                            ],
                            move || Some(selected_out.get().into()),
                            move |v| {
                                if let Some(v) = v {
                                    selected_out.set(v.to_string());
                                }
                            },
                        ))
                        .child(
                            button(ButtonVariant::Outline)
                                .test_id("overwrite-route-rule-add")
                                .size(ButtonSize::Md)
                                .prefix(|| icon_sized(Icon::Plus, Tone::Muted, 14.0))
                                .label(|| t!("overwrite.route.add_rule"))
                                .on_press(move || {
                                    let target = selected_rs.get().trim().to_owned();
                                    if target.is_empty() {
                                        return;
                                    }
                                    let out = selected_out.get();
                                    let entry = if out == "reject" {
                                        json!({ "rule_set": target, "action": "reject" })
                                    } else {
                                        json!({ "rule_set": target, "outbound": out })
                                    };
                                    mutate_section(
                                        &rules_add_state,
                                        SingboxSliceKind::Route,
                                        |s| {
                                            if let Some(obj) = s.as_object_mut() {
                                                let arr = obj
                                                    .entry("rules")
                                                    .or_insert_with(|| Value::Array(Vec::new()));
                                                if let Some(arr) = arr.as_array_mut() {
                                                    arr.push(entry);
                                                }
                                            }
                                        },
                                    );
                                }),
                        ),
                ),
        )
        .child(alert_banner(
            AlertTone::Info,
            text(move || {
                let rules_len = parse_active_json(&stats_state)
                    .and_then(|v| {
                        v.pointer("/route/rules")
                            .and_then(Value::as_array)
                            .map(Vec::len)
                    })
                    .unwrap_or(0);
                let sets_len = parse_active_json(&stats_state)
                    .and_then(|v| {
                        v.pointer("/route/rule_set")
                            .and_then(Value::as_array)
                            .map(Vec::len)
                    })
                    .unwrap_or(0);
                t!(
                    "overwrite.route.summary",
                    rules = rules_len,
                    rule_sets = sets_len
                )
            })
            .text_size(px(12.5))
            .text_color(move || theme().foreground),
        ))
}

fn format_route_rule_summary(rule: &Value) -> String {
    let target = rule
        .get("rule_set")
        .map(|v| match v {
            Value::String(s) => format!("rule_set: {s}"),
            Value::Array(arr) => format!(
                "rule_set: {}",
                arr.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            _ => v.to_string(),
        })
        .unwrap_or_else(|| rule.to_string());
    let dest = rule
        .get("outbound")
        .and_then(Value::as_str)
        .or_else(|| rule.get("action").and_then(Value::as_str))
        .unwrap_or("—");
    format!("{target}  →  {dest}")
}

pub(super) fn outbounds_form(state: OverwriteState) -> impl IntoElement {
    let list_state = state.clone();
    let items_memo = computed(move || {
        parse_active_json(&list_state)
            .and_then(|v| v.get("outbounds").and_then(Value::as_array).cloned())
            .unwrap_or_default()
    });
    let new_tag = signal(String::new());
    let new_kind = signal("selector".to_owned());
    let row_state = state.clone();
    let add_state = state;

    view()
        .flex_col()
        .gap_3()
        .child(alert_banner(
            AlertTone::Info,
            text(|| t!("overwrite.outbounds.placeholder_hint"))
                .text_size(px(12.5))
                .text_color(move || theme().foreground),
        ))
        .child(each(
            items_memo,
            |item: &Value| {
                item.get("tag")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            },
            move |item, index| outbound_row(row_state.clone(), item, index),
        ))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    view().w(px(180.0)).child(
                        input(new_tag)
                            .test_id("overwrite-outbound-new-tag")
                            .placeholder(|| t!("overwrite.outbounds.new_tag"))
                            .accessible_name(|| t!("overwrite.outbounds.new_tag"))
                            .on_change(move |v| new_tag.set(v)),
                    ),
                )
                .child(select(
                    ["selector", "urltest"]
                        .iter()
                        .map(|k| SelectItem::new(*k, *k))
                        .collect(),
                    move || Some(new_kind.get().into()),
                    move |v| {
                        if let Some(v) = v {
                            new_kind.set(v.to_string());
                        }
                    },
                ))
                .child(
                    button(ButtonVariant::Outline)
                        .size(ButtonSize::Md)
                        .test_id("overwrite-outbound-add")
                        .prefix(|| icon_sized(Icon::Plus, Tone::Muted, 14.0))
                        .label(|| t!("overwrite.outbounds.add_group"))
                        .on_press(move || {
                            let tag = new_tag.get().trim().to_owned();
                            if tag.is_empty() {
                                return;
                            }
                            let kind = new_kind.get();
                            let entry = if kind == "urltest" {
                                json!({
                                    "type": "urltest",
                                    "tag": tag,
                                    "outbounds": [NODES_PLACEHOLDER],
                                    "url": "https://www.gstatic.com/generate_204",
                                    "interval": "5m",
                                    "tolerance": 150
                                })
                            } else {
                                json!({
                                    "type": "selector",
                                    "tag": tag,
                                    "outbounds": [NODES_PLACEHOLDER, "direct"]
                                })
                            };
                            mutate_section(&add_state, SingboxSliceKind::Outbounds, |s| {
                                if let Some(arr) = s.as_array_mut() {
                                    arr.push(entry);
                                }
                            });
                            new_tag.set(String::new());
                        }),
                ),
        )
}

fn outbound_row(
    state: OverwriteState,
    item: ReadSignal<Value>,
    index: ReadSignal<usize>,
) -> impl IntoElement {
    let obj = item.get();
    let tag = obj
        .get("tag")
        .and_then(Value::as_str)
        .unwrap_or("—")
        .to_owned();
    let kind = obj
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let has_nodes = obj
        .get("outbounds")
        .and_then(Value::as_array)
        .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some(NODES_PLACEHOLDER)));
    let members_count = obj
        .get("outbounds")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap_2()
        .px_3()
        .py_2()
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
                    crate::ui::BadgeVariant::Default,
                    move || kind.clone(),
                ))
                .child(
                    text(move || tag.clone())
                        .text_size(px(13.5))
                        .font_weight(FontWeight::MEDIUM),
                )
                .child(when(
                    move || has_nodes,
                    || crate::ui::badge(crate::ui::BadgeVariant::Success, || "$nodes".to_owned()),
                ))
                .child(when(
                    move || members_count > 0,
                    move || {
                        text(move || format!("{members_count} 项"))
                            .text_size(px(12.0))
                            .text_color(move || theme().muted_foreground)
                    },
                )),
        )
        .child(
            button(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .label(|| t!("rules.delete"))
                .on_press(move || {
                    mutate_section(&state, SingboxSliceKind::Outbounds, |s| {
                        let idx = index.get();
                        if let Some(arr) = s.as_array_mut()
                            && idx < arr.len()
                        {
                            arr.remove(idx);
                        }
                    });
                }),
        )
}
