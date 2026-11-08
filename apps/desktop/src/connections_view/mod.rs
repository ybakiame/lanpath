mod render;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use gpui_kit::component::{input::EditorState, list::ListState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Task, Window};
use lanpath_client::connections::{
    ActiveConnections, clash_close_connection, clash_get_connections,
};

use crate::{connection_list::ConnectionListDelegate, runtime::DesktopRuntime};

pub(crate) struct ConnectionsView {
    core: Arc<DesktopRuntime>,
    list: Entity<ListState<ConnectionListDelegate>>,
    details: Entity<EditorState>,
    detail_text: String,
    upload_total: u64,
    download_total: u64,
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
        let list =
            cx.new(|cx| crate::keyboard::list_state(ConnectionListDelegate::default(), window, cx));
        let details = cx.new(|cx| EditorState::new(window, cx).line_number(false));
        let observer = cx.observe_in(&list, window, |this, list, window, cx| {
            list.update(cx, |list, cx| {
                let selected = list.delegate().selected_index();
                if list.selected_index() != selected {
                    list.set_selected_index(selected, window, cx);
                }
            });
            let text = list
                .read(cx)
                .delegate()
                .selected()
                .map(|row| {
                    let age = (chrono::Local::now().timestamp().max(0) as u64)
                        .saturating_sub(row.start);
                    format!(
                        "目标：{}\n目标 IP：{}\n协议：{}\n出站链：{}\n命中规则：{} {}\n上传：{} B · 下载：{} B\n开始：{} · {}：{} 秒\n连接 ID：{}",
                        row.host, row.destination_ip, row.network, row.chain, row.rule,
                        row.rule_payload, row.upload, row.download, row.start,
                        if this.show_closed { "距开始" } else { "持续" }, age, row.id
                    )
                })
                .unwrap_or_default();
            if text != this.detail_text {
                this.details.update(cx, |editor, cx| editor.set_value(text.clone(), window, cx));
                this.detail_text = text;
            }
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
            details,
            detail_text: String::new(),
            upload_total: 0,
            download_total: 0,
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

    pub(crate) fn busy(&self) -> bool {
        self.busy
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
        self.upload_total = 0;
        self.download_total = 0;
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
        self.upload_total = 0;
        self.download_total = 0;
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
                        let endpoint = core.business().and_then(|business| {
                            business
                                .clash_api_endpoint()
                                .map_err(|error| error.to_string())
                        });
                        let (port, secret) = match endpoint {
                            Ok(endpoint) => endpoint,
                            Err(error) => return (None, Err(error)),
                        };
                        let close_result = if let Some(id) = close_id {
                            Some(
                                clash_close_connection(port, &secret, &id)
                                    .await
                                    .map_err(|e| e.to_string()),
                            )
                        } else {
                            None
                        };
                        // Retain the HTTP health/error contract; tracker.closed()
                        // alone cannot distinguish a stale snapshot from a live one.
                        let result = match clash_get_connections(port, &secret).await {
                            Ok(_) => match core.observations() {
                                Ok(observations) if show_closed => {
                                    let connections = observations.closed().await;
                                    let upload_total =
                                        connections.iter().map(|row| row.upload).sum();
                                    let download_total =
                                        connections.iter().map(|row| row.download).sum();
                                    Ok(ActiveConnections {
                                        connections,
                                        upload_total,
                                        download_total,
                                    })
                                }
                                Ok(observations) => observations.active().await,
                                Err(error) => Err(error),
                            },
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

    fn apply_result(&mut self, result: Result<ActiveConnections, String>, cx: &mut Context<Self>) {
        match result {
            Ok(snapshot) => {
                self.upload_total = snapshot.upload_total;
                self.download_total = snapshot.download_total;
                self.loaded = true;
                self.load_error = None;
                self.list.update(cx, |list, cx| {
                    if list.delegate_mut().replace(snapshot.connections) {
                        cx.notify();
                    }
                });
            }
            Err(error) => self.load_error = Some(error),
        }
    }
}
