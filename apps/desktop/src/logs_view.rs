//! 日志页外壳：内存/磁盘两个真实分区的切换与数据根打开。全部数据来自
//! NSH 桌面自己的数据根；不读取其他应用的日志，不启动核心或网络请求。

use std::sync::Arc;

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::disk_logs_view::disk_logs;
use crate::log_service::LogService;
use crate::memory_logs_view::memory_logs;
use crate::ui::layout::{AlertTone, alert_banner};
use crate::ui::select::{segmented_group, segmented_radio};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LogSection {
    Memory,
    Disk,
}

#[derive(Clone)]
struct LogsState {
    service: Option<Arc<LogService>>,
    open_error: Signal<Option<String>>,
}

impl LogsState {
    fn new() -> Self {
        match LogService::open() {
            Ok(service) => Self {
                service: Some(service),
                open_error: signal(None),
            },
            Err(error) => Self {
                service: None,
                open_error: signal(Some(format!("打开数据根失败:{error}"))),
            },
        }
    }
}

pub(super) fn logs_page() -> impl IntoElement {
    let state = LogsState::new();
    let section = signal(LogSection::Memory);
    let io_busy = signal(false);
    crate::navigation::protect(move || {
        if io_busy.get() {
            crate::navigation::LeaveState::Busy
        } else {
            crate::navigation::LeaveState::Clean
        }
    });
    let open_error = state.open_error;
    let body = match state.service.clone() {
        Some(service) => logs_body(service, section, io_busy).into_element(),
        None => view().into_element(),
    };
    view()
        .flex_1()
        .flex_col()
        .gap_4()
        .min_h(px(0.0))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_end()
                .justify_between()
                .gap_3()
                .child(crate::ui::page_header(
                    || t!("nav.logs"),
                    || t!("logs.subtitle"),
                ))
                .child(segmented_group(
                    "logs-section-group",
                    vec![
                        section_tab(
                            section,
                            LogSection::Memory,
                            io_busy,
                            "logs-tab-memory",
                            "内存日志",
                        )
                        .into_element(),
                        section_tab(
                            section,
                            LogSection::Disk,
                            io_busy,
                            "logs-tab-disk",
                            "磁盘日志",
                        )
                        .into_element(),
                    ],
                )),
        )
        .child(when(
            move || open_error.get().is_some(),
            move || {
                alert_banner(
                    AlertTone::Danger,
                    text(move || open_error.get().unwrap_or_default())
                        .text_size(px(12.5))
                        .text_color(move || theme().destructive),
                )
            },
        ))
        .child(body)
}

fn logs_body(
    service: Arc<LogService>,
    section: Signal<LogSection>,
    io_busy: Signal<bool>,
) -> impl IntoElement {
    let memory_service = service.clone();
    let disk_service = service;
    view()
        .flex_1()
        .flex_col()
        .gap_3()
        .min_h(px(0.0))
        .child(when(
            move || section.get() == LogSection::Memory,
            move || memory_logs(memory_service.clone()),
        ))
        .child(when(
            move || section.get() == LogSection::Disk,
            move || disk_logs(disk_service.clone(), io_busy),
        ))
}

fn section_tab(
    section: Signal<LogSection>,
    target: LogSection,
    io_busy: Signal<bool>,
    test_id: &'static str,
    label: &'static str,
) -> impl IntoElement {
    segmented_radio(
        move || section.get() == target,
        move || label.to_owned(),
        test_id,
        move || {
            if !io_busy.get() {
                section.set(target);
            }
        },
    )
}
