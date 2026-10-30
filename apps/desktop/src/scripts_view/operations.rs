use gpui_kit::{AppContext as _, Context, Window, component::WindowExt as _};

use crate::script_dialects::validate_selection;

use super::{
    Operation, ScriptsView,
    editing::{dialect_label, kind_label},
    set_input,
};

impl ScriptsView {
    pub(super) fn save_resource(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() || !self.resource_dirty {
            return;
        }
        let resource = match self.form_resource(cx) {
            Ok(resource) => resource,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        let Some(service) = self.service.clone() else {
            return;
        };
        let original = self.selected.clone();
        let core = self.core.clone();
        self.operation = Some(Operation::Save);
        self.error = None;
        self.notice = None;
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.block_on(async {
                        match original.as_deref() {
                            Some(name) => service.update_remote_resource(name, resource).await,
                            None => service.add_remote_resource(resource).await,
                        }
                    })?
                    .map_err(|error| error.to_string())
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.operation = None;
                    match result {
                        Ok(resource) => {
                            this.selected = Some(resource.name.clone());
                            this.creating = false;
                            this.reload_resources(window, cx);
                            this.notice =
                                Some("远程资源已保存；更新缓存后，下次启动会重建定时任务".into());
                        }
                        Err(error) => this.error = Some(error),
                    }
                    cx.notify();
                })
                .ok();
            });
        }));
        cx.notify();
    }

    pub(super) fn request_delete_resource(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.selected.clone() else {
            return;
        };
        if self.locked || self.busy() || self.resource_dirty {
            return;
        }
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            let delete_name = name.clone();
            dialog
                .title(format!("删除“{name}”？"))
                .description("资源清单会删除；既有脚本和片段缓存保留，避免破坏运行中的任务。")
                .confirm()
                .ok_text("删除")
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| {
                        view.delete_resource(delete_name.clone(), window, cx)
                    });
                    true
                })
        });
    }

    fn delete_resource(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        // The confirmation may outlive a ticker acceptance or a root lock.
        if self.locked || self.busy() || self.resource_dirty {
            return;
        }
        let (Some(service), core) = (self.service.clone(), self.core.clone()) else {
            return;
        };
        self.operation = Some(Operation::Delete);
        self.error = None;
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.block_on(async { service.remove_remote_resource(&name).await })?
                        .map_err(|error| error.to_string())
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.operation = None;
                    match result {
                        Ok(()) => {
                            this.selected = None;
                            this.reload_resources(window, cx);
                            this.notice = Some("远程资源已删除；既有缓存已保留".into());
                        }
                        Err(error) => this.error = Some(error),
                    }
                    cx.notify();
                })
                .ok();
            });
        }));
        cx.notify();
    }

    pub(super) fn toggle_resource(
        &mut self,
        name: String,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.locked || self.busy() || self.resource_dirty {
            return;
        }
        let (Some(service), core) = (self.service.clone(), self.core.clone()) else {
            return;
        };
        self.operation = Some(Operation::Toggle);
        self.error = None;
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.block_on(async {
                        service.set_remote_resource_enabled(&name, enabled).await
                    })?
                    .map_err(|error| error.to_string())
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.operation = None;
                    match result {
                        Ok(resource) => {
                            this.selected = Some(resource.name);
                            this.reload_resources(window, cx);
                            this.notice = Some(if enabled {
                                "远程资源已启用".into()
                            } else {
                                "远程资源已停用".into()
                            });
                        }
                        Err(error) => this.error = Some(error),
                    }
                    cx.notify();
                })
                .ok();
            });
        }));
        cx.notify();
    }

    pub(super) fn detect_resource(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        let url = self.inputs.url.read(cx).value().to_string();
        let (Some(service), core) = (self.service.clone(), self.core.clone()) else {
            return;
        };
        self.operation = Some(Operation::Detect);
        self.error = None;
        self.notice = None;
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.block_on(async { service.detect_remote_resource(&url).await })?
                        .map_err(|error| error.to_string())
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.operation = None;
                    match result {
                        Ok(detected) => {
                            this.syncing = true;
                            set_input(&this.inputs.url, &detected.normalized_url, window, cx);
                            if this.inputs.name.read(cx).value().trim().is_empty() {
                                let name = detected
                                    .meta
                                    .as_ref()
                                    .and_then(|meta| meta.name.clone())
                                    .unwrap_or_else(|| derive_name(&detected.normalized_url));
                                set_input(&this.inputs.name, &name, window, cx);
                            }
                            if this.inputs.description.read(cx).value().trim().is_empty()
                                && let Some(description) =
                                    detected.meta.as_ref().and_then(|meta| meta.desc.as_deref())
                            {
                                set_input(&this.inputs.description, description, window, cx);
                            }
                            if let Some(kind) = detected.kind {
                                this.kind = kind;
                            }
                            if let Some(dialect) = detected.dialect {
                                this.dialect = dialect;
                            }
                            if let Some(meta) = detected.meta {
                                this.icon = meta.icon;
                                this.arguments = meta.arguments;
                            }
                            this.syncing = false;
                            this.resource_dirty = true;
                            let mut notice = format!(
                                "嗅探完成：{} / {}，模块参数 {} 个",
                                kind_label(this.kind),
                                dialect_label(this.dialect),
                                this.arguments.len()
                            );
                            if let Some(status) = this.resource_dialect_status() {
                                notice.push('；');
                                notice.push_str(status);
                            }
                            this.notice = Some(notice);
                        }
                        Err(error) => this.error = Some(error),
                    }
                    cx.notify();
                })
                .ok();
            });
        }));
        cx.notify();
    }

    pub(super) fn fetch_resources(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() || self.dirty() || self.resources.is_empty() {
            return;
        }
        let (Some(service), core) = (self.service.clone(), self.core.clone()) else {
            return;
        };
        self.operation = Some(Operation::Fetch);
        self.error = None;
        self.notice = None;
        self.fetch_report = None;
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.block_on(async { service.fetch_remote_resources().await })?
                        .map_err(|error| error.to_string())
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.operation = None;
                    match result {
                        Ok(report) => {
                            this.notice = Some(fetch_notice(&report));
                            this.fetch_report = Some(report);
                            this.reload_resources(window, cx);
                        }
                        Err(error) => this.error = Some(error),
                    }
                    cx.notify();
                })
                .ok();
            });
        }));
        cx.notify();
    }

    pub(super) fn import_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() || !self.import_dirty {
            return;
        }
        if let Err(error) = validate_selection(self.import_dialect, None) {
            self.error = Some(error.to_string());
            cx.notify();
            return;
        }
        let content = self.inputs.import_content.read(cx).text().to_string();
        let dialect = self.import_dialect;
        let (Some(service), core) = (self.service.clone(), self.core.clone()) else {
            return;
        };
        self.operation = Some(Operation::Import);
        self.error = None;
        self.notice = None;
        self.import_summary = None;
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.block_on(async { service.import_script_config(&content, dialect).await })?
                        .map_err(|error| error.to_string())
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.operation = None;
                    match result {
                        Ok(summary) => {
                            this.syncing = true;
                            this.inputs
                                .import_content
                                .update(cx, |state, cx| state.set_value(String::new(), window, cx));
                            this.syncing = false;
                            this.import_dirty = false;
                            this.notice = Some(import_notice(&summary));
                            this.import_summary = Some(summary);
                        }
                        Err(error) => this.error = Some(error),
                    }
                    cx.notify();
                })
                .ok();
            });
        }));
        cx.notify();
    }

    pub(super) fn run_task(&mut self, id: String, cx: &mut Context<Self>) {
        if !self.running || self.busy() {
            return;
        }
        let Some(task_name) = self
            .tasks
            .iter()
            .find(|task| task.id == id)
            .map(|task| task.name.clone())
        else {
            return;
        };
        let core = self.core.clone();
        let generation = self.generation;
        self.operation = Some(Operation::RunTask);
        self.error = None;
        self.task_output = None;
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let output = core.run_script_task(&id)?;
                    let tasks = core.script_tasks()?;
                    Ok::<_, String>((output, tasks))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.operation = None;
                match result {
                    Ok((output, tasks)) => {
                        this.tasks = tasks;
                        this.task_output = Some((task_name, output));
                        this.notice = Some("任务已运行，结果和运行时间已更新".into());
                    }
                    Err(error) => this.error = Some(format!("运行任务失败：{error}")),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

fn derive_name(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let name = path
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or("resource");
    for suffix in [".sgmodule", ".plugin", ".loon", ".conf", ".js"] {
        if let Some(stem) = name.strip_suffix(suffix) {
            return stem.to_string();
        }
    }
    name.to_string()
}

pub(super) fn fetch_notice(report: &lanpath_client::remote::FetchReport) -> String {
    format!(
        "更新完成：资源 {}，脚本 {}，重写 {}，任务 {}，警告 {}",
        report.fetched,
        report.scripts,
        report.rewrites,
        report.tasks,
        report.warnings.len()
    )
}

fn import_notice(summary: &lanpath_client::remote::ImportSummary) -> String {
    format!(
        "导入完成：重写 {}，脚本 {}，任务 {}，主机名 {}，警告 {}",
        summary.rewrites,
        summary.scripts,
        summary.tasks,
        summary.hostnames,
        summary.warnings.len()
    )
}
