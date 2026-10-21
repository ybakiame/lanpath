use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, button::Button,
    input::Editor, list::List,
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

        div().flex().flex_col().flex_1().min_h_0().gap_2()
            .child(div().flex().flex_wrap().items_center().gap_2()
                .child("持久统计")
                .child(Button::new("stats-refresh").label("刷新统计").small().disabled(busy || clearing)
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))))
                .child(Button::new("stats-clear").label("清空全部统计…").small().disabled(clearing)
                    .on_click(cx.listener(|this, _, window, cx| this.request_clear(window, cx)))))
            .children(self.summary.as_ref().map(|summary| {
                div().flex().flex_col().gap_1().text_sm()
                    .child(format!("今日汇总（不受筛选影响）· 本地日期 {} · {} 次连接 · {} 个目标",
                        summary.date, summary.connection_count, summary.target_count))
                    .child(format!("总上传 {} B · 总下载 {} B · 已代理上传 {} B · 已代理下载 {} B",
                        summary.upload_bytes, summary.download_bytes,
                        summary.proxied_upload_bytes, summary.proxied_download_bytes))
            }))
            .child(div().id("stats-status").role(Role::Status).aria_label(status.clone())
                .text_sm().text_color(if !busy && !clearing && (self.clear_error.is_some() || self.error.is_some()) {
                    cx.theme().danger
                } else { cx.theme().muted_foreground }).child(status))
            .child(div().flex().items_center().gap_2()
                .child(Button::new("stats-daily").label("日聚合").small().selected(self.kind == StatsKind::Daily)
                    .on_click(cx.listener(|this, _, window, cx| this.select_kind(StatsKind::Daily, window, cx))))
                .child(Button::new("stats-records").label("持久关闭明细").small().selected(self.kind == StatsKind::Records)
                    .on_click(cx.listener(|this, _, window, cx| this.select_kind(StatsKind::Records, window, cx)))))
            .child(self.render_filters(cx))
            .child(div().text_sm().text_color(cx.theme().muted_foreground)
                .child(format!("修改条件后应用查询或按 Enter；每个视图独立保存条件。返回上限 1–{MAX_QUERY_LIMIT}，不支持分页。{}",
                    match self.kind {
                        StatsKind::Daily => "日聚合按本地单日筛选，不是独立目标或出站排行榜。",
                        StatsKind::Records => "明细查询 API 不支持日期边界或精确出站筛选；搜索覆盖目标、host、IP、规则和出站。",
                    })))
            .child(div().id("stats-results").role(Role::Status).aria_label(results.clone())
                .text_sm().child(results))
            .child(div().flex_1().min_h_0().child(List::new(list).size_full()))
            .children(self.detail_text.as_ref().map(|_| {
                div().id("stats-detail").role(Role::Group).aria_label("统计记录详情")
                    .flex().flex_col().gap_1().h_48().flex_shrink_0()
                    .child("统计记录详情（只读，可选择和复制）")
                    .child(Editor::new(&self.details).readonly(true).size_full())
            }))
            .child(div().text_sm().text_color(cx.theme().muted_foreground)
                .child("采样可能漏记短连接与尾流量；字节为最后已知累计值，时间显示为本地时间。停止不执行最终结算。日聚合保留 90 天，明细保留 7 天，打开存储时清理过期记录。"))
    }
}
