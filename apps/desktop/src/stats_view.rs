use std::{sync::Arc, time::Duration};

use gpui_kit::component::{WindowExt as _, input::EditorState, list::ListState};
use gpui_kit::{AppContext as _, Context, Entity, SharedString, Subscription, Task, Window};
use lanpath_client::stats::{DailyQuery, RecordsQuery, TodaySummary};

use crate::{
    observation::{StatsKind, StatsRows, StatsSnapshot},
    runtime::DesktopRuntime,
    stats_list::StatsListDelegate,
};

mod query;
mod render;
mod requests;

use query::QueryInputs;
use requests::{Request, StatsRequests};

pub(crate) struct StatsView {
    core: Arc<DesktopRuntime>,
    daily_list: Entity<ListState<StatsListDelegate>>,
    records_list: Entity<ListState<StatsListDelegate>>,
    details: Entity<EditorState>,
    detail_text: Option<SharedString>,
    _list_subscriptions: Vec<Subscription>,
    inputs: QueryInputs,
    daily_query: DailyQuery,
    records_query: RecordsQuery,
    loaded_daily: Option<DailyQuery>,
    loaded_records: Option<RecordsQuery>,
    summary: Option<TodaySummary>,
    error: Option<String>,
    clear_error: Option<String>,
    notice: Option<String>,
    kind: StatsKind,
    active: bool,
    running: bool,
    closing: bool,
    requests: StatsRequests,
    request_task: Option<Task<()>>,
    _input_subscriptions: Vec<Subscription>,
    _poll_task: Task<()>,
}

impl StatsView {
    pub(crate) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let daily_list = cx.new(|cx| ListState::new(StatsListDelegate::default(), window, cx));
        let records_list = cx.new(|cx| ListState::new(StatsListDelegate::default(), window, cx));
        let details = cx.new(|cx| EditorState::new(window, cx).line_number(false));
        let list_subscriptions = [&daily_list, &records_list]
            .into_iter()
            .map(|list| {
                cx.observe_in(list, window, |this, list, window, cx| {
                    list.update(cx, |list, cx| {
                        let selected = list.delegate().selected_index();
                        if list.selected_index() != selected {
                            list.set_selected_index(selected, window, cx);
                        }
                    });
                    this.sync_details(window, cx);
                    cx.notify();
                })
            })
            .collect();
        let inputs = QueryInputs::new(window, cx);
        let input_subscriptions = inputs.subscribe(cx);
        let daily_query = inputs.daily_query(cx).expect("valid initial daily query");
        let records_query = inputs
            .records_query(cx)
            .expect("valid initial records query");
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
            daily_list,
            records_list,
            details,
            detail_text: None,
            _list_subscriptions: list_subscriptions,
            inputs,
            daily_query,
            records_query,
            loaded_daily: None,
            loaded_records: None,
            summary: None,
            error: None,
            clear_error: None,
            notice: None,
            kind: StatsKind::Daily,
            active: false,
            running: false,
            closing: false,
            requests: StatsRequests::default(),
            request_task: None,
            _input_subscriptions: input_subscriptions,
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
        self.requests.invalidate();
        self.refresh(cx);
        cx.notify();
    }

    pub(crate) fn busy(&self) -> bool {
        self.requests.busy() || self.requests.clearing()
    }

    pub(crate) fn set_closing(&mut self, closing: bool, cx: &mut Context<Self>) {
        // Parent observers propagate the current lock repeatedly. Equal state
        // must not refresh or enqueue another notification back to the parent.
        if self.closing == closing {
            return;
        }
        self.closing = closing;
        if !closing {
            self.refresh(cx);
        }
        cx.notify();
    }

    fn select_kind(&mut self, kind: StatsKind, window: &mut Window, cx: &mut Context<Self>) {
        self.kind = kind;
        self.sync_details(window, cx);
        cx.notify();
    }

    fn sync_details(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let list = match self.kind {
            StatsKind::Daily => &self.daily_list,
            StatsKind::Records => &self.records_list,
        };
        let text = list.read(cx).delegate().selected_text();
        if self.detail_text != text {
            self.details.update(cx, |editor, cx| {
                editor.set_value(text.clone().unwrap_or_default(), window, cx)
            });
            self.detail_text = text;
        }
    }

    fn apply_query(&mut self, cx: &mut Context<Self>) {
        if self.closing || self.requests.clearing() {
            return;
        }
        let result = match self.kind {
            StatsKind::Daily => self
                .inputs
                .daily_query(cx)
                .map(|query| self.daily_query = query),
            StatsKind::Records => self
                .inputs
                .records_query(cx)
                .map(|query| self.records_query = query),
        };
        if let Err(error) = result {
            self.error = Some(error);
        } else {
            self.error = None;
            self.notice = None;
            self.requests.invalidate();
            self.refresh(cx);
        }
        cx.notify();
    }

    fn request_clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing || self.requests.clearing() {
            return;
        }
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .title("清空全部实验统计？")
                .description("删除实验目录中的所有日期日聚合和持久关闭明细，不受当前筛选条件限制，且无法撤销。生产 Lanpath 数据不受影响；运行中的跟踪器会继续采集新的统计。")
                .confirm()
                .ok_text("清空统计")
                .cancel_text("取消")
                .on_ok(move |_, _, cx| {
                    let _ = view.update(cx, |view, cx| {
                        if view.closing { return; }
                        view.requests.request_clear();
                        view.notice = None;
                        view.clear_error = None;
                        view.refresh(cx);
                        cx.notify();
                    });
                    true
                })
        });
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.requests.begin(self.active && !self.closing) else {
            return;
        };
        let core = self.core.clone();
        let daily = self.daily_query.clone();
        let records = self.records_query.clone();
        self.request_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let observations = core.observations()?;
                    match request {
                        Request::Clear => core.block_on(observations.clear_stats())?.map(|_| None),
                        Request::Query(_) => core
                            .block_on(observations.stats(&daily, &records))?
                            .map(|snapshot| Some((snapshot, daily, records))),
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let accepted = this.requests.complete(request);
                if request == Request::Clear {
                    // Even a failed clear may have removed one table. Never
                    // retain a pre-clear cache or accept an older queued read.
                    this.invalidate_snapshot(cx);
                    match result {
                        Ok(_) => {
                            this.clear_error = None;
                            this.notice = Some("已清空实验统计；跟踪采集不会暂停".into());
                        }
                        Err(error) => this.clear_error = Some(format!("清空实验统计失败：{error}")),
                    }
                    this.refresh(cx);
                } else if !accepted {
                    this.refresh(cx);
                } else {
                    match result {
                        Ok(Some((snapshot, daily, records))) => {
                            this.replace_snapshot(snapshot, daily, records, cx);
                            this.error = None;
                        }
                        Err(error) => {
                            this.error = Some(format!("统计读取失败，保留上次成功结果：{error}"))
                        }
                        Ok(None) => unreachable!("query returns a statistics snapshot"),
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn replace_snapshot(
        &mut self,
        snapshot: StatsSnapshot,
        mut daily: DailyQuery,
        records: RecordsQuery,
        cx: &mut Context<Self>,
    ) {
        if daily.date.is_none() {
            daily.date = Some(snapshot.summary.date.clone());
        }
        self.summary = Some(snapshot.summary);
        self.loaded_daily = Some(daily);
        self.loaded_records = Some(records);
        self.daily_list.update(cx, |list, cx| {
            list.delegate_mut()
                .replace(StatsRows::Daily(snapshot.daily));
            cx.notify();
        });
        self.records_list.update(cx, |list, cx| {
            list.delegate_mut()
                .replace(StatsRows::Records(snapshot.records));
            cx.notify();
        });
    }

    fn invalidate_snapshot(&mut self, cx: &mut Context<Self>) {
        self.summary = None;
        self.loaded_daily = None;
        self.loaded_records = None;
        self.error = None;
        for list in [&self.daily_list, &self.records_list] {
            list.update(cx, |list, cx| {
                *list.delegate_mut() = StatsListDelegate::default();
                cx.notify();
            });
        }
    }
}
