mod disk_logs;
mod render;

use std::{sync::Arc, time::Duration};

use gpui_kit::component::{input::EditorState, list::ListState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Task, Window};

use crate::{log_buffer::CoreLogState, log_list::LogListDelegate, runtime::DesktopRuntime};

pub(crate) struct LogsView {
    core: Arc<DesktopRuntime>,
    disk: Entity<disk_logs::DiskLogsView>,
    disk_visible: bool,
    list: Entity<ListState<LogListDelegate>>,
    details: Entity<EditorState>,
    detail_id: Option<u64>,
    active: bool,
    auto_refresh: bool,
    min_rank: u8,
    limit: usize,
    revision: Option<u64>,
    shown: usize,
    total: usize,
    evicted: u64,
    state: CoreLogState,
    disk_error: Option<String>,
    _poll_task: Task<()>,
    _list_observer: Subscription,
}

impl LogsView {
    pub(crate) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let list = cx.new(|cx| crate::keyboard::list_state(LogListDelegate::default(), window, cx));
        let disk = cx.new(|cx| disk_logs::DiskLogsView::new(core.clone(), window, cx));
        let details = cx.new(|cx| EditorState::new(window, cx).line_number(false));
        let observer = cx.observe_in(&list, window, |this, list, window, cx| {
            list.update(cx, |list, cx| {
                let selected = list.delegate().selected_index();
                if list.selected_index() != selected {
                    list.set_selected_index(selected, window, cx);
                }
            });
            let selected = list.read(cx).delegate().selected();
            let id = selected.as_ref().map(|(id, _)| *id);
            if this.detail_id != id {
                this.detail_id = id;
                this.details.update(cx, |details, cx| {
                    details.set_value(
                        selected.map(|(_, text)| text).unwrap_or_default(),
                        window,
                        cx,
                    );
                });
            }
            cx.notify();
        });
        let poll_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                if this
                    .update(cx, |this, cx| {
                        if this.active && this.auto_refresh {
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
            disk,
            disk_visible: false,
            list,
            details,
            detail_id: None,
            active: false,
            auto_refresh: true,
            min_rank: 2,
            limit: 500,
            revision: None,
            shown: 0,
            total: 0,
            evicted: 0,
            state: CoreLogState::Stopped,
            disk_error: None,
            _poll_task: poll_task,
            _list_observer: observer,
        }
    }
    pub(crate) fn busy(&self, cx: &gpui_kit::App) -> bool {
        self.disk.read(cx).busy()
    }

    pub(crate) fn set_closing(&mut self, closing: bool, cx: &mut Context<Self>) {
        self.disk.update(cx, |disk, cx| {
            disk.set_closing(closing, cx);
        });
    }

    pub(crate) fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.active = active;
        if active {
            self.refresh(cx);
        }
    }

    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        let disk_error = self.core.logs().disk_error();
        if self.disk_error != disk_error {
            self.disk_error = disk_error;
            cx.notify();
        }
        let Some(snapshot) =
            self.core
                .logs()
                .buffer()
                .snapshot(self.revision, self.min_rank, self.limit)
        else {
            return;
        };
        self.revision = Some(snapshot.revision);
        self.shown = snapshot.rows.len();
        self.total = snapshot.total;
        self.evicted = snapshot.evicted;
        self.state = snapshot.state;
        self.list.update(cx, |list, cx| {
            list.delegate_mut().replace(snapshot.rows);
            cx.notify();
        });
        cx.notify();
    }

    fn status(&self) -> String {
        let core = match &self.state {
            CoreLogState::Stopped => "核心日志：已停止".into(),
            CoreLogState::Connecting => "核心日志：连接中".into(),
            CoreLogState::Live => "核心日志：已连接".into(),
            CoreLogState::Disconnected(error) => format!("核心日志：已断开（{error}）；2 秒后重连"),
            CoreLogState::Unavailable(error) => format!("核心日志：不可用（{error}）"),
        };
        format!(
            "显示 {} 条 · 缓冲 {}/1000 · 已淘汰 {} 条 · {core}",
            self.shown, self.total, self.evicted
        )
    }
}
