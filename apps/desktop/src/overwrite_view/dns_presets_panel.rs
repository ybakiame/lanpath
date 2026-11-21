//! 国内外推荐 DNS 预设面板：支持延迟探测、一键添加到 `dns.servers` 以及
//! 一键替换当前正在编辑的上游（包含内置 `local` / `proxy` 固定项）。

use std::collections::HashMap;

use lanpath_client::dns_probe::DnsProbeInput;
use lanpath_client::singbox_overwrite::{
    DnsPresetRegion, DnsPresetSpec, SingboxSliceKind, recommended_dns_presets,
};
use nsh_base::theme;
use nsh_core::prelude::*;
use serde_json::Value;

use super::dns_servers::{DnsEditorDraft, ProbeState, probe_badge, spawn_probe};
use super::forms::{mutate_section, parse_active_json};
use super::state::OverwriteState;
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::layout::{SurfaceLevel, surface};
use crate::ui::select::{segmented_group, segmented_radio};
use crate::ui::{BadgeVariant, badge};

pub(super) fn recommended_dns_panel(
    state: OverwriteState,
    draft: DnsEditorDraft,
    owner: Owner,
    tasks: Stored<Vec<Spawned<()>>>,
    probes: Signal<HashMap<String, ProbeState>>,
) -> AnyElement {
    let region = signal(DnsPresetRegion::Domestic);
    let filtered = computed(move || {
        let current = region.get();
        recommended_dns_presets()
            .iter()
            .copied()
            .filter(|spec| spec.region == current)
            .collect::<Vec<_>>()
    });
    let card_state = state;

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
                .gap_2()
                .child(
                    view()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            text(|| t!("overwrite.dns.presets_title"))
                                .text_size(px(13.5))
                                .font_weight(FontWeight::SEMIBOLD),
                        )
                        .child(
                            text(|| t!("overwrite.dns.presets_hint"))
                                .text_size(px(12.0))
                                .text_color(move || theme().muted_foreground),
                        ),
                )
                .child(segmented_group(
                    "overwrite-dns-preset-region",
                    vec![
                        segmented_radio(
                            move || region.get() == DnsPresetRegion::Domestic,
                            || t!("overwrite.dns.region_domestic"),
                            "overwrite-dns-region-domestic",
                            move || region.set(DnsPresetRegion::Domestic),
                        ),
                        segmented_radio(
                            move || region.get() == DnsPresetRegion::International,
                            || t!("overwrite.dns.region_international"),
                            "overwrite-dns-region-international",
                            move || region.set(DnsPresetRegion::International),
                        ),
                    ],
                )),
        )
        .child(view().flex_row().flex_wrap().gap_2().child(each(
            move || filtered.get(),
            |spec: &DnsPresetSpec| spec.id.to_owned(),
            move |spec, _| {
                preset_card(
                    card_state.clone(),
                    draft,
                    owner,
                    tasks,
                    probes,
                    spec.get_untracked(),
                )
            },
        )))
        .into_element()
}

fn preset_card(
    state: OverwriteState,
    draft: DnsEditorDraft,
    owner: Owner,
    tasks: Stored<Vec<Spawned<()>>>,
    probes: Signal<HashMap<String, ProbeState>>,
    spec: DnsPresetSpec,
) -> AnyElement {
    let zh = move || t!("nav.overwrite") == "配置切片";
    let probe_key = format!("preset:{}", spec.id);
    let probe_key_check = probe_key.clone();
    let probe_key_badge = probe_key.clone();
    let probe_key_run = probe_key;

    let exists_state = state.clone();
    let already_added = computed(move || {
        parse_active_json(&exists_state)
            .and_then(|v| v.pointer("/dns/servers").and_then(Value::as_array).cloned())
            .is_some_and(|servers| {
                servers.iter().any(|s| {
                    s.get("tag")
                        .and_then(Value::as_str)
                        .is_some_and(|t| t.eq_ignore_ascii_case(spec.tag))
                })
            })
    });
    let editing_idx = draft.editing_index;
    let apply_draft = draft;
    let add_state = state;

    view()
        .w(px(300.0))
        .flex_grow(1.0)
        .p_3()
        .rounded(move || theme().radius)
        .bg(move || theme().muted.opacity(0.32))
        .border(px(1.0))
        .border_color(move || theme().border)
        .flex_col()
        .justify_between()
        .gap_2()
        .child(
            view()
                .flex_col()
                .gap(px(4.0))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(
                            text(move || {
                                if zh() {
                                    spec.name_zh.to_owned()
                                } else {
                                    spec.name_en.to_owned()
                                }
                            })
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM),
                        )
                        .child(badge(BadgeVariant::Default, move || {
                            spec.server_type.to_owned()
                        }))
                        .child(when(
                            move || probes.with(|m| m.contains_key(&probe_key_check)),
                            move || probe_badge(probes, probe_key_badge.clone()),
                        )),
                )
                .child(
                    text(move || match spec.detour {
                        Some(d) => format!("{}:{} · detour: {d}", spec.server, spec.server_port),
                        None => format!("{}:{}", spec.server, spec.server_port),
                    })
                    .text_size(px(11.5))
                    .text_color(move || theme().muted_foreground),
                )
                .child(
                    text(move || {
                        if zh() {
                            spec.description_zh.to_owned()
                        } else {
                            spec.description_en.to_owned()
                        }
                    })
                    .text_size(px(11.5))
                    .text_color(move || theme().muted_foreground),
                ),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_end()
                .gap_2()
                .child(
                    button(ButtonVariant::Ghost)
                        .test_id(format!("overwrite-dns-preset-probe-{}", spec.id))
                        .size(ButtonSize::Sm)
                        .prefix(|| icon_sized(Icon::Activity, Tone::Muted, 13.0).into_element())
                        .label(|| t!("overwrite.dns.probe"))
                        .on_press(move || {
                            spawn_probe(
                                owner,
                                tasks,
                                probes,
                                probe_key_run.clone(),
                                DnsProbeInput {
                                    server_type: spec.server_type.to_owned(),
                                    server: spec.server.to_owned(),
                                    server_port: Some(spec.server_port),
                                    domain: None,
                                },
                            );
                        }),
                )
                .child(
                    button(ButtonVariant::Secondary)
                        .test_id(format!("overwrite-dns-preset-add-{}", spec.id))
                        .size(ButtonSize::Sm)
                        .disabled(move || editing_idx.get().is_none() && already_added.get())
                        .label(move || {
                            if editing_idx.get().is_some() {
                                t!("overwrite.dns.apply_to_edit")
                            } else if already_added.get() {
                                t!("rulesets.market.added")
                            } else {
                                t!("overwrite.dns.quick_add")
                            }
                        })
                        .on_press(move || {
                            if apply_draft.editing_index.get().is_some() {
                                // 保留正在编辑的内置/自定义 tag，仅替换上游协议、地址、端口与 detour。
                                apply_draft.server_type.set(spec.server_type.to_owned());
                                apply_draft.server.set(spec.server.to_owned());
                                apply_draft.port.set(spec.server_port.to_string());
                                apply_draft
                                    .detour
                                    .set(spec.detour.unwrap_or_default().to_owned());
                            } else {
                                mutate_section(&add_state, SingboxSliceKind::Dns, |s| {
                                    if let Some(root) = s.as_object_mut() {
                                        let arr = root
                                            .entry("servers")
                                            .or_insert_with(|| Value::Array(Vec::new()));
                                        if let Some(arr) = arr.as_array_mut() {
                                            arr.push(spec.to_server_json(None));
                                        }
                                    }
                                });
                            }
                        }),
                ),
        )
        .into_element()
}
