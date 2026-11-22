//! 规则集与分组（`@group`）Popover 弹窗选择器：
//! - 常态仅渲染紧凑触发按钮，不占用页面垂直空间；
//! - 点击弹出浮层 Popover：Tab 分离「单个规则集」与「规则集分组」、实时检索框、
//!   可滚动候选列表，并自动排除已配置项避免重复选择；支持手动采用输入标签兜底。

use std::rc::Rc;

use lanpath_client::rulesets::RuleSetGroup;
use nsh_base::{PopoverEvent, popover as base_popover, theme};
use nsh_core::prelude::*;
use serde_json::Value;

use crate::rulesets::RuleSetRow;
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::input::input;
use crate::ui::select::{
    popup_ending, popup_starting, popup_surface, segmented_group, segmented_radio,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickerTab {
    Single,
    Group,
}

#[derive(Clone, PartialEq)]
struct PickerCandidate {
    token: String,
    id_suffix: String,
    title: String,
    subtitle: String,
    search_blob: String,
}

/// 选择器各元素的静态 test_id（供自动化测试精确定位）。
#[derive(Clone, Copy)]
pub(crate) struct PickerIds {
    pub(crate) trigger: &'static str,
    pub(crate) root: &'static str,
    pub(crate) tabs: &'static str,
    pub(crate) tab_single: &'static str,
    pub(crate) tab_group: &'static str,
    pub(crate) search: &'static str,
    pub(crate) pick_prefix: &'static str,
}

/// 从 sing-box 路由/DNS 规则数组中提取全部已引用的 `rule_set` 标识（去重，保留首次写法）。
pub(crate) fn extract_used_rule_sets(rules: &[Value]) -> Vec<String> {
    let mut used = Vec::new();
    for rule in rules {
        let Some(rs) = rule.get("rule_set") else {
            continue;
        };
        match rs {
            Value::String(s) => push_unique(&mut used, s),
            Value::Array(arr) => {
                for item in arr {
                    if let Some(s) = item.as_str() {
                        push_unique(&mut used, s);
                    }
                }
            }
            _ => {}
        }
    }
    used
}

/// 大小写不敏感检查某规则集/分组标识是否已存在。
pub(crate) fn contains_token(list: &[String], token: &str) -> bool {
    let needle = token.trim();
    !needle.is_empty() && list.iter().any(|item| item.eq_ignore_ascii_case(needle))
}

fn push_unique(target: &mut Vec<String>, raw: &str) {
    let trimmed = raw.trim();
    if !trimmed.is_empty() && !contains_token(target, trimmed) {
        target.push(trimmed.to_owned());
    }
}

/// Popover 弹窗版规则集 / 分组选择器。
/// - `close_on_pick = true`：选中一项后自动收起弹窗（适用于路由/DNS 单规则添加）；
/// - `close_on_pick = false`：保持弹窗展开以便连续多选（适用于规则页多标签追加）。
pub(crate) fn ruleset_picker_popover(
    ids: PickerIds,
    groups: Signal<Vec<RuleSetGroup>>,
    items: Signal<Vec<RuleSetRow>>,
    excluded: Memo<Vec<String>>,
    selected: Signal<String>,
    close_on_pick: bool,
    on_pick: impl Fn(String) + 'static,
) -> AnyElement {
    let open = signal(false);
    let initial_tab =
        if items.with_untracked(Vec::is_empty) && !groups.with_untracked(Vec::is_empty) {
            PickerTab::Group
        } else {
            PickerTab::Single
        };
    let tab = signal(initial_tab);
    let query = signal(String::new());
    let on_pick: Rc<dyn Fn(String)> = Rc::new(on_pick);

    let available_singles = computed(move || {
        let excl = excluded.get();
        items.with(|list| {
            list.iter()
                .filter(|row| !contains_token(&excl, &row.entry.tag))
                .map(|row| {
                    let tag = row.entry.tag.clone();
                    let name = row.entry.display_name().to_owned();
                    let subtitle = if name.eq_ignore_ascii_case(&tag) {
                        String::new()
                    } else {
                        name.clone()
                    };
                    let search_blob = format!("{tag} {name}").to_ascii_lowercase();
                    PickerCandidate {
                        id_suffix: format!("set-{tag}"),
                        title: tag.clone(),
                        token: tag,
                        subtitle,
                        search_blob,
                    }
                })
                .collect::<Vec<_>>()
        })
    });

    let available_groups = computed(move || {
        let excl = excluded.get();
        groups.with(|list| {
            list.iter()
                .filter(|g| !contains_token(&excl, &g.ref_token()))
                .map(|g| {
                    let token = g.ref_token();
                    let name = g.display_name().to_owned();
                    let subtitle = if name.eq_ignore_ascii_case(&g.tag) || name.is_empty() {
                        t!("rulesets.picker.group_members", count = g.member_tags.len())
                    } else {
                        format!("{name} · {}", g.member_tags.len())
                    };
                    let search_blob =
                        format!("{} {} {} {}", token, g.tag, name, g.member_tags.join(" "))
                            .to_ascii_lowercase();
                    PickerCandidate {
                        id_suffix: format!("group-{}", g.tag),
                        title: token.clone(),
                        token,
                        subtitle,
                        search_blob,
                    }
                })
                .collect::<Vec<_>>()
        })
    });

    let filtered = computed(move || {
        let q = query.get().trim().to_ascii_lowercase();
        let pool = match tab.get() {
            PickerTab::Single => available_singles.get(),
            PickerTab::Group => available_groups.get(),
        };
        if q.is_empty() {
            pool
        } else {
            pool.into_iter()
                .filter(|c| c.search_blob.contains(&q))
                .collect()
        }
    });

    let trigger = nsh_base::button()
        .w_full()
        .h(px(36.0))
        .px_3()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_2()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || {
            if open.get() {
                theme().ring
            } else {
                theme().input
            }
        })
        .bg(move || theme().background)
        .hover(|s| s.bg(move || theme().accent.opacity(0.6)))
        .focus_visible(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .cursor(Cursor::Default)
        .child(
            view()
                .test_id(ids.trigger)
                .flex_1()
                .min_w(px(0.0))
                .flex_row()
                .items_center()
                .gap_2()
                .child(icon_sized(Icon::Shield, Tone::Muted, 14.0))
                .child(
                    text(move || {
                        let cur = selected.get();
                        if cur.trim().is_empty() {
                            t!("rulesets.picker.trigger_placeholder")
                        } else {
                            cur
                        }
                    })
                    .text_size(px(13.0))
                    .text_color(move || {
                        if selected.get().trim().is_empty() {
                            theme().muted_foreground
                        } else {
                            theme().foreground
                        }
                    })
                    .truncate(),
                ),
        )
        .child(icon_sized(Icon::Chevron, Tone::Muted, 14.0));

    let pick_for_list = on_pick.clone();
    let pick_for_custom = on_pick;

    let popup_content = view()
        .test_id(ids.root)
        .w(px(360.0))
        .flex_col()
        .gap_2()
        .child(segmented_group(
            ids.tabs,
            vec![
                segmented_radio(
                    move || tab.get() == PickerTab::Single,
                    move || {
                        t!(
                            "rulesets.picker.tab_single",
                            count = available_singles.with(Vec::len)
                        )
                    },
                    ids.tab_single,
                    move || tab.set(PickerTab::Single),
                ),
                segmented_radio(
                    move || tab.get() == PickerTab::Group,
                    move || {
                        t!(
                            "rulesets.picker.tab_group",
                            count = available_groups.with(Vec::len)
                        )
                    },
                    ids.tab_group,
                    move || tab.set(PickerTab::Group),
                ),
            ],
        ))
        .child(
            input(query)
                .test_id(ids.search)
                .placeholder(|| t!("rulesets.picker.search_placeholder"))
                .on_change(move |v| query.set(v)),
        )
        .child(when(
            move || filtered.with(Vec::is_empty),
            move || {
                let pick_custom = pick_for_custom.clone();
                view()
                    .py_3()
                    .px_2()
                    .flex_col()
                    .gap_2()
                    .child(
                        text(move || {
                            let (source_empty, avail_empty) = match tab.get() {
                                PickerTab::Single => (
                                    items.with(Vec::is_empty),
                                    available_singles.with(Vec::is_empty),
                                ),
                                PickerTab::Group => (
                                    groups.with(Vec::is_empty),
                                    available_groups.with(Vec::is_empty),
                                ),
                            };
                            if source_empty {
                                match tab.get() {
                                    PickerTab::Single => t!("rulesets.picker.empty_single"),
                                    PickerTab::Group => t!("rulesets.picker.empty_group"),
                                }
                            } else if avail_empty && query.get().trim().is_empty() {
                                t!("rulesets.picker.all_configured")
                            } else {
                                t!("rulesets.picker.no_match")
                            }
                        })
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                    )
                    .child(when(
                        move || {
                            let q = query.get();
                            let trimmed = q.trim();
                            !trimmed.is_empty() && !contains_token(&excluded.get(), trimmed)
                        },
                        move || {
                            let pick_custom = pick_custom.clone();
                            button(ButtonVariant::Outline)
                                .size(ButtonSize::Sm)
                                .label(move || {
                                    t!(
                                        "rulesets.picker.use_custom",
                                        value = query.get().trim().to_owned()
                                    )
                                })
                                .on_press(move || {
                                    let val = query.get().trim().to_owned();
                                    if !val.is_empty() {
                                        pick_custom(val);
                                        query.set(String::new());
                                        if close_on_pick {
                                            open.set(false);
                                        }
                                    }
                                })
                        },
                    ))
            },
        ))
        .child(when(
            move || !filtered.with(Vec::is_empty),
            move || {
                let pick_list = pick_for_list.clone();
                scroll()
                    .max_h(px(220.0))
                    .child(view().flex_col().gap_1().child(each(
                        filtered,
                        |c: &PickerCandidate| c.token.clone(),
                        move |cand, _| {
                            let c = cand.get_untracked();
                            let tok = c.token.clone();
                            let tok_sel = c.token.clone();
                            let title = c.title.clone();
                            let subtitle = c.subtitle.clone();
                            let has_subtitle = !subtitle.is_empty();
                            let tid = format!("{}-pick-{}", ids.pick_prefix, c.id_suffix);
                            let pick = pick_list.clone();
                            let is_sel = move || selected.get().eq_ignore_ascii_case(&tok_sel);
                            nsh_base::button()
                                .w_full()
                                .min_h(px(32.0))
                                .px_2()
                                .py(px(5.0))
                                .rounded(move || {
                                    theme().number("radius-sm").map(px).unwrap_or(px(4.0))
                                })
                                .cursor(Cursor::Default)
                                .bg(move || {
                                    if is_sel() {
                                        theme().accent
                                    } else {
                                        transparent_black().into()
                                    }
                                })
                                .hover(|s| s.bg(move || theme().accent.opacity(0.75)))
                                .on_press(move || {
                                    pick(tok.clone());
                                    if close_on_pick {
                                        open.set(false);
                                    }
                                })
                                .child(
                                    view()
                                        .test_id(&tid)
                                        .w_full()
                                        .flex_row()
                                        .items_center()
                                        .justify_between()
                                        .gap_2()
                                        .child(
                                            text(title)
                                                .text_size(px(12.5))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(move || theme().popover_foreground)
                                                .truncate(),
                                        )
                                        .child(when(
                                            move || has_subtitle,
                                            move || {
                                                text(subtitle.clone())
                                                    .text_size(px(11.5))
                                                    .text_color(move || theme().muted_foreground)
                                                    .truncate()
                                            },
                                        )),
                                )
                        },
                    )))
            },
        ));

    base_popover()
        .open(move || Some(open.get()))
        .placement(Placement::Bottom)
        .align(Align::Start)
        .offset(px(4.0))
        .margin(px(8.0))
        .on_open_change(move |ev: &PopoverEvent| {
            if let PopoverEvent::OpenChange(next, _) = ev {
                open.set(*next);
                if !*next {
                    query.set(String::new());
                }
            }
        })
        .popup_style(|s| popup_surface(s).p_3())
        .popup_starting_style(popup_starting)
        .popup_ending_style(popup_ending)
        .trigger(trigger)
        .child(popup_content)
        .into_element()
}
