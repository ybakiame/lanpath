use crate::observation::StatsRows;
use gpui_kit::component::{
    ActiveTheme as _, IndexPath,
    label::Label,
    list::{ListDelegate, ListItem, ListState},
};
use gpui_kit::{
    App, Context, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use lanpath_client::stats::DailyStatRow;

enum Selection {
    Daily(DailyStatRow),
    Record(i64),
}

#[derive(Default)]
pub(crate) struct StatsListDelegate {
    rows: Option<StatsRows>,
    selected: Option<Selection>,
}

impl StatsListDelegate {
    pub(crate) fn replace(&mut self, rows: StatsRows) {
        self.rows = Some(rows);
        if self.selected_index().is_none() {
            self.selected = None;
        }
    }

    pub(crate) fn len(&self) -> usize {
        match &self.rows {
            Some(StatsRows::Daily(rows)) => rows.len(),
            Some(StatsRows::Records(rows)) => rows.len(),
            None => 0,
        }
    }

    pub(crate) fn selected_index(&self) -> Option<IndexPath> {
        self.selected.as_ref()?;
        (0..self.len())
            .find(|&ix| self.is_selected(ix))
            .map(IndexPath::new)
    }

    fn is_selected(&self, ix: usize) -> bool {
        match (&self.rows, &self.selected) {
            (Some(StatsRows::Daily(rows)), Some(Selection::Daily(key))) => {
                rows.get(ix).is_some_and(|row| {
                    row.date == key.date
                        && row.target == key.target
                        && row.destination_ip == key.destination_ip
                        && row.rule == key.rule
                        && row.rule_payload == key.rule_payload
                        && row.outbound == key.outbound
                })
            }
            (Some(StatsRows::Records(rows)), Some(Selection::Record(id))) => {
                rows.get(ix).is_some_and(|row| row.id == *id)
            }
            _ => false,
        }
    }

    pub(crate) fn selected_text(&self) -> Option<SharedString> {
        let ix = self.selected_index()?.row;
        let text = match self.rows.as_ref()? {
            StatsRows::Daily(rows) => {
                let row = rows.get(ix)?;
                format!(
                    "本地日期：{}\n目标：{}\n目标 IP：{}\n命中规则：{} {}\n出站：{}\n上传：{} B · 下载：{} B\n连接数：{}\n最后出现：{}",
                    row.date,
                    row.target,
                    row.destination_ip,
                    row.rule,
                    row.rule_payload,
                    row.outbound,
                    row.upload_bytes,
                    row.download_bytes,
                    row.conn_count,
                    format_time(row.last_seen)
                )
            }
            StatsRows::Records(rows) => {
                let row = rows.get(ix)?;
                format!(
                    "记录 ID：{}\n目标：{}\nHost：{}\n目标 IP：{}\n协议：{}\n命中规则：{} {}\n出站：{}\n完整出站链：{}\n上传：{} B · 下载：{} B\n开始：{}\n检测关闭：{}",
                    row.id,
                    row.target,
                    row.host,
                    row.destination_ip,
                    row.network,
                    row.rule,
                    row.rule_payload,
                    row.outbound,
                    row.chain,
                    row.upload,
                    row.download,
                    format_time(row.started_at),
                    format_time(row.ended_at)
                )
            }
        };
        Some(text.into())
    }

    fn row_info(&self, ix: IndexPath) -> Option<(SharedString, SharedString)> {
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
                        "{} · {} · {} · {} {} · {} · ↑ {} B · ↓ {} B · {} 次连接 · 最后出现 {}",
                        row.date,
                        row.target,
                        row.destination_ip,
                        row.rule,
                        row.rule_payload,
                        row.outbound,
                        row.upload_bytes,
                        row.download_bytes,
                        row.conn_count,
                        format_time(row.last_seen)
                    ),
                )
            }
            StatsRows::Records(rows) => {
                let row = rows.get(ix.row)?;
                (
                    SharedString::from(format!("record-{}", row.id)),
                    format!(
                        "{} · {} · {} · {} · {} · {} {} · {} · 记录 {} · ↑ {} B · ↓ {} B · 开始 {} · 检测关闭 {}",
                        row.target,
                        row.host,
                        row.destination_ip,
                        row.network,
                        row.outbound,
                        row.rule,
                        row.rule_payload,
                        row.chain,
                        row.id,
                        row.upload,
                        row.download,
                        format_time(row.started_at),
                        format_time(row.ended_at)
                    ),
                )
            }
        };
        Some((id, text.into()))
    }
}

impl ListDelegate for StatsListDelegate {
    type Item = ListItem;

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let (id, text) = self.row_info(ix)?;
        Some(
            ListItem::new(id)
                .role(Role::ListItem)
                .selected(self.is_selected(ix.row))
                .accessibility_label(text.clone())
                .child(Label::new(text).w_full().min_w_0().h_5().truncate()),
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

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        self.selected = ix.and_then(|ix| match self.rows.as_ref()? {
            StatsRows::Daily(rows) => rows.get(ix.row).cloned().map(Selection::Daily),
            StatsRows::Records(rows) => rows.get(ix.row).map(|row| Selection::Record(row.id)),
        });
        cx.notify();
    }
}

fn format_time(timestamp: i64) -> String {
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S %:z")
                .to_string()
        })
        .unwrap_or_else(|| format!("Unix {timestamp}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_selection_survives_reordering_and_counter_updates_but_not_key_changes() {
        let row = DailyStatRow {
            date: "2026-10-21".into(),
            target: "target.example".into(),
            destination_ip: "192.0.2.1".into(),
            rule: "DOMAIN".into(),
            rule_payload: "target.example".into(),
            outbound: "proxy".into(),
            upload_bytes: 10,
            download_bytes: 20,
            conn_count: 1,
            last_seen: 0,
        };
        let mut other = row.clone();
        other.destination_ip = "192.0.2.2".into();
        let mut list = StatsListDelegate::default();
        list.replace(StatsRows::Daily(vec![row.clone(), other.clone()]));
        list.selected = Some(Selection::Daily(row.clone()));
        let mut updated = row;
        updated.upload_bytes = 100;
        updated.conn_count = 2;
        list.replace(StatsRows::Daily(vec![other, updated.clone()]));
        assert_eq!(list.selected_index(), Some(IndexPath::new(1)));
        updated.rule_payload = "different.example".into();
        list.replace(StatsRows::Daily(vec![updated]));
        assert!(list.selected_index().is_none());
        assert!(list.selected_text().is_none());
    }
}
