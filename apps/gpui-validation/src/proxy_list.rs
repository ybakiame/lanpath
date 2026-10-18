use gpui_kit::component::{
    ActiveTheme as _, IndexPath,
    label::Label,
    list::{ListDelegate, ListItem, ListState},
};
use gpui_kit::{App, Context, IntoElement, ParentElement as _, Styled as _, Window, div};
use lanpath_client::proxies::GroupView;

#[derive(Default)]
pub(crate) struct ProxyListDelegate {
    group: Option<GroupView>,
    selected_index: Option<IndexPath>,
}

impl ProxyListDelegate {
    pub(crate) fn set_group(&mut self, group: Option<GroupView>) {
        self.selected_index = group
            .as_ref()
            .and_then(|group| group.members.iter().position(|member| member == &group.now))
            .map(IndexPath::new);
        self.group = group;
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
        _cx: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let member = self.group.as_ref()?.members.get(ix.row)?;
        Some(
            ListItem::new(ix)
                .child(Label::new(member.clone()))
                .accessibility_label(member.clone())
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
