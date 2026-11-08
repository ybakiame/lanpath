use std::collections::HashMap;

use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, IndexPath, Sizable as _,
    list::{ListDelegate, ListItem, ListState},
    tag::Tag,
};
use gpui_kit::{App, Context, IntoElement, ParentElement as _, Styled as _, Window, div};
use lanpath_client::{
    DelayResult,
    proxies::{GroupView, NodeView},
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ProxySort {
    #[default]
    Original,
    Name,
    Delay,
}

#[derive(Default)]
pub(crate) struct ProxyListDelegate {
    group: Option<GroupView>,
    nodes: HashMap<String, NodeView>,
    member_delays: HashMap<String, Option<u16>>,
    selected_index: Option<IndexPath>,
    testing: Option<String>,
    visible: Vec<String>,
    query: String,
    sort: ProxySort,
}

impl ProxyListDelegate {
    pub(crate) fn matches_nodes(&self, nodes: &[NodeView]) -> bool {
        self.nodes.len() == nodes.len()
            && nodes
                .iter()
                .all(|node| self.nodes.get(&node.name) == Some(node))
    }

    pub(crate) fn set_nodes(&mut self, nodes: Vec<NodeView>) {
        self.member_delays.clear();
        self.nodes = nodes
            .into_iter()
            .map(|node| (node.name.clone(), node))
            .collect();
    }

    /// Background snapshots update current usage without moving keyboard focus
    /// or replacing a measured timeout with an older core history sample.
    pub(crate) fn refresh_snapshot(&mut self, nodes: Vec<NodeView>, group: Option<GroupView>) {
        let focus = self.selected_member();
        let same_group =
            self.group.as_ref().map(|group| &group.name) == group.as_ref().map(|group| &group.name);
        let mut measured = std::mem::take(&mut self.member_delays);
        self.set_nodes(nodes);
        measured.retain(|name, _| {
            self.nodes.contains_key(name)
                || group
                    .as_ref()
                    .is_some_and(|group| group.members.contains(name))
        });
        self.member_delays = measured;
        self.set_group(group);
        if same_group
            && let Some(index) =
                focus.and_then(|name| self.visible.iter().position(|member| member == &name))
        {
            self.selected_index = Some(IndexPath::new(index));
        }
    }

    pub(crate) fn set_group(&mut self, group: Option<GroupView>) {
        self.group = group;
        self.rebuild();
    }

    pub(crate) fn clear(&mut self) {
        self.group = None;
        self.nodes.clear();
        self.member_delays.clear();
        self.selected_index = None;
        self.testing = None;
        self.visible.clear();
    }

    pub(crate) fn reset_selection(&mut self) {
        self.selected_index = self.group.as_ref().and_then(|group| {
            self.visible
                .iter()
                .position(|member| member == &group.now)
                .map(IndexPath::new)
        });
    }

    pub(crate) fn filter(&mut self, query: String) {
        self.query = query.to_lowercase();
        self.rebuild();
    }
    pub(crate) fn sort(&mut self, sort: ProxySort) {
        self.sort = sort;
        self.rebuild();
    }
    pub(crate) fn selected_index(&self) -> Option<IndexPath> {
        self.selected_index
    }

    pub(crate) fn sort_order(&self) -> ProxySort {
        self.sort
    }

    fn delay(&self, name: &str) -> Option<u16> {
        self.member_delays
            .get(name)
            .copied()
            .unwrap_or_else(|| self.nodes.get(name).and_then(|node| node.delay_ms))
    }

    fn rebuild(&mut self) {
        self.visible = self
            .group
            .as_ref()
            .map(|group| {
                group
                    .members
                    .iter()
                    .filter(|name| name.to_lowercase().contains(&self.query))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut visible = std::mem::take(&mut self.visible);
        match self.sort {
            ProxySort::Original => {}
            ProxySort::Name => visible.sort_by_key(|name| name.to_lowercase()),
            ProxySort::Delay => visible.sort_by_key(|name| {
                (
                    self.delay(name).is_none(),
                    self.delay(name).unwrap_or_default(),
                )
            }),
        }
        self.visible = visible;
        self.reset_selection();
    }

    pub(crate) fn group_name(&self) -> Option<String> {
        self.group.as_ref().map(|group| group.name.clone())
    }

    pub(crate) fn is_selector(&self) -> bool {
        self.group
            .as_ref()
            .is_some_and(|group| group.group_type == "Selector")
    }

    pub(crate) fn member(&self, row: usize) -> Option<String> {
        self.visible.get(row).cloned()
    }

    pub(crate) fn selected_member(&self) -> Option<String> {
        self.selected_index.and_then(|index| self.member(index.row))
    }

    pub(crate) fn set_testing(&mut self, name: Option<String>) {
        self.testing = name;
    }

    pub(crate) fn apply_delay(&mut self, name: &str, delay_ms: Option<u16>) {
        self.record_delay(name, delay_ms);
        self.rebuild();
    }

    pub(crate) fn apply_delays(&mut self, delays: &[DelayResult]) {
        for result in delays {
            self.record_delay(&result.name, result.delay_ms);
        }
        self.rebuild();
    }

    fn record_delay(&mut self, name: &str, delay: Option<u16>) {
        self.member_delays.insert(name.to_string(), delay);
        if let Some(node) = self.nodes.get_mut(name) {
            node.delay_ms = delay;
        }
    }
}

impl ListDelegate for ProxyListDelegate {
    type Item = ListItem;

    fn items_count(&self, _section: usize, _cx: &App) -> usize {
        self.visible.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let group = self.group.as_ref()?;
        let member = self.visible.get(ix.row)?;
        let current = member == &group.now;
        let node = self.nodes.get(member);
        let kind = node
            .map(|node| node.node_type.as_str())
            .unwrap_or("策略成员");
        let delay = self.delay(member);
        let testing = self.testing.as_deref() == Some(member);
        let timeout = self.member_delays.get(member).is_some_and(Option::is_none);
        let latency = if testing {
            "测速中…".into()
        } else if let Some(delay) = delay {
            format!("{delay} ms")
        } else if timeout {
            "超时".into()
        } else {
            "未测速".into()
        };
        let color = if timeout {
            cx.theme().danger
        } else {
            match delay {
                Some(0..=200) => cx.theme().success,
                Some(201..=500) => cx.theme().warning,
                Some(_) => cx.theme().danger,
                None => cx.theme().muted_foreground,
            }
        };
        let label = format!(
            "{member}，{kind}，{latency}{}",
            if current { "，当前使用" } else { "" }
        );
        Some(
            ListItem::new(ix)
                .py_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .w_full()
                        .gap_3()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .gap_1()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .h_6()
                                        .gap_2()
                                        .child(
                                            div()
                                                .font_medium()
                                                .min_w_0()
                                                .truncate()
                                                .child(member.clone()),
                                        )
                                        .children(
                                            current
                                                .then(|| Tag::primary().small().child("当前使用")),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(format!(
                                            "{kind}{}",
                                            if node.is_some_and(|node| node.udp) {
                                                " · UDP"
                                            } else {
                                                ""
                                            }
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .text_sm()
                                .font_medium()
                                .text_color(color)
                                .child(latency),
                        ),
                )
                .accessibility_label(label)
                .selected(current),
        )
    }

    fn render_section_header(
        &mut self,
        _section: usize,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<impl IntoElement> {
        let group = self.group.as_ref()?;
        Some(
            div()
                .px_2()
                .py_1()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(format!(
                    "{} · {} · 当前：{} · {} 个候选",
                    group.name,
                    group.group_type,
                    group.now,
                    group.members.len()
                )),
        )
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        div()
            .p_4()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(if self.group.is_some() {
                "没有匹配的节点"
            } else {
                "暂无代理分组"
            })
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        self.selected_index = ix;
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn populated() -> ProxyListDelegate {
        let mut list = ProxyListDelegate::default();
        list.set_nodes(vec![NodeView {
            name: "Beta".into(),
            node_type: "Trojan".into(),
            delay_ms: Some(80),
            udp: true,
        }]);
        list.set_group(Some(GroupView {
            name: "proxy".into(),
            group_type: "Selector".into(),
            now: "Beta".into(),
            members: vec!["Alpha".into(), "Beta".into(), "Gamma".into()],
        }));
        list
    }

    #[test]
    fn filtering_and_sorting_keep_confirmation_mapped_to_visible_members() {
        let mut list = populated();
        list.sort(ProxySort::Delay);
        assert_eq!(list.member(0).as_deref(), Some("Beta"));
        list.filter("GAM".into());
        assert_eq!(list.member(0).as_deref(), Some("Gamma"));
        assert!(list.selected_member().is_none());
        assert_eq!(list.group.as_ref().unwrap().now, "Beta");
        list.filter(String::new());
        assert_eq!(list.selected_member().as_deref(), Some("Beta"));
        list.sort(ProxySort::Original);
        assert_eq!(list.member(0).as_deref(), Some("Alpha"));
        assert_eq!(list.selected_index.unwrap().row, 1);
    }

    #[test]
    fn measured_timeouts_do_not_fall_back_to_old_latency() {
        let mut list = populated();
        list.apply_delay("Beta", None);
        assert_eq!(list.delay("Beta"), None);
        list.apply_delay("Gamma", Some(30));
        list.sort(ProxySort::Delay);
        assert_eq!(list.member(0).as_deref(), Some("Gamma"));
        assert_eq!(list.group.as_ref().unwrap().now, "Beta");
    }
    #[test]
    fn background_snapshot_preserves_focus_filter_and_explicit_timeout() {
        let mut list = populated();
        list.filter("be".into());
        list.apply_delay("Beta", None);
        let mut group = list.group.clone().unwrap();
        group.now = "Alpha".into();
        list.refresh_snapshot(
            vec![NodeView {
                name: "Beta".into(),
                node_type: "Trojan".into(),
                delay_ms: Some(80),
                udp: true,
            }],
            Some(group),
        );
        assert_eq!(list.selected_member().as_deref(), Some("Beta"));
        assert_eq!(list.group.as_ref().unwrap().now, "Alpha");
        assert_eq!(list.delay("Beta"), None);
        assert_eq!(list.query, "be");
        let next = GroupView {
            name: "proxy".into(),
            group_type: "Selector".into(),
            now: "Alpha".into(),
            members: vec!["Alpha".into()],
        };
        list.filter(String::new());
        list.refresh_snapshot(Vec::new(), Some(next));
        assert_eq!(list.selected_member().as_deref(), Some("Alpha"));
        assert!(!list.member_delays.contains_key("Beta"));
    }
}
