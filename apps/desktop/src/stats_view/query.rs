use gpui_kit::component::{
    Disableable as _, IndexPath, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::{Input, InputEvent, InputState},
    select::{Select, SelectState},
};
use gpui_kit::{
    App, AppContext as _, Context, Div, Entity, IntoElement, ParentElement as _, Styled as _,
    Subscription, Window, div,
};
use lanpath_client::stats::{
    DEFAULT_QUERY_LIMIT, DailyQuery, DailySort, MAX_QUERY_LIMIT, RecordSort, RecordsQuery,
};

use super::StatsView;
use crate::observation::StatsKind;

const DAILY_SORTS: [&str; 5] = ["总流量", "上传", "下载", "连接数", "最后出现"];
const RECORD_SORTS: [&str; 5] = ["检测关闭时间", "开始时间", "总流量", "上传", "下载"];

struct QueryForm {
    search: Entity<InputState>,
    limit: Entity<InputState>,
    sort: Entity<SelectState<Vec<&'static str>>>,
    direction: Entity<SelectState<Vec<&'static str>>>,
}

pub(super) struct QueryInputs {
    daily: QueryForm,
    records: QueryForm,
    date: Entity<InputState>,
    outbound: Entity<InputState>,
}

impl QueryForm {
    fn new(sorts: Vec<&'static str>, window: &mut Window, cx: &mut Context<StatsView>) -> Self {
        Self {
            search: cx.new(|cx| InputState::new(window, cx).placeholder("目标、IP、规则、出站")),
            limit: cx.new(|cx| {
                InputState::new(window, cx).default_value(DEFAULT_QUERY_LIMIT.to_string())
            }),
            sort: cx.new(|cx| SelectState::new(sorts, Some(IndexPath::default()), window, cx)),
            direction: cx.new(|cx| {
                SelectState::new(vec!["降序", "升序"], Some(IndexPath::default()), window, cx)
            }),
        }
    }

    fn search(&self, cx: &App) -> Option<String> {
        optional_text(self.search.read(cx).value().as_ref())
    }

    fn limit(&self, cx: &App) -> Result<u32, String> {
        let value = self.limit.read(cx).value();
        value
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|limit| (1..=MAX_QUERY_LIMIT).contains(limit))
            .ok_or_else(|| format!("查询上限必须为 1–{MAX_QUERY_LIMIT} 的整数"))
    }

    fn desc(&self, cx: &App) -> bool {
        self.direction.read(cx).selected_value().copied() != Some("升序")
    }

    fn render(&self, prefix: &'static str, disabled: bool) -> impl IntoElement {
        div()
            .flex()
            .flex_wrap()
            .items_end()
            .gap_2()
            .child(
                input_field(prefix, "搜索", &self.search, disabled)
                    .flex_1()
                    .min_w_48(),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .w_40()
                    .text_sm()
                    .child("排序字段")
                    .child(
                        Select::new(&self.sort)
                            .accessibility_label("统计排序字段")
                            .small()
                            .disabled(disabled)
                            .w_full(),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .w_24()
                    .text_sm()
                    .child("顺序")
                    .child(
                        Select::new(&self.direction)
                            .accessibility_label("统计排序顺序")
                            .small()
                            .disabled(disabled)
                            .w_full(),
                    ),
            )
            .child(input_field("stats-query-limit", "返回上限", &self.limit, disabled).w_24())
    }
}

impl QueryInputs {
    pub(super) fn new(window: &mut Window, cx: &mut Context<StatsView>) -> Self {
        Self {
            daily: QueryForm::new(DAILY_SORTS.to_vec(), window, cx),
            records: QueryForm::new(RECORD_SORTS.to_vec(), window, cx),
            date: cx.new(|cx| InputState::new(window, cx).placeholder("YYYY-MM-DD；留空为今天")),
            outbound: cx
                .new(|cx| InputState::new(window, cx).placeholder("精确出站 tag；留空为全部")),
        }
    }

    pub(super) fn subscribe(&self, cx: &mut Context<StatsView>) -> Vec<Subscription> {
        [
            &self.daily.search,
            &self.daily.limit,
            &self.records.search,
            &self.records.limit,
            &self.date,
            &self.outbound,
        ]
        .into_iter()
        .map(|input| {
            cx.subscribe(input, |view, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    view.apply_query(cx);
                }
            })
        })
        .collect()
    }

    pub(super) fn daily_query(&self, cx: &App) -> Result<DailyQuery, String> {
        let date = parse_date(self.date.read(cx).value().as_ref())?;
        let sort = match self.daily.sort.read(cx).selected_value().copied() {
            Some("上传") => DailySort::Upload,
            Some("下载") => DailySort::Download,
            Some("连接数") => DailySort::Count,
            Some("最后出现") => DailySort::LastSeen,
            _ => DailySort::Total,
        };
        Ok(DailyQuery {
            date,
            search: self.daily.search(cx),
            outbound: optional_text(self.outbound.read(cx).value().as_ref()),
            sort,
            desc: Some(self.daily.desc(cx)),
            limit: Some(self.daily.limit(cx)?),
        })
    }

    pub(super) fn records_query(&self, cx: &App) -> Result<RecordsQuery, String> {
        let sort = match self.records.sort.read(cx).selected_value().copied() {
            Some("上传") => RecordSort::Upload,
            Some("下载") => RecordSort::Download,
            Some("总流量") => RecordSort::Total,
            Some("开始时间") => RecordSort::Started,
            _ => RecordSort::Ended,
        };
        Ok(RecordsQuery {
            search: self.records.search(cx),
            sort,
            desc: Some(self.records.desc(cx)),
            limit: Some(self.records.limit(cx)?),
        })
    }
}

impl StatsView {
    pub(super) fn render_filters(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.requests.clearing();
        let daily = self.kind == StatsKind::Daily;
        let form = if daily {
            &self.inputs.daily
        } else {
            &self.inputs.records
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(form.render(
                if daily {
                    "stats-daily-search"
                } else {
                    "stats-records-search"
                },
                disabled,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_end()
                    .gap_2()
                    .children(daily.then(|| {
                        input_field(
                            "stats-date",
                            "聚合日期（本地）",
                            &self.inputs.date,
                            disabled,
                        )
                        .w_64()
                    }))
                    .children(daily.then(|| {
                        input_field(
                            "stats-outbound",
                            "精确出站",
                            &self.inputs.outbound,
                            disabled,
                        )
                        .w_64()
                    }))
                    .child(
                        Button::new("stats-apply-query")
                            .label("应用查询")
                            .primary()
                            .small()
                            .disabled(disabled)
                            .on_click(cx.listener(|view, _, _, cx| view.apply_query(cx))),
                    ),
            )
    }
}

fn input_field(
    id: &'static str,
    label: &'static str,
    state: &Entity<InputState>,
    disabled: bool,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .text_sm()
        .min_w_0()
        .child(label)
        .child(
            Input::new(state)
                .accessibility_id(id)
                .aria_label(label)
                .small()
                .disabled(disabled),
        )
}

fn optional_text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn parse_date(value: &str) -> Result<Option<String>, String> {
    let Some(value) = optional_text(value) else {
        return Ok(None);
    };
    chrono::NaiveDate::parse_from_str(&value, "%Y-%m-%d")
        .ok()
        .filter(|date| date.format("%Y-%m-%d").to_string() == value)
        .map(|_| Some(value))
        .ok_or_else(|| "聚合日期必须为有效的 YYYY-MM-DD 本地日期；留空为今天".into())
}

pub(super) fn daily_description(query: &DailyQuery) -> String {
    let sort = match query.sort {
        DailySort::Upload => "上传",
        DailySort::Download => "下载",
        DailySort::Total => "总流量",
        DailySort::Count => "连接数",
        DailySort::LastSeen => "最后出现",
    };
    format!(
        "日聚合 · 日期 {} · 搜索 {} · 出站 {} · {}{} · 上限 {}",
        query.date.as_deref().unwrap_or("今天"),
        query.search.as_deref().unwrap_or("全部"),
        query.outbound.as_deref().unwrap_or("全部"),
        sort,
        if query.desc.unwrap_or(true) {
            "降序"
        } else {
            "升序"
        },
        query.limit.unwrap_or(DEFAULT_QUERY_LIMIT)
    )
}

pub(super) fn records_description(query: &RecordsQuery) -> String {
    let sort = match query.sort {
        RecordSort::Upload => "上传",
        RecordSort::Download => "下载",
        RecordSort::Total => "总流量",
        RecordSort::Started => "开始时间",
        RecordSort::Ended => "检测关闭时间",
    };
    format!(
        "关闭明细 · 搜索 {} · {}{} · 上限 {}",
        query.search.as_deref().unwrap_or("全部"),
        sort,
        if query.desc.unwrap_or(true) {
            "降序"
        } else {
            "升序"
        },
        query.limit.unwrap_or(DEFAULT_QUERY_LIMIT)
    )
}

#[cfg(test)]
mod tests {
    use super::parse_date;

    #[test]
    fn historical_date_accepts_leap_day_but_rejects_impossible_or_noncanonical_dates() {
        assert_eq!(
            parse_date(" 2024-02-29 ").unwrap().as_deref(),
            Some("2024-02-29")
        );
        assert!(parse_date("2025-02-29").is_err());
        assert!(parse_date("2024-2-29").is_err());
        assert_eq!(parse_date("  ").unwrap(), None);
    }
}
