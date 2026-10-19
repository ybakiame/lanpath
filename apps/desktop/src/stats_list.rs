use gpui_kit::component::{
    ActiveTheme as _, IndexPath,
    label::Label,
    list::{ListDelegate, ListItem, ListState},
};
use gpui_kit::{
    App, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div,
};

use crate::observation::StatsRows;

#[derive(Default)]
pub(crate) struct StatsListDelegate {
    rows: Option<StatsRows>,
}

impl StatsListDelegate {
    pub(crate) fn replace(&mut self, rows: StatsRows) {
        self.rows = Some(rows);
    }
}

impl ListDelegate for StatsListDelegate {
    type Item = ListItem;

    fn items_count(&self, _: usize, _: &App) -> usize {
        match &self.rows {
            Some(StatsRows::Daily(rows)) => rows.len(),
            Some(StatsRows::Records(rows)) => rows.len(),
            None => 0,
        }
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let (id, text) = match self.rows.as_ref()? {
            StatsRows::Daily(rows) => {
                let row = rows.get(ix.row)?;
                let id = format!(
                    "{:?}",
                    (
                        &row.date,
                        &row.target,
                        &row.destination_ip,
                        &row.rule,
                        &row.rule_payload,
                        &row.outbound
                    )
                );
                (
                    SharedString::from(id),
                    format!(
                        "{} · {} · {} {} · {} · ↑ {} B · ↓ {} B · {} 次连接",
                        row.target,
                        row.destination_ip,
                        row.rule,
                        row.rule_payload,
                        row.outbound,
                        row.upload_bytes,
                        row.download_bytes,
                        row.conn_count
                    ),
                )
            }
            StatsRows::Records(rows) => {
                let row = rows.get(ix.row)?;
                (
                    SharedString::from(format!("record-{}", row.id)),
                    format!(
                        "{} · {} · {} · ↑ {} B · ↓ {} B · 开始 {} · 检测关闭 {}",
                        row.host,
                        row.network,
                        row.chain,
                        row.upload,
                        row.download,
                        row.started_at,
                        row.ended_at
                    ),
                )
            }
        };
        let text = SharedString::from(text);
        Some(
            ListItem::new(id)
                .accessibility_label(text.clone())
                .child(Label::new(text)),
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
            .child(if self.rows.is_some() {
                "没有匹配的已提交记录"
            } else {
                "尚未读取统计记录"
            })
    }

    // Stats lists are explicitly non-selectable; no selection state is owned here.
    fn set_selected_index(
        &mut self,
        _: Option<IndexPath>,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) {
    }
}
