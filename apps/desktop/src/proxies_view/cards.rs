//! 运行时策略组选择条与节点网格卡片（支持点击切换、延迟徽章着色、单节点测速与排序）。

use lanpath_client::GroupView;
use nsh_base::theme;
use nsh_core::prelude::*;

use super::state::{DelayCell, ProxiesState};
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::input::input;
use crate::ui::layout::{SurfaceLevel, section_title, surface};
use crate::ui::{BadgeVariant, badge};

pub(super) fn live_proxies_content(state: ProxiesState) -> AnyElement {
    let groups_sig = state.groups;
    let selected_group = state.selected_group;
    let search = state.search;
    let sort_by_delay = state.sort_by_delay;
    let testing_group = state.testing_group;
    let active_state = state.clone();
    let test_group_state = state.clone();
    let grid_state = state.clone();

    let active_group_memo = computed(move || active_state.active_group());

    let visible_members = computed(move || {
        let Some(group) = active_group_memo.get() else {
            return Vec::new();
        };
        let query = search.get().trim().to_ascii_lowercase();
        let mut members: Vec<String> = group
            .members
            .into_iter()
            .filter(|name| query.is_empty() || name.to_ascii_lowercase().contains(&query))
            .collect();
        if sort_by_delay.get() {
            let delays = grid_state.delays.get();
            members.sort_by_key(|name| match delays.get(name) {
                Some(DelayCell::Measured(ms)) => (0u8, *ms),
                Some(DelayCell::Testing) => (1u8, u16::MAX),
                None => (2u8, u16::MAX),
                Some(DelayCell::Timeout) => (3u8, u16::MAX),
            });
        }
        members
    });

    view()
        .flex_col()
        .gap_4()
        .child(group_selector_strip(groups_sig, selected_group))
        .child(when(
            move || active_group_memo.get().is_some(),
            move || {
                let test_state = test_group_state.clone();
                let cards_state = state.clone();
                surface(SurfaceLevel::Card)
                    .p_5()
                    .flex_col()
                    .gap_4()
                    .child(
                        view()
                            .flex_row()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                view()
                                    .flex_row()
                                    .flex_wrap()
                                    .items_center()
                                    .gap_2()
                                    .child(section_title(move || {
                                        active_group_memo.get().map(|g| g.name).unwrap_or_default()
                                    }))
                                    .child(badge(BadgeVariant::Default, move || {
                                        active_group_memo
                                            .get()
                                            .map(|g| g.group_type)
                                            .unwrap_or_default()
                                    }))
                                    .child(badge(BadgeVariant::Success, move || {
                                        let now = active_group_memo
                                            .get()
                                            .map(|g| g.now)
                                            .unwrap_or_default();
                                        if now.is_empty() {
                                            "—".to_owned()
                                        } else {
                                            t!("proxies.current_now", name = now)
                                        }
                                    })),
                            )
                            .child(
                                view()
                                    .flex_row()
                                    .flex_wrap()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        view().w(px(200.0)).child(
                                            input(search)
                                                .test_id("proxies-search")
                                                .placeholder(|| t!("proxies.search_placeholder"))
                                                .accessible_name(|| {
                                                    t!("proxies.search_placeholder")
                                                })
                                                .on_change(move |v| search.set(v)),
                                        ),
                                    )
                                    .child(
                                        button(ButtonVariant::Outline)
                                            .test_id("proxies-sort-delay")
                                            .size(ButtonSize::Sm)
                                            .selected(move || sort_by_delay.get())
                                            .label(|| t!("proxies.sort_delay"))
                                            .on_press(move || {
                                                sort_by_delay.set(!sort_by_delay.get());
                                            }),
                                    )
                                    .child(
                                        button(ButtonVariant::Secondary)
                                            .test_id("proxies-test-group")
                                            .size(ButtonSize::Sm)
                                            .disabled(move || testing_group.get())
                                            .prefix(|| {
                                                icon_sized(Icon::Activity, Tone::Foreground, 13.0)
                                                    .into_element()
                                            })
                                            .label(move || {
                                                if testing_group.get() {
                                                    t!("proxies.testing")
                                                } else {
                                                    t!("proxies.test_group")
                                                }
                                            })
                                            .on_press(move || test_state.test_active_group()),
                                    ),
                            ),
                    )
                    .child(when(
                        move || {
                            active_group_memo
                                .get()
                                .is_some_and(|g| !is_manual_selectable(&g.group_type))
                        },
                        || {
                            text(|| t!("proxies.auto_group_hint"))
                                .text_size(px(12.0))
                                .text_color(move || theme().muted_foreground)
                        },
                    ))
                    .child(view().flex_row().flex_wrap().gap_3().child(each(
                        visible_members,
                        |name: &String| name.clone(),
                        move |member_sig, _| {
                            let name = member_sig.get_untracked();
                            node_member_card(cards_state.clone(), active_group_memo, name)
                        },
                    )))
            },
        ))
        .into_element()
}

fn group_selector_strip(
    groups: Signal<Vec<GroupView>>,
    selected_group: Signal<String>,
) -> AnyElement {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_2()
        .child(each(
            move || groups.get(),
            |g: &GroupView| format!("{}:{}", g.name, g.now),
            move |group_sig, _| {
                let g = group_sig.get_untracked();
                let name = g.name.clone();
                let name_id = g.name.clone();
                let name_chk = g.name.clone();
                let label = if g.now.is_empty() {
                    g.name.clone()
                } else {
                    format!("{} · {}", g.name, g.now)
                };
                button(ButtonVariant::Outline)
                    .test_id(format!("proxy-group-tab-{name_id}"))
                    .size(ButtonSize::Md)
                    .selected(move || selected_group.get() == name_chk)
                    .label(move || label.clone())
                    .on_press(move || selected_group.set(name.clone()))
            },
        ))
        .into_element()
}

fn node_member_card(
    state: ProxiesState,
    active_group: Memo<Option<GroupView>>,
    member_name: String,
) -> AnyElement {
    let name_for_selected = member_name.clone();
    let name_for_press = member_name.clone();
    let name_for_test = member_name.clone();
    let name_for_delay = member_name.clone();
    let name_for_meta = member_name.clone();
    let name_for_card_id = member_name.clone();
    let name_for_test_id = member_name.clone();

    let is_selected = computed(move || {
        active_group
            .get()
            .is_some_and(|g| g.now == name_for_selected)
    });
    let can_select = computed(move || {
        active_group
            .get()
            .is_some_and(|g| is_manual_selectable(&g.group_type))
    });

    let node_info = state
        .node_map
        .with_untracked(|map| map.get(&name_for_meta).cloned());
    let proto_label = node_info
        .as_ref()
        .map(|n| n.node_type.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| t!("proxies.group_ref"));
    let supports_udp = node_info.as_ref().is_some_and(|n| n.udp);

    let delays = state.delays;
    let delay_state = computed(move || delays.with(|map| map.get(&name_for_delay).copied()));
    let select_state = state.clone();
    let test_state = state;

    view()
        .w(px(236.0))
        .p_3()
        .rounded(move || theme().radius)
        .border(px(1.5))
        .border_color(move || {
            if is_selected.get() {
                theme().primary
            } else {
                theme().border
            }
        })
        .bg(move || {
            if is_selected.get() {
                theme().primary.opacity(0.08)
            } else {
                theme().card
            }
        })
        .flex_col()
        .gap_2()
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    text(member_name)
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(px(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(move || theme().foreground)
                        .truncate(),
                )
                .child(when(
                    move || is_selected.get(),
                    || icon_sized(Icon::Check, Tone::Primary, 14.0),
                )),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .child(badge(BadgeVariant::Outline, move || proto_label.clone()))
                        .child(when(
                            move || supports_udp,
                            || badge(BadgeVariant::Default, || "UDP".to_owned()),
                        )),
                )
                .child(latency_pill(delay_state)),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    button(ButtonVariant::Ghost)
                        .test_id(format!("proxy-test-{name_for_test_id}"))
                        .size(ButtonSize::Sm)
                        .disabled(move || matches!(delay_state.get(), Some(DelayCell::Testing)))
                        .label(|| t!("proxies.test_one"))
                        .on_press(move || test_state.test_node(name_for_test.clone())),
                )
                .child(when(
                    move || can_select.get(),
                    move || {
                        let st = select_state.clone();
                        let member = name_for_press.clone();
                        let card_id = name_for_card_id.clone();
                        button(ButtonVariant::Outline)
                            .test_id(format!("proxy-select-{card_id}"))
                            .size(ButtonSize::Sm)
                            .disabled(move || is_selected.get())
                            .label(move || {
                                if is_selected.get() {
                                    t!("proxies.selected")
                                } else {
                                    t!("proxies.use_node")
                                }
                            })
                            .on_press(move || {
                                if let Some(group) = active_group.get() {
                                    st.select_member(group.name, member.clone());
                                }
                            })
                    },
                )),
        )
        .into_element()
}

fn latency_pill(delay: Memo<Option<DelayCell>>) -> AnyElement {
    let label = move || match delay.get() {
        Some(DelayCell::Testing) => t!("proxies.testing"),
        Some(DelayCell::Measured(ms)) => format!("{ms} ms"),
        Some(DelayCell::Timeout) => t!("proxies.timeout"),
        None => "—".to_owned(),
    };
    text(label)
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(move || match delay.get() {
            Some(DelayCell::Measured(ms)) if ms < 150 => {
                theme().color("success").unwrap_or(theme().primary)
            }
            Some(DelayCell::Measured(ms)) if ms < 350 => {
                theme().color("warning").unwrap_or(theme().foreground)
            }
            Some(DelayCell::Measured(_)) | Some(DelayCell::Timeout) => theme().destructive,
            _ => theme().muted_foreground,
        })
        .into_element()
}

fn is_manual_selectable(group_type: &str) -> bool {
    group_type.eq_ignore_ascii_case("selector") || group_type.eq_ignore_ascii_case("select")
}
