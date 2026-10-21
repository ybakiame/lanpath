use gpui_kit::component::{
    ActiveTheme as _, IndexPath,
    label::Label,
    list::{ListDelegate, ListItem, ListState},
};
use gpui_kit::{
    App, Context, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

#[derive(Default)]
pub(super) struct DiskFileList {
    names: Vec<String>,
    selected: Option<String>,
}

impl DiskFileList {
    pub(super) fn replace(&mut self, names: Vec<String>) {
        if !self
            .selected
            .as_ref()
            .is_some_and(|name| names.contains(name))
        {
            self.selected = names.first().cloned();
        }
        self.names = names;
    }

    pub(super) fn selected(&self) -> Option<String> {
        self.selected.clone()
    }

    pub(super) fn selected_index(&self) -> Option<IndexPath> {
        let name = self.selected.as_ref()?;
        self.names
            .iter()
            .position(|item| item == name)
            .map(IndexPath::new)
    }
}

impl ListDelegate for DiskFileList {
    type Item = ListItem;

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.names.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let name = self.names.get(ix.row)?;
        Some(
            ListItem::new(SharedString::from(format!("disk-log-file:{name}")))
                .role(Role::ListItem)
                .selected(self.selected.as_ref() == Some(name))
                .accessibility_label(SharedString::from(format!("日志文件 {name}")))
                .child(
                    Label::new(SharedString::from(name.clone()))
                        .w_full()
                        .min_w_0()
                        .truncate(),
                ),
        )
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        div()
            .p_4()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child("暂无此类型日志；启动实验应用或核心后刷新")
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        self.selected = ix.and_then(|ix| self.names.get(ix.row)).cloned();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_keeps_file_identity_and_deleted_file_selects_remaining_file() {
        let mut files = DiskFileList::default();
        files.replace(vec!["app.log.2".into(), "core.log".into()]);
        files.selected = Some("core.log".into());
        files.replace(vec![
            "app.log.3".into(),
            "app.log.2".into(),
            "core.log".into(),
        ]);
        assert_eq!(files.selected_index(), Some(IndexPath::new(2)));
        files.replace(vec!["app.log.3".into()]);
        assert_eq!(files.selected().as_deref(), Some("app.log.3"));
        files.replace(Vec::new());
        assert!(files.selected().is_none());
    }
}
