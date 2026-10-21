#[path = "disk_logs/mod.rs"]
mod disk_logs;

use std::{sync::Arc, time::Duration};

use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, WindowExt as _,
    button::Button,
    input::{Editor, EditorState},
    list::{List, ListState},
    switch::Switch,
};
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Role, StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, div,
};

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
        let list = cx.new(|cx| ListState::new(LogListDelegate::default(), window, cx));
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
            disk.set_closing(closing);
            cx.notify();
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

impl LogsView {
    fn render_memory(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let levels = [
            (0, "全部级别"),
            (1, "DEBUG+"),
            (2, "INFO+"),
            (3, "WARN+"),
            (4, "ERROR"),
        ];
        let status = self.status();
        let error = matches!(
            self.state,
            CoreLogState::Disconnected(_) | CoreLogState::Unavailable(_)
        );
        div().flex().flex_col().flex_1().min_h_0().gap_2()
            .child(div().flex().items_center().gap_2().child("运行日志")
                .child(Button::new("logs-refresh").label("刷新日志").on_click(cx.listener(|this, _, _, cx| this.refresh(cx))))
                .child(Switch::new("logs-auto-refresh").label("自动刷新").checked(self.auto_refresh)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.auto_refresh = *checked;
                        if *checked { this.refresh(cx); }
                        cx.notify();
                    })))
                .child(Button::new("logs-clear").label("清空内存缓冲…").disabled(self.total == 0)
                    .on_click(cx.listener(|_, _, window, cx| {
                        let view = cx.weak_entity();
                        window.open_alert_dialog(cx, move |dialog, _, _| {
                            let view = view.clone();
                            dialog.title("清空实验应用的内存日志缓冲？")
                                .description("仅移除本次进程已采集的日志；不删除磁盘文件，不影响核心运行或后续采集。")
                                .confirm().ok_text("清空内存缓冲").cancel_text("取消")
                                .on_ok(move |_, _, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        this.core.logs().buffer().clear();
                                        this.refresh(cx);
                                    });
                                    true
                                })
                        });
                    }))))
            .child(div().flex().items_center().gap_2().children(levels.into_iter().map(|(rank, label)| {
                Button::new(("logs-level", rank as usize)).label(label).selected(self.min_rank == rank)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.min_rank = rank;
                        this.revision = None;
                        this.refresh(cx);
                    }))
            })))
            .child(div().flex().items_center().gap_2().child("最新条数")
                .children([200, 500, 1000].into_iter().map(|limit| {
                    Button::new(("logs-limit", limit)).label(limit.to_string()).selected(self.limit == limit)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.limit = limit;
                            this.revision = None;
                            this.refresh(cx);
                        }))
                })))
            .child(div().id("logs-status").role(Role::Status).aria_label(status.clone()).text_sm()
                .text_color(if error { cx.theme().danger } else { cx.theme().muted_foreground }).child(status))
            .child(List::new(&self.list).flex_1().min_h_0())
            .children(self.detail_id.map(|_| {
                div().id("log-detail").role(Role::Group).aria_label("日志详情")
                    .flex().flex_col().gap_1().h_40().flex_shrink_0()
                    .child(div().text_sm().child("已选日志详情（只读，可选择和复制）"))
                    .child(Editor::new(&self.details).readonly(true).size_full())
            }))
            .child(div().text_sm().text_color(cx.theme().muted_foreground)
                .child("最新在前，最多保留 1000 条；单条消息最多 8 KiB。暂停刷新或离开页面不停止采集。"))
            .child(div().text_sm().text_color(cx.theme().muted_foreground)
                .child("内存缓冲仅限本次进程；核心实时订阅在订阅前或断流期间不回放。清空仅影响内存，磁盘日志在另一个视图查看。"))
    }
}

impl Render for LogsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = if self.disk_visible {
            self.disk.clone().into_any_element()
        } else {
            self.render_memory(cx).into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("logs-memory-view")
                            .label("内存日志")
                            .selected(!self.disk_visible)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.disk_visible = false;
                                this.refresh(cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("logs-disk-view")
                            .label("磁盘日志")
                            .selected(self.disk_visible)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.disk_visible = true;
                                this.disk.update(cx, |disk, cx| disk.refresh(window, cx));
                                cx.notify();
                            })),
                    ),
            )
            .children(self.disk_error.as_ref().map(|error| {
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(format!("磁盘日志写入失败：{error}"))
            }))
            .child(content)
    }
}
