use std::sync::Arc;

use gpui_kit::component::{
    ActiveTheme as _, IndexPath,
    label::Label,
    list::{ListDelegate, ListItem, ListState},
};
use gpui_kit::{
    App, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
};

use crate::log_buffer::LogEntry;

#[derive(Default)]
pub(crate) struct LogListDelegate {
    rows: Vec<Arc<LogEntry>>,
    selected_id: Option<u64>,
}

impl LogListDelegate {
    pub(crate) fn replace(&mut self, rows: Vec<Arc<LogEntry>>) {
        if self
            .selected_id
            .is_some_and(|id| !rows.iter().any(|row| row.id == id))
        {
            self.selected_id = None;
        }
        self.rows = rows;
    }

    pub(crate) fn selected_index(&self) -> Option<IndexPath> {
        let id = self.selected_id?;
        self.rows
            .iter()
            .position(|row| row.id == id)
            .map(IndexPath::new)
    }

    pub(crate) fn selected(&self) -> Option<(u64, SharedString)> {
        let index = self.selected_index()?;
        let row = &self.rows[index.row];
        Some((row.id, row.line.clone().into()))
    }
}

impl ListDelegate for LogListDelegate {
    type Item = ListItem;

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.rows.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let row = self.rows.get(ix.row)?;
        let color = match row.rank {
            4.. => cx.theme().danger,
            3 => cx.theme().warning,
            _ => cx.theme().foreground,
        };
        Some(
            ListItem::new(("log", row.id))
                .selected(self.selected_id == Some(row.id))
                .accessibility_label(SharedString::from(row.line.clone()))
                .child(
                    Label::new(SharedString::from(row.line.clone()))
                        .w_full()
                        .min_w_0()
                        .h_5()
                        .truncate()
                        .text_color(color),
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
            .child("暂无符合级别的已采集日志")
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        self.selected_id = ix.and_then(|ix| self.rows.get(ix.row)).map(|row| row.id);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incoming_entries_do_not_retarget_selection_and_eviction_clears_it() {
        let row = |id| {
            Arc::new(LogEntry {
                id,
                rank: 2,
                line: "event".into(),
            })
        };
        let mut list = LogListDelegate::default();
        list.replace(vec![row(2), row(1)]);
        list.selected_id = Some(1);
        list.replace(vec![row(3), row(2), row(1)]);
        assert_eq!(list.selected_index(), Some(IndexPath::new(2)));
        assert_eq!(list.selected().map(|(id, _)| id), Some(1));
        list.replace(vec![row(4), row(3)]);
        assert!(list.selected().is_none());
        list.replace(vec![row(5), row(4)]);
        assert!(list.selected_index().is_none());
    }
}
