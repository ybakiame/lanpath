use std::sync::Arc;

use gpui_kit::component::{
    ActiveTheme as _, IndexPath,
    label::Label,
    list::{ListDelegate, ListItem, ListState},
    tooltip::Tooltip,
};
use gpui_kit::{
    App, Context, Div, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, relative,
};
use lanpath_client::mitm::TrafficRecord;

pub(super) struct TrafficRow {
    id: SharedString,
    summary: SharedString,
    time: SharedString,
    method: SharedString,
    url: SharedString,
    status: SharedString,
    duration: SharedString,
}

impl TrafficRow {
    fn from_record(record: TrafficRecord) -> Self {
        let id = SharedString::from(record.id.to_string());
        let time = SharedString::from(record.timestamp.to_rfc3339());
        let method = SharedString::from(record.method);
        let url = SharedString::from(record.url);
        let status = SharedString::from(record.response_status.to_string());
        let duration = SharedString::from(format!("{} ms", record.duration_ms));
        let summary = SharedString::from(format!("{} {} · {} · {}", method, url, status, duration));
        Self {
            id,
            summary,
            time,
            method,
            url,
            status,
            duration,
        }
    }
}

#[derive(Default)]
pub(super) struct TrafficListDelegate {
    rows: Vec<Arc<TrafficRow>>,
    last_marker: Option<(usize, u128)>,
}

impl TrafficListDelegate {
    pub(super) fn replace(&mut self, records: Vec<TrafficRecord>) -> bool {
        let marker = records
            .last()
            .map(|record| (records.len(), record.id.as_u128()));
        if marker == self.last_marker {
            return false;
        }
        self.rows = records
            .into_iter()
            .rev()
            .map(TrafficRow::from_record)
            .map(Arc::new)
            .collect();
        self.last_marker = marker;
        true
    }

    pub(super) fn len(&self) -> usize {
        self.rows.len()
    }
}

// Share the same proportional budget with the header: secondary metadata may
// truncate, but never consumes the URL's remaining width at larger text scales.
pub(super) fn traffic_columns(
    time: impl Into<SharedString>,
    method: impl Into<SharedString>,
    url: impl Into<SharedString>,
    status: impl Into<SharedString>,
    duration: impl Into<SharedString>,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .w_full()
        .min_w_0()
        .child(
            Label::new(time)
                .w(relative(0.16))
                .min_w_0()
                .flex_shrink_0()
                .truncate(),
        )
        .child(
            Label::new(method)
                .w(relative(0.12))
                .min_w_0()
                .flex_shrink_0()
                .truncate(),
        )
        .child(Label::new(url).flex_1().min_w_0().truncate())
        .child(
            Label::new(status)
                .w(relative(0.10))
                .min_w_0()
                .flex_shrink_0()
                .truncate(),
        )
        .child(
            Label::new(duration)
                .w(relative(0.16))
                .min_w_0()
                .flex_shrink_0()
                .truncate(),
        )
}

impl ListDelegate for TrafficListDelegate {
    type Item = ListItem;

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.rows.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        _cx: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let row = self.rows.get(ix.row)?;
        let time = row.time.clone();
        let summary = row.summary.clone();
        Some(
            ListItem::new(row.id.clone())
                .accessibility_label(row.summary.clone())
                .text_sm()
                .tooltip(move |window, cx| {
                    Tooltip::new(format!("{time} · {summary}")).build(window, cx)
                })
                .child(traffic_columns(
                    row.time.clone(),
                    row.method.clone(),
                    row.url.clone(),
                    row.status.clone(),
                    row.duration.clone(),
                )),
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
            .child("暂无抓包记录；启动核心并让请求命中 Hostname 白名单后会显示在这里")
    }

    fn set_selected_index(
        &mut self,
        _: Option<IndexPath>,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) {
    }
}
