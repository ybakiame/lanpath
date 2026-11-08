mod file_list;

use crate::runtime::DesktopRuntime;
use file_list::DiskFileList;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    input::{Editor, EditorState},
    list::{List, ListState},
};
use gpui_kit::{
    AppContext as _, ClipboardItem, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Role, StatefulInteractiveElement as _, Styled as _, Subscription,
    Task, Window, div,
};
use lanpath_client::logs::DiskLogService;
use std::{path::PathBuf, sync::Arc};

pub(super) struct DiskLogsView {
    service: Option<DiskLogService>,
    exports: Option<PathBuf>,
    files: Entity<ListState<DiskFileList>>,
    tail: Entity<EditorState>,
    names: Vec<String>,
    kind: usize,
    selected: Option<String>,
    error: Option<String>,
    busy: bool,
    reading: bool,
    closing: bool,
    exported: Option<PathBuf>,
    copied: bool,
    operation_task: Option<Task<()>>,
    tail_task: Option<Task<()>>,
    _selection_observer: Subscription,
}

impl DiskLogsView {
    pub(super) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let files = cx.new(|cx| crate::keyboard::list_state(DiskFileList::default(), window, cx));
        let tail = cx.new(|cx| EditorState::new(window, cx).line_number(false));
        let root = core
            .business()
            .map(|service| service.data_dir().to_path_buf());
        let (service, exports, error) = match root {
            Ok(root) => (
                Some(DiskLogService::new(root.join("logs")).with_core_log()),
                Some(root.join("exports")),
                None,
            ),
            Err(error) => (None, None, Some(error)),
        };
        let observer = cx.observe_in(&files, window, |this, files, window, cx| {
            files.update(cx, |files, cx| {
                let selected = files.delegate().selected_index();
                if files.selected_index() != selected {
                    files.set_selected_index(selected, window, cx);
                }
            });
            let selected = files.read(cx).delegate().selected();
            if this.selected != selected {
                this.selected = selected;
                this.read_tail(true, window, cx);
            }
            cx.notify();
        });
        Self {
            service,
            exports,
            files,
            tail,
            names: Vec::new(),
            kind: 0,
            selected: None,
            error,
            busy: false,
            reading: false,
            closing: false,
            exported: None,
            copied: false,
            operation_task: None,
            tail_task: None,
            _selection_observer: observer,
        }
    }

    pub(super) fn busy(&self) -> bool {
        self.busy || self.reading
    }

    pub(super) fn set_closing(&mut self, closing: bool, cx: &mut Context<Self>) {
        if self.closing != closing {
            self.closing = closing;
            cx.notify();
        }
    }

    pub(super) fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing || self.busy {
            return;
        }
        let Some(service) = self.service.clone() else {
            return;
        };
        self.busy = true;
        self.error = None;
        self.operation_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(
                    async move { service.list_files().map_err(|error| error.to_string()) },
                )
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.busy = false;
                    this.operation_task = None;
                    match result {
                        Ok(names) => {
                            this.names = names;
                            this.filter(window, cx);
                        }
                        Err(error) => {
                            this.error = Some(error);
                        }
                    }
                    cx.notify();
                })
                .ok();
            });
        }));
        cx.notify();
    }

    fn filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let names = self
            .names
            .iter()
            .filter(|name| match self.kind {
                1 => name.starts_with("app.log"),
                2 => name.as_str() == "core.log",
                3 => name.as_str() == "libbox.log",
                4 => name.as_str() == "last_start_config.json",
                _ => true,
            })
            .cloned()
            .collect();
        self.files.update(cx, |files, cx| {
            files.delegate_mut().replace(names);
            let selected = files.delegate().selected_index();
            files.set_selected_index(selected, window, cx);
            cx.notify();
        });
        let selected = self.files.read(cx).delegate().selected();
        let changed = self.selected != selected;
        self.selected = selected;
        self.read_tail(changed, window, cx);
    }

    fn read_tail(&mut self, clear: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing || self.service.is_none() {
            return;
        }
        self.error = None;
        if clear {
            self.tail
                .update(cx, |tail, cx| tail.set_value("", window, cx));
        }
        if self.reading {
            return;
        }
        let (Some(name), Some(service)) = (self.selected.clone(), self.service.clone()) else {
            return;
        };
        self.reading = true;
        self.tail_task = Some(cx.spawn_in(window, async move |this, cx| {
            let requested = name.clone();
            let result = cx
                .background_spawn(async move {
                    service
                        .read_tail(&name, Some(1000))
                        .map_err(|error| error.to_string())
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.reading = false;
                    this.tail_task = None;
                    if this.selected.as_ref() != Some(&requested) {
                        this.read_tail(false, window, cx);
                        cx.notify();
                        return;
                    }
                    match result {
                        Ok(text) => this
                            .tail
                            .update(cx, |tail, cx| tail.set_value(text, window, cx)),
                        Err(error) => {
                            this.error = Some(format!("{error}；文件可能已删除，请刷新文件列表"))
                        }
                    }
                    cx.notify();
                })
                .ok();
            });
        }));
    }

    fn export(&mut self, cx: &mut Context<Self>) {
        if self.closing || self.busy {
            return;
        }
        let (Some(service), Some(exports)) = (self.service.clone(), self.exports.clone()) else {
            return;
        };
        self.busy = true;
        self.error = None;
        self.copied = false;
        self.operation_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    service
                        .export_to(&exports)
                        .map_err(|error| error.to_string())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.operation_task = None;
                match result {
                    Ok(path) => this.exported = Some(path),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

impl Render for DiskLogsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = if self.busy {
            "正在读取或导出日志…".to_string()
        } else if self.reading {
            "正在读取文件尾部…".to_string()
        } else {
            format!(
                "{} 个磁盘文件 · 当前：{}",
                self.names.len(),
                self.selected.as_deref().unwrap_or("未选择")
            )
        };
        div().flex().flex_col().flex_1().min_h_0().min_w_0().gap_2()
            .child(crate::design::surface(cx).p_3().flex().items_center().flex_wrap().gap_2()
                .child(ButtonGroup::new("disk-log-kinds").small().compact().outline()
                    .children(["全部", "应用", "核心", "libbox", "启动配置"].into_iter().enumerate().map(|(kind,label)| {
                        Button::new(("disk-logs-kind",kind)).label(label).selected(self.kind == kind)
                    })).on_click(cx.listener(|this,indices: &Vec<usize>,window,cx| {
                        if let Some(kind) = indices.first() { this.kind = *kind; this.filter(window,cx); cx.notify(); }
                    })))
                .child(div().flex_1())
                .child(Button::new("disk-logs-refresh").ghost().icon(gpui_kit::assets::IconName::RefreshCw)
                    .accessibility_label("刷新磁盘日志").tooltip("刷新磁盘日志")
                    .disabled(self.closing || self.busy || self.service.is_none())
                    .on_click(cx.listener(|this,_,window,cx| this.refresh(window,cx)))))
            .children(self.error.as_ref().map(|error| div().id("disk-logs-error").role(Role::Status)
                .aria_label(error.clone()).text_sm().text_color(cx.theme().danger).child(error.clone())))
            .child(div().flex().items_stretch().flex_1().min_h_0().min_w_0().gap_2()
                .child(div().w(gpui_kit::relative(0.28)).min_w(gpui_kit::rems(7.)).flex_shrink_0().flex().flex_col().child(List::new(&self.files).flex_1().min_h_0()))
                .child(div().id("disk-log-tail").role(Role::Group).aria_label("磁盘日志尾部，只读，可选择和复制")
                    .flex().flex_col().flex_1().min_w_0().min_h_0()
                    .child(Editor::new(&self.tail).readonly(true).size_full())))
            .children(self.exported.as_ref().map(|path| {
                let path = path.to_string_lossy().into_owned();
                div().flex().flex_col().gap_1().min_w_0()
                    .child(div().text_sm().child(format!("已导出：{path}")))
                    .child(Button::new("disk-logs-copy-path").label(if self.copied { "已复制导出路径" } else { "复制导出路径" })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                            this.copied = true;
                            cx.notify();
                        })))
            }))
            .child(crate::design::toolbar()
                .child(div().id("disk-logs-status").accessibility_id("disk-logs-status").role(Role::Status).aria_label(status.clone())
                    .flex_1().text_xs().text_color(cx.theme().muted_foreground).child(status))
                .child(Button::new("disk-logs-export").small().label("导出磁盘日志")
                    .disabled(self.closing || self.busy || self.service.is_none() || self.names.is_empty())
                    .on_click(cx.listener(|this,_,_,cx| this.export(cx)))))
            .child(crate::design::help("disk-logs-help","文件与导出说明",div().text_sm().child(
                "仅读取实验目录的日志尾部，最多 1000 行 / 2 MiB。导出包含各类型日志原文（不含启动配置），未加密，写入 exports/；不会删除文件。")))
    }
}
