//! 日志页磁盘分区：本应用数据根 logs/ 目录的真实文件列表与尾部原文
//! （共享 `lanpath-client` 安全读取：白名单文件名、拒绝链接/硬链接、
//! 2 MiB / 1000 行上限），以及经 NSH 原生保存对话框选择目标的导出。

use std::sync::Arc;

use lanpath_common::LanpathError;
use nsh_base::{button, theme};
use nsh_core::file_dialog::{FileFilter, SaveDialog};
use nsh_core::prelude::*;

use crate::log_service::LogService;

#[derive(Clone)]
struct DiskState {
    service: Arc<LogService>,
    files: Signal<Vec<String>>,
    selected: Signal<Option<String>>,
    tail: Signal<String>,
    error: Signal<Option<String>>,
    exported: Signal<Option<String>>,
    exporting: Signal<bool>,
}

impl DiskState {
    fn refresh(&self) {
        self.error.set(None);
        let Some(reader) = self.service.disk_reader() else {
            self.error
                .set(Some("磁盘日志不可用：写入器未初始化".to_owned()));
            return;
        };
        match reader.list_files() {
            Ok(names) => {
                let keep = self
                    .selected
                    .get()
                    .filter(|name| names.iter().any(|listed| listed == name));
                self.selected.set(keep.or_else(|| names.first().cloned()));
                self.files.set(names);
                self.read_tail();
            }
            Err(error) => self.error.set(Some(format!("读取日志目录失败:{error}"))),
        }
    }

    fn read_tail(&self) {
        self.error.set(None);
        let Some(name) = self.selected.get() else {
            self.tail.set(String::new());
            return;
        };
        match self
            .service
            .disk_reader()
            .map(|reader| reader.read_tail(&name, Some(1000)))
        {
            Some(Ok(content)) => self.tail.set(content),
            Some(Err(error)) => self
                .error
                .set(Some(format!("{error}；文件可能已删除，请刷新文件列表"))),
            None => self.error.set(Some("磁盘日志不可用".to_owned())),
        }
    }
}

pub(super) fn disk_logs(service: Arc<LogService>) -> impl IntoElement {
    let state = DiskState {
        service,
        files: signal(Vec::new()),
        selected: signal(None),
        tail: signal(String::new()),
        error: signal(None),
        exported: signal(None),
        exporting: signal(false),
    };
    state.refresh();
    // 导出任务句柄随 owner 存活；重复点击只替换未完成任务。
    let export_task: Stored<Option<Spawned<()>>> = stored(None);

    let refresh = state.clone();
    let export = state.clone();
    let error = state.clone();
    let status = state.clone();
    let empty = state.clone();
    let file_list = state.clone();
    let tail = state;
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
                .child(
                    text(move || {
                        format!(
                            "{} 个磁盘文件 · 当前：{}",
                            status.files.get().len(),
                            status.selected.get().unwrap_or_else(|| "未选择".to_owned())
                        )
                    })
                    .test_id("logs-disk-status")
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground),
                )
                .child(view().flex_1())
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
                .child(export_button(export, export_task)),
        )
        .child(
            text(move || error.error.get().unwrap_or_default())
                .text_size(px(12.0))
                .text_color(move || theme().destructive),
        )
        .child(when(
            move || empty.files.get().is_empty(),
            || text("暂无磁盘日志文件。").text_color(move || theme().muted_foreground),
        ))
        .child(
            view()
                .flex_1()
                .flex_row()
                .gap_2()
                .min_h(px(120.0))
                .child(view().w(px(200.0)).flex_none().child(scroll().child(
                    view().flex_col().gap_1().child(each(
                        file_list.files,
                        |name: &String| name.clone(),
                        move |name, _index| disk_file_row(file_list.clone(), name),
                    )),
                )))
                .child(
                    scroll().flex_1().min_w(px(0.0)).child(
                        view().p_2().child(
                            text(move || tail.tail.get().clone())
                                .test_id("disk-log-tail")
                                .selectable(true)
                                .text_size(px(12.0)),
                        ),
                    ),
                ),
        )
        .child(when(
            move || tail.exported.get().is_some(),
            move || {
                let exported = tail.clone();
                text(move || format!("已导出：{}", exported.exported.get().unwrap_or_default()))
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground)
            },
        ))
}

fn disk_file_row(state: DiskState, item: ReadSignal<String>) -> impl IntoElement {
    let select = state;
    let test_id = format!("disk-log-file-{}", item.get());
    button()
        .px_2()
        .py_1()
        .rounded(move || theme().radius)
        .items_center()
        .bg(move || {
            if select.selected.get().as_deref() == Some(item.get().as_str()) {
                theme().secondary
            } else {
                theme().background
            }
        })
        .on_press(move || {
            select.selected.set(Some(item.get().clone()));
            select.read_tail();
        })
        .child(
            text(move || item.get().clone())
                .test_id(&test_id)
                .text_size(px(12.0))
                .truncate()
                .min_w(px(0.0)),
        )
}

fn export_button(state: DiskState, task: Stored<Option<Spawned<()>>>) -> impl IntoElement {
    let label = state.clone();
    button()
        .px_3()
        .py_1()
        .rounded(move || theme().radius)
        .bg(move || theme().primary)
        .on_press(move || request_export(state.clone(), task))
        .child(text(move || {
            if label.exporting.get() {
                "导出中…"
            } else {
                "导出日志…"
            }
        }))
}

/// NSH 原生保存对话框选定目标后，把磁盘日志原文拼接到该路径；
/// 取消不做事，对话框无法显示与写出失败都以真实错误上屏。
fn request_export(state: DiskState, task: Stored<Option<Spawned<()>>>) {
    if state.exporting.get() {
        return;
    }
    state.exporting.set(true);
    state.error.set(None);
    task.with(|slot| {
        slot.replace(spawn(async move {
            let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
            let picked = pick_save_path(
                SaveDialog::new()
                    .file_name(format!("lanpath-desktop-logs-{stamp}.log"))
                    .title("导出磁盘日志")
                    .button_label("导出")
                    .filter(FileFilter::new("日志", ["log"])),
            )
            .await;
            let result = match picked {
                Err(error) => Some(Err(LanpathError::Config(format!(
                    "保存对话框无法显示:{error}"
                )))),
                Ok(None) => None,
                Ok(Some(path)) => Some(state.service.export_disk_logs(&path)),
            };
            state.exporting.set(false);
            match result {
                None => {}
                Some(Ok(path)) => {
                    state.exported.set(Some(path.display().to_string()));
                    state.error.set(None);
                }
                Some(Err(error)) => {
                    state.exported.set(None);
                    state.error.set(Some(format!("导出失败:{error}")));
                }
            }
        }));
    });
}
