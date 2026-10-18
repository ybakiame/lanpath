use gpui_kit::component::{
    ActiveTheme as _, IndexPath,
    label::Label,
    list::{ListDelegate, ListItem, ListState},
};
use gpui_kit::{App, Context, IntoElement, ParentElement as _, Styled as _, Window, div};
use lanpath_client::connections::ConnectionView;

#[derive(Default)]
pub(crate) struct ConnectionListDelegate {
    rows: Vec<ConnectionView>,
    selected_id: Option<String>,
}

impl ConnectionListDelegate {
    pub(crate) fn replace(&mut self, mut rows: Vec<ConnectionView>) -> bool {
        rows.sort_unstable_by(|a, b| a.start.cmp(&b.start).then_with(|| a.id.cmp(&b.id)));
        if self.rows == rows {
            return false;
        }
        if !rows
            .iter()
            .any(|row| Some(&row.id) == self.selected_id.as_ref())
        {
            self.selected_id = None;
        }
        self.rows = rows;
        true
    }

    pub(crate) fn selected_id(&self) -> Option<&str> {
        self.selected_id.as_deref()
    }

    pub(crate) fn selected_index(&self) -> Option<IndexPath> {
        let id = self.selected_id.as_ref()?;
        self.rows
            .iter()
            .position(|row| &row.id == id)
            .map(IndexPath::new)
    }

    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }
}

impl ListDelegate for ConnectionListDelegate {
    type Item = ListItem;

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.rows.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) -> Option<Self::Item> {
        let row = self.rows.get(ix.row)?;
        Some(
            ListItem::new(gpui_kit::SharedString::from(row.id.clone()))
                .accessibility_label(format!("{} · {}", row.host, row.id))
                .selected(self.selected_id.as_deref() == Some(row.id.as_str()))
                .child(Label::new(format!(
                    "{} · {} · {} · ↑ {} B · ↓ {} B",
                    row.host, row.network, row.chain, row.upload, row.download
                ))),
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
            .child("当前没有可显示的连接")
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        self.selected_id = ix
            .and_then(|ix| self.rows.get(ix.row))
            .map(|row| row.id.clone());
        cx.notify();
    }
}
