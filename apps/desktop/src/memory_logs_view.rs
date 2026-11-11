//! 日志页内存分区：有界缓冲的真实展示，级别筛选、显示条数、暂停/刷新、
//! 详情查看与清空内存（确认）。采集在分区外持续进行，清空只影响缓冲。

use std::sync::Arc;
use std::time::Duration;

use nsh_base::{button, dialog, theme};
use nsh_core::prelude::*;

use crate::log_buffer::LOG_CAPACITY;
use crate::log_service::LogService;

/// 级别下限选项：与 `log_buffer::level_rank` 的秩一致，默认“信息”。
const LEVELS: [(u8, &str); 5] = [
    (0, "全部"),
    (1, "调试"),
    (2, "信息"),
    (3, "警告"),
    (4, "错误"),
];
const LIMITS: [usize; 3] = [200, 500, 1000];
const POLL_PERIOD: Duration = Duration::from_secs(2);

#[derive(Clone, PartialEq)]
struct MemoryRow {
    id: u64,
    line: String,
}

#[derive(Clone)]
struct MemoryState {
    service: Arc<LogService>,
    min_rank: Signal<u8>,
    limit: Signal<usize>,
    paused: Signal<bool>,
    rows: Signal<Vec<MemoryRow>>,
    revision: Signal<Option<u64>>,
    detail: Signal<Option<MemoryRow>>,
    confirm_clear: Signal<bool>,
    status: Signal<String>,
    disk_warning: Signal<Option<String>>,
}

impl MemoryState {
    /// 增量拉取缓冲快照；revision 未变时不触碰任何 signal。
    fn refresh(&self) {
        self.disk_warning.set(self.service.disk_error());
        let Some(snapshot) = self.service.buffer().snapshot(
            self.revision.get(),
            self.min_rank.get(),
            self.limit.get(),
        ) else {
            return;
        };
        self.revision.set(Some(snapshot.revision));
        self.rows.set(
            snapshot
                .rows
                .iter()
                .map(|row| MemoryRow {
                    id: row.id,
                    line: row.line.to_string(),
                })
                .collect(),
        );
        self.status.set(format!(
            "显示 {} 条 · 缓冲 {}/{} · 已淘汰 {} 条",
            snapshot.rows.len(),
            snapshot.total,
            LOG_CAPACITY,
            snapshot.evicted
        ));
    }
}

pub(super) fn memory_logs(service: Arc<LogService>) -> impl IntoElement {
    let state = MemoryState {
        service,
        min_rank: signal(2),
        limit: signal(200),
        paused: signal(false),
        rows: signal(Vec::new()),
        revision: signal(None),
        detail: signal(None),
        confirm_clear: signal(false),
        status: signal(String::new()),
        disk_warning: signal(None),
    };
    // 轮询句柄随 owner 存活：切页卸载即取消；暂停只停自动刷新，采集不停。
    let ticker: Stored<Option<Spawned<()>>> = stored(None);
    ticker.with(|slot| {
        let poll = state.clone();
        slot.replace(set_interval(POLL_PERIOD, move || {
            if !poll.paused.get() {
                poll.refresh();
            }
        }));
    });
    state.refresh();

    let toolbar = state.clone();
    let pause_label = state.clone();
    let refresh = state.clone();
    let warning = state.clone();
    let status = state.clone();
    let rows = state.clone();
    let empty = state.clone();
    let detail = state;
    view()
        .flex_1()
        .flex_col()
        .gap_2()
        .min_h(px(0.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .flex_wrap()
                .child(level_filter(toolbar.clone()))
                .child(limit_filter(toolbar.clone()))
                .child(view().flex_1())
                .child(
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .border(px(1.0))
                        .border_color(move || theme().border)
                        .on_press(move || {
                            let paused = !toolbar.paused.get();
                            toolbar.paused.set(paused);
                            if !paused {
                                toolbar.refresh();
                            }
                        })
                        .child(text(move || {
                            if pause_label.paused.get() {
                                "继续"
                            } else {
                                "暂停"
                            }
                        })),
                )
                .child(
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .border(px(1.0))
                        .border_color(move || theme().border)
                        .on_press(move || refresh.refresh())
                        .child(text("刷新")),
                )
                .child(
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .border(px(1.0))
                        .border_color(move || theme().destructive)
                        .on_press(move || warning.confirm_clear.set(true))
                        .child(text("清空内存日志").text_color(move || theme().destructive)),
                ),
        )
        .child(
            text(move || {
                warning
                    .disk_warning
                    .get()
                    .map(|error| format!("磁盘日志写入失败：{error}"))
                    .unwrap_or_default()
            })
            .text_size(px(12.0))
            .text_color(move || theme().destructive),
        )
        .child(
            text(move || status.status.get())
                .test_id("logs-memory-status")
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            scroll().flex_1().min_h(px(120.0)).child(
                view()
                    .flex_col()
                    .gap_1()
                    .child(when(
                        move || rows.rows.get().is_empty(),
                        || text("暂无日志。").text_color(move || theme().muted_foreground),
                    ))
                    .child(each(
                        rows.rows,
                        |row: &MemoryRow| row.id,
                        move |item, _index| memory_row(rows.clone(), item),
                    )),
            ),
        )
        .child(when(
            move || empty.detail.get().is_some(),
            move || {
                let panel = empty.clone();
                view()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded(move || theme().radius)
                    .border(px(1.0))
                    .border_color(move || theme().border)
                    .bg(move || theme().card)
                    .child(
                        text("日志详情")
                            .text_size(px(12.0))
                            .text_color(move || theme().muted_foreground),
                    )
                    .child(
                        text(move || panel.detail.get().map(|row| row.line).unwrap_or_default())
                            .selectable(true)
                            .text_size(px(12.0)),
                    )
            },
        ))
        .child(when(
            move || detail.confirm_clear.get(),
            move || clear_dialog(detail.clone()),
        ))
}

fn level_filter(state: MemoryState) -> impl IntoElement {
    view()
        .flex_row()
        .gap_1()
        .children(LEVELS.map(|(rank, label)| {
            let filter = state.clone();
            let selected = move || filter.min_rank.get() == rank;
            button()
                .px_2()
                .py_1()
                .rounded(move || theme().radius)
                .bg(move || {
                    if selected() {
                        theme().primary
                    } else {
                        theme().card
                    }
                })
                .on_press(move || {
                    filter.min_rank.set(rank);
                    filter.revision.set(None);
                    filter.refresh();
                })
                .child(text(label).text_size(px(12.0)).text_color(move || {
                    if selected() {
                        theme().primary_foreground
                    } else {
                        theme().foreground
                    }
                }))
        }))
}

fn limit_filter(state: MemoryState) -> impl IntoElement {
    view().flex_row().gap_1().children(LIMITS.map(|limit| {
        let filter = state.clone();
        let selected = move || filter.limit.get() == limit;
        button()
            .px_2()
            .py_1()
            .rounded(move || theme().radius)
            .bg(move || {
                if selected() {
                    theme().secondary
                } else {
                    theme().card
                }
            })
            .on_press(move || {
                filter.limit.set(limit);
                filter.revision.set(None);
                filter.refresh();
            })
            .child(
                text(format!("最近 {limit} 条"))
                    .text_size(px(12.0))
                    .text_color(move || theme().foreground),
            )
    }))
}

fn memory_row(state: MemoryState, item: ReadSignal<MemoryRow>) -> impl IntoElement {
    let select = state.clone();
    let highlight = state;
    let test_id = format!("log-row-{}", item.get().id);
    button()
        .px_2()
        .py_1()
        .rounded(move || theme().radius)
        .items_center()
        .bg(move || {
            if highlight
                .detail
                .get()
                .is_some_and(|selected| selected.id == item.get().id)
            {
                theme().secondary
            } else {
                theme().background
            }
        })
        .on_press(move || select.detail.set(Some(item.get())))
        .child(
            text(move || item.get().line.clone())
                .test_id(&test_id)
                .text_size(px(12.0))
                .truncate()
                .min_w(px(0.0)),
        )
}

fn clear_dialog(state: MemoryState) -> AnyElement {
    let on_cancel = state.clone();
    let cancel_button = state.clone();
    let confirm = state;
    dialog()
        .on_cancel(move || on_cancel.confirm_clear.set(false))
        .child(
            view()
                .flex_col()
                .gap_3()
                .w(px(360.0))
                .p_4()
                .rounded(move || theme().radius)
                .bg(move || theme().card)
                .border(px(1.0))
                .border_color(move || theme().border)
                .child(text("清空内存日志？"))
                .child(
                    text("仅清空本次进程的日志缓冲，不删除磁盘文件，不停止后续采集。")
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                )
                .child(
                    view()
                        .flex_row()
                        .gap_2()
                        .justify_end()
                        .child(
                            button()
                                .px_3()
                                .py_1()
                                .rounded(move || theme().radius)
                                .border(px(1.0))
                                .border_color(move || theme().border)
                                .on_press(move || cancel_button.confirm_clear.set(false))
                                .child(text("取消")),
                        )
                        .child(
                            button()
                                .px_3()
                                .py_1()
                                .rounded(move || theme().radius)
                                .bg(move || theme().destructive)
                                .on_press(move || {
                                    confirm.confirm_clear.set(false);
                                    confirm.service.buffer().clear();
                                    confirm.revision.set(None);
                                    confirm.detail.set(None);
                                    confirm.refresh();
                                })
                                .child(
                                    text("确认清空").text_color(move || theme().primary_foreground),
                                ),
                        ),
                ),
        )
        .into_element()
}
