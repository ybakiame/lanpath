//! `overwrite/sing-box/outbounds.json` 可视化出站策略与节点分组表单：
//! - 一键生成常用地区自动测速分组（HK/TW/JP/SG/US/EU，并自动加入 `proxy` 选择组）
//! - 展示已有策略组（含 `$nodes` 与 `_filter` 正则筛选摘要）、支持编辑/排序/删除
//! - 展开 `outbounds_editor` 进行新建或就地修改（成员勾选 + 正则实时预览）

use lanpath_client::singbox_overwrite::{
    NODES_PLACEHOLDER, NodeFilterSpec, RegionGroupPreset, SingboxSliceKind,
    recommended_region_presets,
};
use nsh_base::theme;
use nsh_core::prelude::*;
use serde_json::{Value, json};

use super::forms::{mutate_section, parse_active_json};
use super::outbounds_editor::outbound_editor_card;
use super::state::OverwriteState;
use crate::singbox_overwrite::SingboxOverwriteService;
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::layout::{AlertTone, SurfaceLevel, alert_banner, section_title, surface};
use crate::ui::{BadgeVariant, badge};

/// 可拷贝的出站分组编辑器草稿状态（用于新增与就地编辑）。
#[derive(Clone, Copy)]
pub(super) struct OutboundEditorDraft {
    pub(super) open: Signal<bool>,
    pub(super) editing_index: Signal<Option<usize>>,
    pub(super) tag: Signal<String>,
    pub(super) kind: Signal<String>,
    pub(super) default_member: Signal<String>,
    pub(super) include_nodes: Signal<bool>,
    pub(super) filter_include: Signal<String>,
    pub(super) filter_exclude: Signal<String>,
    pub(super) members: Signal<Vec<String>>,
    pub(super) url: Signal<String>,
    pub(super) interval: Signal<String>,
    pub(super) tolerance: Signal<String>,
    pub(super) error: Signal<Option<String>>,
}

impl OutboundEditorDraft {
    pub(super) fn new() -> Self {
        Self {
            open: signal(false),
            editing_index: signal(None),
            tag: signal(String::new()),
            kind: signal("selector".to_owned()),
            default_member: signal(String::new()),
            include_nodes: signal(true),
            filter_include: signal(String::new()),
            filter_exclude: signal(String::new()),
            members: signal(vec!["direct".to_owned()]),
            url: signal("https://www.gstatic.com/generate_204".to_owned()),
            interval: signal("5m".to_owned()),
            tolerance: signal("150".to_owned()),
            error: signal(None),
        }
    }

    pub(super) fn open_add(self) {
        self.editing_index.set(None);
        self.tag.set(String::new());
        self.kind.set("selector".to_owned());
        self.default_member.set(String::new());
        self.include_nodes.set(true);
        self.filter_include.set(String::new());
        self.filter_exclude.set(String::new());
        self.members.set(Vec::new());
        self.url
            .set("https://www.gstatic.com/generate_204".to_owned());
        self.interval.set("5m".to_owned());
        self.tolerance.set("150".to_owned());
        self.error.set(None);
        self.open.set(true);
    }

    pub(super) fn open_edit(self, index: usize, value: &Value) {
        let obj = value.as_object();
        let tag = obj
            .and_then(|o| o.get("tag"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let kind = obj
            .and_then(|o| o.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("selector")
            .to_owned();
        let default_member = obj
            .and_then(|o| o.get("default"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let raw_members: Vec<String> = obj
            .and_then(|o| o.get("outbounds"))
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let include_nodes = raw_members.iter().any(|m| m == NODES_PLACEHOLDER);
        let static_members: Vec<String> = raw_members
            .into_iter()
            .filter(|m| m != NODES_PLACEHOLDER)
            .collect();
        let filter = obj
            .and_then(|o| NodeFilterSpec::from_outbound_map(o).ok().flatten())
            .unwrap_or_default();
        let url = obj
            .and_then(|o| o.get("url"))
            .and_then(Value::as_str)
            .unwrap_or("https://www.gstatic.com/generate_204")
            .to_owned();
        let interval = obj
            .and_then(|o| o.get("interval"))
            .and_then(Value::as_str)
            .unwrap_or("5m")
            .to_owned();
        let tolerance = obj
            .and_then(|o| o.get("tolerance"))
            .and_then(Value::as_u64)
            .map(|n| n.to_string())
            .unwrap_or_else(|| "150".to_owned());

        self.editing_index.set(Some(index));
        self.tag.set(tag);
        self.kind.set(kind);
        self.default_member.set(default_member);
        self.include_nodes.set(include_nodes);
        self.filter_include.set(filter.include);
        self.filter_exclude.set(filter.exclude);
        self.members.set(static_members);
        self.url.set(url);
        self.interval.set(interval);
        self.tolerance.set(tolerance);
        self.error.set(None);
        self.open.set(true);
    }

    pub(super) fn close(self) {
        self.open.set(false);
        self.editing_index.set(None);
        self.error.set(None);
    }

    pub(super) fn toggle_member(self, tag: &str) {
        self.members.update(|list| {
            if let Some(pos) = list.iter().position(|existing| existing == tag) {
                list.remove(pos);
            } else {
                list.push(tag.to_owned());
            }
        });
    }
}

pub(super) fn outbounds_form(state: OverwriteState) -> impl IntoElement {
    let list_state = state.clone();
    let items_memo = computed(move || {
        parse_active_json(&list_state)
            .and_then(|v| v.get("outbounds").and_then(Value::as_array).cloned())
            .unwrap_or_default()
    });
    let cached_nodes = SingboxOverwriteService::open()
        .map(|s| s.load_active_subscription_node_tags())
        .unwrap_or_default();
    let draft = OutboundEditorDraft::new();
    let preset_state = state.clone();
    let row_state = state.clone();
    let editor_state = state;

    view()
        .flex_col()
        .gap_4()
        .child(alert_banner(
            AlertTone::Info,
            text(|| t!("overwrite.outbounds.placeholder_hint"))
                .text_size(px(12.5))
                .text_color(move || theme().foreground),
        ))
        .child(region_presets_card(preset_state, items_memo, &cached_nodes))
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_3()
                .child(section_title(|| t!("overwrite.outbounds.groups_title")))
                .child(
                    button(ButtonVariant::Primary)
                        .test_id("overwrite-outbound-open-add")
                        .size(ButtonSize::Sm)
                        .prefix(|| icon_sized(Icon::Plus, Tone::OnPrimary, 13.0))
                        .label(|| t!("overwrite.outbounds.add_group"))
                        .on_press(move || draft.open_add()),
                ),
        )
        .child(when(
            move || draft.open.get(),
            move || {
                outbound_editor_card(
                    editor_state.clone(),
                    draft,
                    items_memo,
                    cached_nodes.clone(),
                )
            },
        ))
        .child(view().flex_col().gap_2().child(each(
            items_memo,
            |item: &Value| item.to_string(),
            move |item, index| outbound_row(row_state.clone(), draft, item, index, items_memo),
        )))
}

fn region_presets_card(
    state: OverwriteState,
    items_memo: Memo<Vec<Value>>,
    cached_nodes: &[String],
) -> AnyElement {
    let zh = t!("nav.overwrite") == "配置切片";
    let mut chips = view().flex_row().flex_wrap().items_center().gap_2();
    for preset in recommended_region_presets() {
        let st = state.clone();
        let tag = preset.tag;
        let label = if zh { preset.label_zh } else { preset.label_en };
        let match_count = count_preset_matches(preset, cached_nodes);
        let has_cache = !cached_nodes.is_empty();
        let exists = computed(move || {
            items_memo.with(|list| {
                list.iter()
                    .any(|item| item.get("tag").and_then(Value::as_str) == Some(tag))
            })
        });
        chips = chips.child(
            button(ButtonVariant::Outline)
                .test_id(format!("overwrite-outbound-preset-{tag}"))
                .size(ButtonSize::Sm)
                .disabled(move || exists.get())
                .prefix(|| icon_sized(Icon::Plus, Tone::Muted, 12.0))
                .label(move || {
                    if exists.get() {
                        format!("{label} · {}", t!("rulesets.market.added"))
                    } else if has_cache {
                        format!("{label} ({match_count})")
                    } else {
                        label.to_owned()
                    }
                })
                .on_press(move || add_region_preset(&st, preset)),
        );
    }

    surface(SurfaceLevel::Card)
        .p_4()
        .flex_col()
        .gap_3()
        .child(
            view()
                .flex_col()
                .gap_1()
                .child(section_title(|| t!("overwrite.outbounds.presets_title")))
                .child(
                    text(|| t!("overwrite.outbounds.presets_hint"))
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .child(chips)
        .into_element()
}

fn count_preset_matches(preset: &RegionGroupPreset, cached_nodes: &[String]) -> usize {
    let spec = NodeFilterSpec {
        include: preset.include_pattern.to_owned(),
        exclude: preset.exclude_pattern.to_owned(),
    };
    let Ok(compiled) = spec.compile() else {
        return 0;
    };
    cached_nodes.iter().filter(|n| compiled.matches(n)).count()
}

fn add_region_preset(state: &OverwriteState, preset: &RegionGroupPreset) {
    let entry = preset.to_outbound_json("urltest");
    let tag = preset.tag.to_owned();
    mutate_section(state, SingboxSliceKind::Outbounds, |section| {
        let Some(arr) = section.as_array_mut() else {
            return;
        };
        if arr
            .iter()
            .any(|item| item.get("tag").and_then(Value::as_str) == Some(tag.as_str()))
        {
            return;
        }
        // 自动将新建的地区分组前置加入 `proxy` 选择组（位于 `auto` 之后），开箱即用。
        if let Some(proxy_group) = arr
            .iter_mut()
            .find(|item| item.get("tag").and_then(Value::as_str) == Some("proxy"))
            && let Some(members) = proxy_group
                .get_mut("outbounds")
                .and_then(Value::as_array_mut)
            && !members.iter().any(|m| m.as_str() == Some(tag.as_str()))
        {
            let pos = members
                .iter()
                .position(|m| m.as_str() == Some(NODES_PLACEHOLDER))
                .unwrap_or(members.len());
            members.insert(pos, json!(tag));
        }
        arr.push(entry);
    });
}

fn outbound_row(
    state: OverwriteState,
    draft: OutboundEditorDraft,
    item: ReadSignal<Value>,
    index: ReadSignal<usize>,
    items_memo: Memo<Vec<Value>>,
) -> impl IntoElement {
    let obj = item.get_untracked();
    let tag = obj
        .get("tag")
        .and_then(Value::as_str)
        .unwrap_or("—")
        .to_owned();
    let tag_edit_id = tag.clone();
    let tag_del_id = tag.clone();
    let kind = obj
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    let is_group = kind == "selector" || kind == "urltest";
    let has_nodes = obj
        .get("outbounds")
        .and_then(Value::as_array)
        .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some(NODES_PLACEHOLDER)));
    let members_count = obj
        .get("outbounds")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let filter_summary = obj
        .as_object()
        .and_then(|o| NodeFilterSpec::from_outbound_map(o).ok().flatten())
        .filter(|f| !f.is_empty())
        .map(|f| match (f.include.is_empty(), f.exclude.is_empty()) {
            (false, false) => format!("+{} / -{}", f.include, f.exclude),
            (false, true) => format!("+{}", f.include),
            (true, false) => format!("-{}", f.exclude),
            (true, true) => String::new(),
        });
    let has_filter = filter_summary.is_some();
    let filter_label = filter_summary.unwrap_or_default();

    let edit_obj = obj.clone();
    let up_state = state.clone();
    let down_state = state.clone();
    let del_state = state;

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
                .child(badge(BadgeVariant::Default, move || kind.clone()))
                .child(
                    text(move || tag.clone())
                        .text_size(px(13.5))
                        .font_weight(FontWeight::MEDIUM),
                )
                .child(when(
                    move || has_nodes,
                    || badge(BadgeVariant::Success, || NODES_PLACEHOLDER.to_owned()),
                ))
                .child(when(
                    move || has_filter,
                    move || {
                        let lbl = filter_label.clone();
                        badge(BadgeVariant::Outline, move || lbl.clone())
                    },
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
            view()
                .flex_row()
                .items_center()
                .gap_1()
                .child(when(
                    move || is_group,
                    move || {
                        let edit_obj = edit_obj.clone();
                        let tid = tag_edit_id.clone();
                        button(ButtonVariant::Ghost)
                            .test_id(format!("overwrite-outbound-edit-{tid}"))
                            .size(ButtonSize::Sm)
                            .label(|| t!("rulesets.edit"))
                            .on_press(move || draft.open_edit(index.get(), &edit_obj))
                    },
                ))
                .child(
                    button(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .disabled(move || index.get() == 0)
                        .label(|| t!("rules.up"))
                        .on_press(move || {
                            mutate_section(&up_state, SingboxSliceKind::Outbounds, |s| {
                                let i = index.get();
                                if let Some(arr) = s.as_array_mut()
                                    && i > 0
                                    && i < arr.len()
                                {
                                    arr.swap(i - 1, i);
                                }
                            });
                        }),
                )
                .child(
                    button(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .disabled(move || index.get() + 1 >= items_memo.with(Vec::len))
                        .label(|| t!("rules.down"))
                        .on_press(move || {
                            mutate_section(&down_state, SingboxSliceKind::Outbounds, |s| {
                                let i = index.get();
                                if let Some(arr) = s.as_array_mut()
                                    && i + 1 < arr.len()
                                {
                                    arr.swap(i, i + 1);
                                }
                            });
                        }),
                )
                .child(
                    button(ButtonVariant::Ghost)
                        .test_id(format!("overwrite-outbound-delete-{tag_del_id}"))
                        .size(ButtonSize::Sm)
                        .label(|| t!("rules.delete"))
                        .on_press(move || {
                            mutate_section(&del_state, SingboxSliceKind::Outbounds, |s| {
                                let idx = index.get();
                                if let Some(arr) = s.as_array_mut()
                                    && idx < arr.len()
                                {
                                    arr.remove(idx);
                                }
                            });
                        }),
                ),
        )
}
