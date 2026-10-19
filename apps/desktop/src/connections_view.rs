use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _,
    button::Button,
    list::{List, ListState},
};
use gpui_kit::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, Task, Window, div,
};
use lanpath_client::connections::{ConnectionView, clash_close_connection, clash_get_connections};

use crate::{connection_list::ConnectionListDelegate, runtime::DesktopRuntime};

pub(crate) struct ConnectionsView {
    core: Arc<DesktopRuntime>,
    list: Entity<ListState<ConnectionListDelegate>>,
    running: bool,
    active: bool,
    busy: bool,
    closing: bool,
    show_closed: bool,
    loaded: bool,
    generation: u64,
    next_poll: Instant,
    load_error: Option<String>,
    action_error: Option<String>,
    notice: Option<&'static str>,
    request_task: Option<Task<()>>,
    _poll_task: Task<()>,
    _list_observer: Subscription,
}

impl ConnectionsView {
    pub(crate) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let list = cx.new(|cx| ListState::new(ConnectionListDelegate::default(), window, cx));
        let observer = cx.observe_in(&list, window, |_, list, window, cx| {
            list.update(cx, |list, cx| {
                let selected = list.delegate().selected_index();
                if list.selected_index() != selected {
                    list.set_selected_index(selected, window, cx);
                }
            });
            cx.notify();
        });
        let poll_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                if this
                    .update(cx, |this, cx| {
                        if Instant::now() >= this.next_poll {
                            this.request(None, cx);
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
            running: false,
            active: false,
            busy: false,
            closing: false,
            show_closed: false,
            loaded: false,
            generation: 0,
            next_poll: Instant::now(),
            load_error: None,
            action_error: None,
            notice: None,
            request_task: None,
            _poll_task: poll_task,
            _list_observer: observer,
        }
    }

    pub(crate) fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.active = active;
        if active {
            self.request(None, cx);
        }
        cx.notify();
    }

    pub(crate) fn set_running(&mut self, running: bool, cx: &mut Context<Self>) {
        self.generation += 1;
        self.running = running;
        self.loaded = false;
        self.load_error = None;
        self.action_error = None;
        self.notice = None;
        self.list.update(cx, |list, cx| {
            list.delegate_mut().replace(Vec::new());
            cx.notify();
        });
        // In-flight work retains ownership until completion; a new session cannot
        // accept its result or start a second request on top of it.
        if running {
            self.request(None, cx);
        }
        cx.notify();
    }

    fn select_kind(&mut self, show_closed: bool, cx: &mut Context<Self>) {
        if self.show_closed == show_closed {
            return;
        }
        self.show_closed = show_closed;
        self.generation += 1;
        self.loaded = false;
        self.load_error = None;
        self.action_error = None;
        self.notice = None;
        self.list.update(cx, |list, cx| {
            list.delegate_mut().replace(Vec::new());
            cx.notify();
        });
        self.next_poll = Instant::now();
        self.request(None, cx);
        cx.notify();
    }

    fn request(&mut self, close_id: Option<String>, cx: &mut Context<Self>) {
        if !self.running || !self.active || self.busy || (self.show_closed && close_id.is_some()) {
            return;
        }
        self.busy = true;
        self.closing = close_id.is_some();
        if self.closing {
            self.action_error = None;
            self.notice = None;
        }
        let generation = self.generation;
        let show_closed = self.show_closed;
        let core = self.core.clone();
        self.request_task = Some(cx.spawn(async move |this, cx| {
            let closing = close_id.is_some();
            let (close_result, result) = cx
                .background_spawn(async move {
                    core.block_on(async {
                        let close_result = if let Some(id) = close_id {
                            Some(
                                clash_close_connection(core.api_port(), "", &id)
                                    .await
                                    .map_err(|e| e.to_string()),
                            )
                        } else {
                            None
                        };
                        // Retain the HTTP health/error contract; tracker.closed()
                        // alone cannot distinguish a stale snapshot from a live one.
                        let result = match clash_get_connections(core.api_port(), "").await {
                            Ok(_) if show_closed => match core.observations() {
                                Ok(observations) => Ok(observations.closed().await),
                                Err(error) => Err(error),
                            },
                            Ok(rows) => Ok(rows),
                            Err(error) => Err(error.to_string()),
                        };
                        (close_result, result)
                    })
                    .unwrap_or_else(|error| (None, Err(error)))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.closing = false;
                if generation != this.generation || !this.running {
                    cx.notify();
                    return;
                }
                if let Some(close_result) = close_result {
                    match close_result {
                        Ok(()) => this.notice = Some("连接已关闭"),
                        Err(error) => this.action_error = Some(error),
                    }
                }
                this.next_poll = Instant::now()
                    + if result.is_err() {
                        Duration::from_secs(10)
                    } else if closing {
                        Duration::from_millis(1500)
                    } else {
                        Duration::ZERO
                    };
                this.apply_result(result, cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn apply_result(
        &mut self,
        result: Result<Vec<ConnectionView>, String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(rows) => {
                self.loaded = true;
                self.load_error = None;
                self.list.update(cx, |list, cx| {
                    if list.delegate_mut().replace(rows) {
                        cx.notify();
                    }
                });
            }
            Err(error) => self.load_error = Some(error),
        }
    }
}

impl Render for ConnectionsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_selection = self.list.read(cx).delegate().selected_id().is_some();
        let count = self.list.read(cx).delegate().len();
        let status = if !self.running {
            "启动隔离核心后可用".to_string()
        } else if self.closing {
            "正在关闭所选连接…".to_string()
        } else if let Some(error) = &self.load_error {
            format!("读取失败，保留最后成功快照；10 秒后重试：{error}")
        } else if !self.loaded {
            "正在读取活动连接…".to_string()
        } else {
            if self.show_closed {
                format!("{count} 条当前会话关闭记录 · 最多 500 条 · 每 2 秒刷新")
            } else {
                format!("{count} 个活动连接 · 每 2 秒轮询 · 切页暂停显示刷新")
            }
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child("连接")
                    .child(
                        Button::new("connections-refresh")
                            .label("刷新连接")
                            .disabled(!self.running || self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.request(None, cx))),
                    )
                    .child(
                        Button::new("connection-close")
                            .label("关闭所选连接")
                            .disabled(
                                self.show_closed || !self.running || self.busy || !has_selection,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                let id = this
                                    .list
                                    .read(cx)
                                    .delegate()
                                    .selected_id()
                                    .map(str::to_owned);
                                if let Some(id) = id {
                                    this.request(Some(id), cx);
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("connections-active")
                            .label("活动连接")
                            .selected(!self.show_closed)
                            .on_click(cx.listener(|this, _, _, cx| this.select_kind(false, cx))),
                    )
                    .child(
                        Button::new("connections-closed")
                            .label("会话已关闭")
                            .selected(self.show_closed)
                            .on_click(cx.listener(|this, _, _, cx| this.select_kind(true, cx))),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(status),
            )
            .children(self.action_error.as_ref().map(|error| {
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(format!("关闭失败：{error}"))
            }))
            .children(self.notice.map(|notice| div().text_sm().child(notice)))
            .child(List::new(&self.list).size_full())
    }
}
