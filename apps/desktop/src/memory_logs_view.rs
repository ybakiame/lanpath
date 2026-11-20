//! 日志页内存分区：有界缓冲的真实展示，级别筛选、显示条数、暂停/刷新、
//! 详情查看与清空内存（确认）。采集在分区外持续进行，清空只影响缓冲。

use std::sync::Arc;
use std::time::Duration;

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::log_buffer::LOG_CAPACITY;
use crate::log_service::LogService;
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::layout::{AlertTone, SurfaceLevel, alert_banner, surface};

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
    let warning_banner = state.clone();
    let status = state.clone();
    let rows = state.clone();
    let empty = state.clone();
    let detail = state;
    view()
        .flex_1()
        .flex_col()
        .gap_3()
        .min_h(px(0.0))
        .child(
            surface(SurfaceLevel::Card)
                .p_3()
                .flex_col()
                .gap_2()
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
                            crate::ui::button(crate::ui::ButtonVariant::Outline)
                                .size(crate::ui::ButtonSize::Sm)
                                .on_press(move || {
                                    let paused = !toolbar.paused.get();
                                    toolbar.paused.set(paused);
                                    if !paused {
                                        toolbar.refresh();
                                    }
                                })
                                .label(move || {
                                    if pause_label.paused.get() {
                                        "继续".to_owned()
                                    } else {
                                        "暂停".to_owned()
                                    }
                                }),
                        )
                        .child(
                            crate::ui::button(crate::ui::ButtonVariant::Outline)
                                .size(crate::ui::ButtonSize::Sm)
                                .prefix(|| icon_sized(Icon::Refresh, Tone::Muted, 13.0))
                                .on_press(move || refresh.refresh())
                                .label(|| t!("product.action.refresh")),
                        )
                        .child(
                            crate::ui::button(crate::ui::ButtonVariant::Outline)
                                .size(crate::ui::ButtonSize::Sm)
                                .on_press(move || warning.confirm_clear.set(true))
                                .label(|| t!("product.action.clear_logs")),
                        ),
                )
                .child(
                    text(move || status.status.get())
                        .test_id("logs-memory-status")
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .child(when(
            move || warning_banner.disk_warning.get().is_some(),
            move || {
                let warn = warning_banner.disk_warning;
                alert_banner(
                    AlertTone::Danger,
                    text(move || {
                        warn.get()
                            .map(|error| format!("磁盘日志写入失败：{error}"))
                            .unwrap_or_default()
                    })
                    .text_size(px(12.5))
                    .text_color(move || theme().destructive),
                )
            },
        ))
        .child(
            surface(SurfaceLevel::Card)
                .flex_1()
                .min_h(px(140.0))
                .p_2()
                .child(
                    scroll().flex_1().child(
                        view()
                            .flex_col()
                            .gap_1()
                            .child(when(
                                move || rows.rows.get().is_empty(),
                                || {
                                    crate::ui::empty_with_icon(
                                        Some(Icon::Logs),
                                        || t!("nav.logs"),
                                        || t!("product.logs.empty"),
                                    )
                                },
                            ))
                            .child(each(
                                rows.rows,
                                |row: &MemoryRow| row.id,
                                move |item, _index| memory_row(rows.clone(), item),
                            )),
                    ),
                ),
        )
        .child(when(
            move || empty.detail.get().is_some(),
            move || {
                let panel = empty.clone();
                let close = empty.clone();
                surface(SurfaceLevel::Card)
                    .p_3()
                    .flex_col()
                    .gap_2()
                    .child(
                        view()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                text(|| t!("product.logs.detail"))
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(move || theme().muted_foreground),
                            )
                            .child(
                                crate::ui::button(crate::ui::ButtonVariant::Ghost)
                                    .size(crate::ui::ButtonSize::Sm)
                                    .prefix(|| icon_sized(Icon::Close, Tone::Muted, 13.0))
                                    .label(|| t!("common.close"))
                                    .on_press(move || close.detail.set(None)),
                            ),
                    )
                    .child(
                        text(move || panel.detail.get().map(|row| row.line).unwrap_or_default())
                            .selectable(true)
                            .text_size(px(12.5)),
                    )
            },
        ))
        .child(clear_dialog(detail))
}

fn level_filter(state: MemoryState) -> impl IntoElement {
    view()
        .flex_row()
        .gap_1()
        .children(LEVELS.map(|(rank, label)| {
            let filter = state.clone();
            let selected = move || filter.min_rank.get() == rank;
            crate::ui::button(crate::ui::ButtonVariant::Ghost)
                .selected(selected)
                .size(crate::ui::ButtonSize::Sm)
                .on_press(move || {
                    filter.min_rank.set(rank);
                    filter.revision.set(None);
                    filter.refresh();
                })
                .label(move || label.to_owned())
        }))
}

fn limit_filter(state: MemoryState) -> impl IntoElement {
    view().flex_row().gap_1().children(LIMITS.map(|limit| {
        let filter = state.clone();
        let selected = move || filter.limit.get() == limit;
        crate::ui::button(crate::ui::ButtonVariant::Ghost)
            .selected(selected)
            .size(crate::ui::ButtonSize::Sm)
            .on_press(move || {
                filter.limit.set(limit);
                filter.revision.set(None);
                filter.refresh();
            })
            .label(move || format!("最近 {limit} 条"))
    }))
}

fn memory_row(state: MemoryState, item: ReadSignal<MemoryRow>) -> impl IntoElement {
    let select = state.clone();
    let highlight = state;
    let test_id = format!("log-row-{}", item.get().id);
    crate::ui::button(crate::ui::ButtonVariant::Ghost)
        .full_width()
        .selected(move || {
            highlight
                .detail
                .get()
                .is_some_and(|selected| selected.id == item.get().id)
        })
        .size(crate::ui::ButtonSize::Sm)
        .on_press(move || select.detail.set(Some(item.get())))
        .test_id(&test_id)
        .label(move || item.get().line.clone())
}

fn clear_dialog(state: MemoryState) -> AnyElement {
    let open = state.confirm_clear;
    let cancel = state.clone();
    let confirm = state;
    crate::ui::confirm_dialog(move || open.get())
        .destructive()
        .title(|| t!("product.logs.clear_title"))
        .description(|| t!("product.logs.clear_hint"))
        .cancel_label(|| t!("product.action.cancel"))
        .confirm_label(|| t!("product.action.confirm_clear"))
        .on_cancel(move || cancel.confirm_clear.set(false))
        .on_confirm(move || {
            confirm.confirm_clear.set(false);
            confirm.service.buffer().clear();
            confirm.revision.set(None);
            confirm.detail.set(None);
            confirm.refresh();
        })
        .into_element()
}
