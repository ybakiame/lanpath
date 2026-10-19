use std::{sync::Arc, time::Duration};

use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _,
    button::Button,
    list::{List, ListState},
};
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _, Task,
    Window, div,
};
use lanpath_client::stats::TodaySummary;

use crate::{observation::StatsKind, runtime::DesktopRuntime, stats_list::StatsListDelegate};

pub(crate) struct StatsView {
    core: Arc<DesktopRuntime>,
    list: Entity<ListState<StatsListDelegate>>,
    summary: Option<TodaySummary>,
    error: Option<String>,
    kind: StatsKind,
    active: bool,
    running: bool,
    busy: bool,
    generation: u64,
    request_task: Option<Task<()>>,
    _poll_task: Task<()>,
}

impl StatsView {
    pub(crate) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let list =
            cx.new(|cx| ListState::new(StatsListDelegate::default(), window, cx).selectable(false));
        let poll_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(5)).await;
                if this
                    .update(cx, |this, cx| {
                        if this.active && this.running {
                            this.refresh(cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            core,
            list,
            summary: None,
            error: None,
            kind: StatsKind::Daily,
            active: false,
            running: false,
            busy: false,
            generation: 0,
            request_task: None,
            _poll_task: poll_task,
        }
    }

    pub(crate) fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.active = active;
        if active {
            self.refresh(cx);
        }
    }

    pub(crate) fn set_running(&mut self, running: bool, cx: &mut Context<Self>) {
        self.running = running;
        self.generation += 1;
        if self.active {
            self.refresh(cx);
        }
        cx.notify();
    }

    fn select_kind(&mut self, kind: StatsKind, cx: &mut Context<Self>) {
        if self.kind == kind {
            return;
        }
        self.kind = kind;
        self.generation += 1;
        // A stale result stays associated with its original segment, never
        // displayed under a different query's heading.
        self.list.update(cx, |list, cx| {
            *list.delegate_mut() = StatsListDelegate::default();
            cx.notify();
        });
        self.refresh(cx);
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.busy || !self.active {
            return;
        }
        self.busy = true;
        let generation = self.generation;
        let kind = self.kind;
        let core = self.core.clone();
        self.request_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.observations().and_then(|observations| {
                        core.block_on(observations.stats(kind))
                            .and_then(|result| result)
                    })
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                if generation != this.generation {
                    this.refresh(cx);
                    return;
                }
                match result {
                    Ok(snapshot) => {
                        this.summary = Some(snapshot.summary);
                        this.list.update(cx, |list, cx| {
                            list.delegate_mut().replace(snapshot.rows);
                            cx.notify();
                        });
                        this.error = None;
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

impl Render for StatsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let summary = self.summary.as_ref().map(|s| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(format!(
                    "本地日期 {} · {} 次连接 · {} 个目标",
                    s.date, s.connection_count, s.target_count
                ))
                .child(format!(
                    "总上传 {} B · 总下载 {} B · 已代理上传 {} B · 已代理下载 {} B",
                    s.upload_bytes,
                    s.download_bytes,
                    s.proxied_upload_bytes,
                    s.proxied_download_bytes
                ))
        });
        div().flex().flex_col().flex_1().min_h_0().gap_2()
            .child(div().flex().items_center().gap_2().child("持久统计")
                .child(Button::new("stats-refresh").label("刷新统计").disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx)))))
            .children(summary)
            .children(self.error.as_ref().map(|error| div().text_sm().text_color(cx.theme().danger).child(format!("统计读取失败，保留已读取结果：{error}"))))
            .child(div().text_sm().text_color(cx.theme().muted_foreground).child(if self.busy {
                "正在读取已提交统计…"
            } else if self.running {
                "运行时每 5 秒刷新；跟踪采集不随页面切换暂停"
            } else { "核心停止后仍可读取历史；统计目录由验证环境单独保存" }))
            .child(div().flex().items_center().gap_2()
                .child(Button::new("stats-daily").label("今日日聚合").selected(self.kind == StatsKind::Daily)
                    .on_click(cx.listener(|this, _, _, cx| this.select_kind(StatsKind::Daily, cx))))
                .child(Button::new("stats-records").label("持久关闭明细").selected(self.kind == StatsKind::Records)
                    .on_click(cx.listener(|this, _, _, cx| this.select_kind(StatsKind::Records, cx)))))
            .child(div().text_sm().text_color(cx.theme().muted_foreground).child(match self.kind {
                StatsKind::Daily => "今日复合维度聚合，按流量降序显示前 200 条；不是独立目标或出站排行榜",
                StatsKind::Records => "跨日期关闭明细，按检测关闭时间倒序显示前 200 条；字节为最后已知累计值，时间为 Unix 秒",
            }))
            .child(List::new(&self.list).size_full())
            .child(div().text_sm().text_color(cx.theme().muted_foreground).child("采样可能漏记短连接与尾流量；停止不执行最终结算。日聚合保留 90 天，明细保留 7 天，打开存储时清理过期记录。"))
    }
}
