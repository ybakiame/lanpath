//! `dns.json` 切片可视化工作台：基础策略、可编辑上游列表、国内外推荐 DNS 与规则集分流。

use std::collections::HashMap;

use lanpath_client::singbox_overwrite::SingboxSliceKind;
use nsh_base::{SelectItem, theme};
use nsh_core::prelude::*;
use serde_json::{Value, json};

use super::dns_presets_panel::recommended_dns_panel;
use super::dns_servers::{
    DnsEditorDraft, ProbeState, dns_server_editor, dns_server_row, probe_input_from_server,
    spawn_probe,
};
use super::forms::{mutate_section, parse_active_json, read_bool, read_str, toggle_row};
use super::state::OverwriteState;
use crate::rulesets::{RuleSetsService, RuleSetsSnapshot};
use crate::rulesets_view::picker::{
    PickerIds, contains_token, extract_used_rule_sets, ruleset_picker_popover,
};
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::feedback::show_toast;
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::input::{field, input};
use crate::ui::select::select;

pub(super) fn dns_form(state: OverwriteState) -> impl IntoElement {
    let owner = Owner::current();
    let final_sig = signal(read_str(&state, "/dns/final", "proxy"));
    let sync_state = state.clone();
    effect(move || {
        if sync_state.active_slice.get() == SingboxSliceKind::Dns {
            let cur = read_str(&sync_state, "/dns/final", "");
            if final_sig.get_untracked() != cur {
                final_sig.set(cur);
            }
        }
    });

    let probes = signal(HashMap::<String, ProbeState>::new());
    let probe_tasks = stored(Vec::<Spawned<()>>::new());
    let draft = DnsEditorDraft::new();
    let rs_snapshot = RuleSetsService::open()
        .ok()
        .and_then(|s| s.list_snapshot().ok())
        .unwrap_or_default();

    let strategies = ["prefer_ipv4", "prefer_ipv6", "ipv4_only", "ipv6_only"];
    let strat_read = state.clone();
    let strat_write = state.clone();
    let final_write = state.clone();
    let rev_read = state.clone();
    let rev_write = state.clone();
    let ind_read = state.clone();
    let ind_write = state.clone();
    let servers_state = state.clone();
    let probe_all_state = state.clone();
    let row_state = state.clone();
    let save_draft_state = state.clone();
    let presets_state = state.clone();
    let dns_rules_state = state;

    let strat_memo = computed(move || read_str(&strat_read, "/dns/strategy", "prefer_ipv4"));
    let servers_memo = computed(move || {
        parse_active_json(&servers_state)
            .and_then(|v| v.pointer("/dns/servers").and_then(Value::as_array).cloned())
            .unwrap_or_default()
    });

    view()
        .flex_col()
        .gap_5()
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_start()
                .gap_4()
                .child(
                    field()
                        .label(|| t!("overwrite.dns.strategy"))
                        .control(select(
                            strategies.iter().map(|s| SelectItem::new(*s, *s)).collect(),
                            move || Some(strat_memo.get().into()),
                            move |v| {
                                if let Some(strategy) = v {
                                    mutate_section(&strat_write, SingboxSliceKind::Dns, |s| {
                                        if let Some(obj) = s.as_object_mut() {
                                            obj.insert(
                                                "strategy".into(),
                                                Value::String(strategy.to_string()),
                                            );
                                        }
                                    });
                                }
                            },
                        )),
                )
                .child(
                    view().w(px(200.0)).child(
                        field().label(|| t!("overwrite.dns.final")).control(
                            input(final_sig)
                                .test_id("overwrite-dns-final")
                                .accessible_name(|| t!("overwrite.dns.final"))
                                .on_change(move |val| {
                                    final_sig.set(val.clone());
                                    mutate_section(&final_write, SingboxSliceKind::Dns, |s| {
                                        if let Some(obj) = s.as_object_mut() {
                                            obj.insert("final".into(), Value::String(val));
                                        }
                                    });
                                }),
                        ),
                    ),
                ),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_6()
                .child(toggle_row(
                    move || read_bool(&rev_read, "/dns/reverse_mapping", true),
                    || t!("overwrite.dns.reverse_mapping"),
                    "overwrite-dns-reverse",
                    move |v| {
                        mutate_section(&rev_write, SingboxSliceKind::Dns, |s| {
                            if let Some(obj) = s.as_object_mut() {
                                obj.insert("reverse_mapping".into(), Value::Bool(v));
                            }
                        });
                    },
                ))
                .child(toggle_row(
                    move || read_bool(&ind_read, "/dns/independent_cache", false),
                    || t!("overwrite.dns.independent_cache"),
                    "overwrite-dns-independent",
                    move |v| {
                        mutate_section(&ind_write, SingboxSliceKind::Dns, |s| {
                            if let Some(obj) = s.as_object_mut() {
                                obj.insert("independent_cache".into(), Value::Bool(v));
                            }
                        });
                    },
                )),
        )
        .child(
            view()
                .flex_col()
                .gap_2()
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            text(|| t!("overwrite.dns.servers"))
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM),
                        )
                        .child(
                            button(ButtonVariant::Secondary)
                                .test_id("overwrite-dns-probe-all")
                                .size(ButtonSize::Sm)
                                .prefix(|| {
                                    icon_sized(Icon::Activity, Tone::Foreground, 13.0)
                                        .into_element()
                                })
                                .label(|| t!("overwrite.dns.probe_all"))
                                .on_press(move || {
                                    let servers = parse_active_json(&probe_all_state)
                                        .and_then(|v| {
                                            v.pointer("/dns/servers")
                                                .and_then(Value::as_array)
                                                .cloned()
                                        })
                                        .unwrap_or_default();
                                    for srv in servers {
                                        if let Some((key, input)) = probe_input_from_server(&srv) {
                                            spawn_probe(owner, probe_tasks, probes, key, input);
                                        }
                                    }
                                }),
                        ),
                )
                .child(each(
                    servers_memo,
                    |s: &Value| {
                        format!(
                            "{}:{}:{}:{}",
                            s.get("tag").and_then(Value::as_str).unwrap_or(""),
                            s.get("type").and_then(Value::as_str).unwrap_or(""),
                            s.get("server").and_then(Value::as_str).unwrap_or(""),
                            s.get("detour").and_then(Value::as_str).unwrap_or("")
                        )
                    },
                    move |item, index| {
                        dns_server_row(
                            row_state.clone(),
                            draft,
                            owner,
                            probe_tasks,
                            probes,
                            item,
                            index,
                        )
                    },
                ))
                .child(dns_server_editor(save_draft_state, draft)),
        )
        .child(recommended_dns_panel(
            presets_state,
            draft,
            owner,
            probe_tasks,
            probes,
        ))
        .child(dns_ruleset_rules_section(dns_rules_state, rs_snapshot))
}

fn dns_ruleset_rules_section(
    state: OverwriteState,
    snapshot: RuleSetsSnapshot,
) -> impl IntoElement {
    let rules_state = state.clone();
    let add_state = state.clone();
    let row_state = state;

    let rules_memo = computed(move || {
        parse_active_json(&rules_state)
            .and_then(|v| v.pointer("/dns/rules").and_then(Value::as_array).cloned())
            .unwrap_or_default()
    });
    let used_rs_memo = computed(move || extract_used_rule_sets(&rules_memo.get()));

    let rs_groups = signal(snapshot.groups);
    let rs_items = signal(snapshot.items);
    let selected_rs = signal(String::new());
    let selected_srv = signal("local".to_owned());

    view()
        .flex_col()
        .gap_2()
        .child(
            text(|| t!("overwrite.dns.rules_title"))
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM),
        )
        .child(
            text(|| t!("overwrite.dns.rules_hint"))
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(each(
            rules_memo,
            |r: &Value| r.to_string(),
            move |rule, idx| {
                let r = rule.get_untracked();
                let st = row_state.clone();
                let rs_label = r
                    .get("rule_set")
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        Value::Array(arr) => arr
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", "),
                        _ => v.to_string(),
                    })
                    .unwrap_or_else(|| r.to_string());
                let srv = r
                    .get("server")
                    .and_then(Value::as_str)
                    .unwrap_or("—")
                    .to_owned();
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
                        text(format!("rule_set: {rs_label}  →  {srv}"))
                            .text_size(px(12.5))
                            .text_color(move || theme().foreground),
                    )
                    .child(
                        button(ButtonVariant::Ghost)
                            .size(ButtonSize::Sm)
                            .label(|| t!("rules.delete"))
                            .on_press(move || {
                                mutate_section(&st, SingboxSliceKind::Dns, |s| {
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
                        .min_w(px(220.0))
                        .child(ruleset_picker_popover(
                            PickerIds {
                                trigger: "overwrite-dns-picker-trigger",
                                root: "overwrite-dns-picker",
                                tabs: "overwrite-dns-picker-tabs",
                                tab_single: "overwrite-dns-picker-tab-single",
                                tab_group: "overwrite-dns-picker-tab-group",
                                search: "overwrite-dns-picker-search",
                                pick_prefix: "overwrite-dns",
                            },
                            rs_groups,
                            rs_items,
                            used_rs_memo,
                            selected_rs,
                            true,
                            move |tok| selected_rs.set(tok),
                        )),
                )
                .child(
                    view().w(px(160.0)).child(
                        input(selected_srv)
                            .test_id("overwrite-dns-rule-server")
                            .placeholder(|| "local / proxy".into())
                            .on_change(move |v| selected_srv.set(v)),
                    ),
                )
                .child(
                    button(ButtonVariant::Outline)
                        .test_id("overwrite-dns-rule-add")
                        .size(ButtonSize::Md)
                        .prefix(|| icon_sized(Icon::Plus, Tone::Muted, 14.0))
                        .label(|| t!("overwrite.dns.add_rule"))
                        .on_press(move || {
                            let target = selected_rs.get().trim().to_owned();
                            let srv = selected_srv.get().trim().to_owned();
                            if target.is_empty() || srv.is_empty() {
                                return;
                            }
                            if contains_token(&used_rs_memo.get_untracked(), &target) {
                                show_toast(t!(
                                    "rulesets.picker.already_configured",
                                    target = target
                                ));
                                return;
                            }
                            mutate_section(&add_state, SingboxSliceKind::Dns, |s| {
                                if let Some(obj) = s.as_object_mut() {
                                    let arr = obj
                                        .entry("rules")
                                        .or_insert_with(|| Value::Array(Vec::new()));
                                    if let Some(arr) = arr.as_array_mut() {
                                        arr.push(json!({
                                            "rule_set": target,
                                            "server": srv
                                        }));
                                    }
                                }
                            });
                            selected_rs.set(String::new());
                        }),
                ),
        )
}
