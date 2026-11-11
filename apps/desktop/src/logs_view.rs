//! 日志页外壳：内存/磁盘两个真实分区的切换与数据根打开。全部数据来自
//! NSH 桌面自己的数据根；不读取其他应用的日志，不启动核心或网络请求。

use std::sync::Arc;

use nsh_base::{button, theme};
use nsh_core::prelude::*;

use crate::disk_logs_view::disk_logs;
use crate::log_service::LogService;
use crate::memory_logs_view::memory_logs;

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
    let error_text = move || state.open_error.get().unwrap_or_default();
    let body = match state.service.clone() {
        Some(service) => logs_body(service, section).into_element(),
        None => view().into_element(),
    };
    view()
        .flex_1()
        .flex_col()
        .gap_4()
        .min_h(px(0.0))
        .child(text("日志").text_size(px(24.0)))
        .child(
            text("本应用运行事件 · 内存缓冲与磁盘原文，仅限 NSH 桌面数据根")
                .text_color(move || theme().muted_foreground),
        )
        .child(
            view()
                .flex_row()
                .gap_2()
                .child(section_tab(section, LogSection::Memory, "内存日志"))
                .child(section_tab(section, LogSection::Disk, "磁盘日志")),
        )
        .child(text(error_text).text_color(move || theme().destructive))
        .child(body)
}

fn logs_body(service: Arc<LogService>, section: Signal<LogSection>) -> impl IntoElement {
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
            move || disk_logs(disk_service.clone()),
        ))
}

fn section_tab(
    section: Signal<LogSection>,
    target: LogSection,
    label: &'static str,
) -> impl IntoElement {
    let selected = move || section.get() == target;
    button()
        .px_3()
        .py_2()
        .rounded(move || theme().radius)
        .items_center()
        .bg(move || {
            if selected() {
                theme().primary
            } else {
                theme().card
            }
        })
        .on_press(move || section.set(target))
        .child(text(label).text_color(move || {
            if selected() {
                theme().primary_foreground
            } else {
                theme().foreground
            }
        }))
}
