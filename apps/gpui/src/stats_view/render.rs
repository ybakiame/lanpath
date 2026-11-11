use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::Editor,
    list::List,
    scroll::ScrollableElement as _,
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use lanpath_client::stats::{DEFAULT_QUERY_LIMIT, MAX_QUERY_LIMIT};

use super::{
    StatsView,
    query::{daily_description, records_description},
};
use crate::observation::StatsKind;

impl Render for StatsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Metadata scrolls as one region, preserving space for actual results
        // at the minimum window size and with a larger interface scale.
        let metadata_height = (window.viewport_size().height * 0.34).min(
            (window.viewport_size().height - window.rem_size() * 16.0).max(window.rem_size() * 4.0)
                * 0.45,
        );
        let clearing = self.closing || self.requests.clearing();
        let busy = self.requests.busy();
        let operation = if clearing {
            Some("正在清空全部实验统计；等待已发出的读取结束…")
        } else if busy {
            Some("正在读取已提交统计，保留上次成功结果…")
        } else {
            None
        };
        let status = operation
            .map(str::to_string)
            .or_else(|| self.clear_error.as_ref().or(self.error.as_ref()).cloned())
            .or_else(|| self.notice.clone())
            .unwrap_or_else(|| {
                if self.running {
                    "运行时每 5 秒刷新；跟踪采集不随页面切换暂停".into()
                } else {
                    "核心停止后仍可查询历史；仅使用实验目录的统计数据库".into()
                }
            });
        let list = match self.kind {
            StatsKind::Daily => &self.daily_list,
            StatsKind::Records => &self.records_list,
        };
        let count = list.read(cx).delegate().len();
        let loaded = match self.kind {
            StatsKind::Daily => self.loaded_daily.as_ref().map(daily_description),
            StatsKind::Records => self.loaded_records.as_ref().map(records_description),
        };
        let limit = match self.kind {
            StatsKind::Daily => self.loaded_daily.as_ref().and_then(|query| query.limit),
            StatsKind::Records => self.loaded_records.as_ref().and_then(|query| query.limit),
        }
        .unwrap_or(DEFAULT_QUERY_LIMIT);
        let results = loaded
            .map(|loaded| {
                format!(
                    "上次成功查询：{loaded} · 返回 {count} 条{}",
                    if count >= limit as usize {
                        "（达到上限，可能还有匹配结果；可提高上限或缩小搜索范围）"
                    } else {
                        ""
                    }
                )
            })
            .unwrap_or_else(|| "尚未读取当前查询结果".into());

        div().flex().flex_col().flex_1().min_h_0().min_w_0().gap_3()
            .child(crate::design::page_header("统计", "本机流量与连接记录",
                crate::design::toolbar()
                    .child(Button::new("stats-apply-query").label("应用查询").primary().small()
                        .disabled(self.requests.clearing())
                        .on_click(cx.listener(|this, _, _, cx| this.apply_query(cx))))
                    .child(Button::new("stats-refresh").label("刷新").small().disabled(busy || clearing)
                        .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))))
                    .child(Button::new("stats-clear").label("清空统计…").small().disabled(clearing)
                        .on_click(cx.listener(|this, _, window, cx| this.request_clear(window, cx)))), cx))
            .child(div().id("stats-metadata").h(metadata_height).min_h_0().min_w_0().flex_shrink_0()
                .overflow_y_scrollbar()
                .child(div().flex().flex_col().gap_3().min_w_0()
            .children(self.summary.as_ref().map(|summary| {
                crate::design::toolbar()
                    .child(crate::design::metric("今日上传", crate::design::format_bytes(summary.upload_bytes), cx))
                    .child(crate::design::metric("今日下载", crate::design::format_bytes(summary.download_bytes), cx))
                    .child(crate::design::metric("今日连接", summary.connection_count.to_string(), cx))
            }))
            .child(gpui_kit::component::tab::TabBar::new("stats-kind").segmented()
                .selected_index(if self.kind == StatsKind::Daily { 0 } else { 1 })
                .child(gpui_kit::component::tab::Tab::new().label("日聚合"))
                .child(gpui_kit::component::tab::Tab::new().label("关闭明细"))
                .on_click(cx.listener(|this, ix: &usize, window, cx| {
                    this.select_kind(if *ix == 0 { StatsKind::Daily } else { StatsKind::Records }, window, cx);
                })))
            .child(div().id("stats-status").role(Role::Status).aria_label(status.clone())
                .text_sm().text_color(if !busy && !clearing && (self.clear_error.is_some() || self.error.is_some()) {
                    cx.theme().danger
                } else { cx.theme().muted_foreground }).child(status))
            .child(crate::design::surface(cx)
                .child(div().flex().flex_col().gap_3().p_4()
                    .child(self.render_filters(cx))
                    .child(crate::design::help("stats-query-help", "查询与采样说明",
                        div().flex().flex_col().gap_2().text_sm().text_color(cx.theme().muted_foreground)
                            .children(self.summary.as_ref().map(|summary| {
                                div().child(format!("今日汇总不受筛选影响 · 本地日期 {} · {} 个目标 · 已代理上传 {} / 下载 {}",
                                    summary.date, summary.target_count,
                                    crate::design::format_bytes(summary.proxied_upload_bytes),
                                    crate::design::format_bytes(summary.proxied_download_bytes)))
                            }))
                            .child(format!("修改条件后应用查询或按 Enter。上限 1–{MAX_QUERY_LIMIT}，不支持分页。{}",
                                match self.kind {
                                    StatsKind::Daily => "日聚合按本地单日筛选。",
                                    StatsKind::Records => "明细不支持日期边界或精确出站筛选；搜索覆盖目标、host、IP、规则和出站。",
                                }))
                            .child("采样可能漏记短连接与尾流量；字节为最后已知累计值，时间为本地时间。停止不执行最终结算。日聚合保留 90 天，明细保留 7 天，打开存储时清理过期记录。")))))
            .child(div().id("stats-results").role(Role::Status).aria_label(results.clone())
                .text_sm().text_color(cx.theme().muted_foreground).child(results))
                ))
            .child(crate::design::surface(cx).flex_1().min_h_0().child(List::new(list).size_full()))
            .children(self.detail_text.as_ref().map(|_| {
                div().id("stats-detail").role(Role::Group).aria_label("统计记录详情")
                    .flex().flex_col().gap_2().h_48().flex_shrink_0()
                    .max_h(gpui_kit::relative(0.25)).min_h_0().min_w_0()
                    .child("记录详情")
                    .child(div().flex_1().min_h_0().min_w_0()
                        .child(Editor::new(&self.details).readonly(true).size_full()))
            }))
    }
}
