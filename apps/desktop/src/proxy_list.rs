use std::collections::HashMap;

use gpui_kit::component::{
    ActiveTheme as _, IndexPath,
    label::Label,
    list::{ListDelegate, ListItem, ListState},
};
use gpui_kit::{App, Context, IntoElement, ParentElement as _, Styled as _, Window, div};
use lanpath_client::{
    DelayResult,
    proxies::{GroupView, NodeView},
};

#[derive(Default)]
pub(crate) struct ProxyListDelegate {
    group: Option<GroupView>,
    nodes: HashMap<String, NodeView>,
    member_delays: HashMap<String, Option<u16>>,
    selected_index: Option<IndexPath>,
    testing: Option<String>,
}

impl ProxyListDelegate {
    pub(crate) fn set_nodes(&mut self, nodes: Vec<NodeView>) {
        self.member_delays.clear();
        self.nodes = nodes
            .into_iter()
            .map(|node| (node.name.clone(), node))
            .collect();
    }

    pub(crate) fn set_group(&mut self, group: Option<GroupView>) {
        self.selected_index = group
            .as_ref()
            .and_then(|group| group.members.iter().position(|member| member == &group.now))
            .map(IndexPath::new);
        self.group = group;
    }

    pub(crate) fn clear(&mut self) {
        self.group = None;
        self.nodes.clear();
        self.member_delays.clear();
        self.selected_index = None;
        self.testing = None;
    }

    pub(crate) fn reset_selection(&mut self) {
        self.selected_index = self.group.as_ref().and_then(|group| {
            group
                .members
                .iter()
                .position(|member| member == &group.now)
                .map(IndexPath::new)
        });
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
        self.group.as_ref()?.members.get(row).cloned()
    }

    pub(crate) fn selected_member(&self) -> Option<String> {
        self.selected_index.and_then(|index| self.member(index.row))
    }

    pub(crate) fn set_testing(&mut self, name: Option<String>) {
        self.testing = name;
    }

    pub(crate) fn apply_delay(&mut self, name: &str, delay_ms: Option<u16>) {
        if let Some(node) = self.nodes.get_mut(name) {
            node.delay_ms = delay_ms;
        } else {
            self.member_delays.insert(name.to_string(), delay_ms);
        }
    }

    pub(crate) fn apply_delays(&mut self, delays: &[DelayResult]) {
        for result in delays {
            self.apply_delay(&result.name, result.delay_ms);
        }
    }
}

impl ListDelegate for ProxyListDelegate {
    type Item = ListItem;

    fn items_count(&self, _section: usize, _cx: &App) -> usize {
        self.group.as_ref().map_or(0, |group| group.members.len())
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _window: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let member = self.group.as_ref()?.members.get(ix.row)?;
        let detail = if self.testing.as_deref() == Some(member) {
            "测速中".to_string()
        } else if let Some(node) = self.nodes.get(member) {
            let delay = node
                .delay_ms
                .map(|delay| format!("{delay}ms"))
                .unwrap_or_else(|| "未测速 / 超时".into());
            format!(
                "{} · {}{}",
                node.node_type,
                delay,
                if node.udp { " · UDP" } else { "" }
            )
        } else if let Some(delay_ms) = self.member_delays.get(member) {
            delay_ms
                .map(|delay| format!("策略成员 · {delay}ms"))
                .unwrap_or_else(|| "策略成员 · 未测速 / 超时".into())
        } else {
            "策略成员 · 未测速".into()
        };
        let accessibility_label = format!("{member}，{detail}");
        Some(
            ListItem::new(ix)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .w_full()
                        .child(Label::new(member.clone()))
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(detail),
                        ),
                )
                .accessibility_label(accessibility_label)
                .selected(Some(ix) == self.selected_index),
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
            .child("暂无代理分组")
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
