use super::super::{Operation, ScriptsView, set_input};
use gpui_kit::component::WindowExt as _;
use gpui_kit::{AppContext as _, Context, PathPromptOptions, Window};
use lanpath_client::{
    import::parse_local_resource,
    remote::{LocalDocument, LocalIcon, LocalSource},
};

impl ScriptsView {
    pub(in crate::scripts_view) fn edit_local(
        &mut self,
        id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.locked || self.busy() || self.local.open {
            return;
        }
        let original = id.and_then(|id| self.local.rows.iter().find(|row| row.id == id).cloned());
        let source = original.as_ref().map(LocalDocument::source);
        self.syncing = true;
        set_input(
            &self.local.name,
            source
                .as_ref()
                .map(|source| source.name.as_str())
                .unwrap_or(""),
            window,
            cx,
        );
        self.local.content.update(cx, |state, cx| {
            state.set_value(
                source
                    .as_ref()
                    .and_then(|source| source.content.clone())
                    .unwrap_or_default(),
                window,
                cx,
            )
        });
        self.set_local_assets(
            original
                .as_ref()
                .map(LocalDocument::assets)
                .unwrap_or_default(),
            window,
            cx,
        );
        self.local.icon = source.as_ref().and_then(|source| source.icon.clone());
        self.local.dialect = source
            .as_ref()
            .map(|source| source.dialect)
            .unwrap_or(lanpath_client::remote::ScriptDialect::Surge);
        self.local.enabled = source.as_ref().is_none_or(|source| source.enabled);
        self.local.original = original;
        self.local.form_tab = 0;
        self.local.asset_index = 0;
        self.local.advisories.clear();
        self.syncing = false;
        self.local.dirty = false;
        self.error = None;
        self.notice = None;
        self.open_local_dialog(window, cx);
    }
    pub(in crate::scripts_view) fn save_local(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.locked || self.busy() || !self.local.open || !self.local.dirty {
            return;
        }
        let document = self.local_store().and_then(|store| {
            store
                .prepare(
                    self.local.original.as_ref(),
                    LocalSource {
                        name: self.local.name.read(cx).value().to_string(),
                        enabled: self.local.enabled,
                        dialect: self.local.dialect,
                        content: Some(self.local.content.read(cx).text().to_string()),
                        icon: self.local.icon.clone(),
                    },
                    &self.local.asset_values(cx),
                )
                .map_err(|error| error.to_string())
        });
        let document = match document {
            Ok(document) => document,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        let creating = self.local.original.is_none();
        self.run_local_write(
            window,
            cx,
            move |store| async move {
                store
                    .save(document, creating)
                    .await
                    .map_err(|error| error.to_string())
            },
            true,
        );
    }
    pub(super) fn run_local_write<F, Fut>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        action: F,
        close: bool,
    ) where
        F: FnOnce(lanpath_client::remote::LocalResourceStore) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), String>> + Send + 'static,
    {
        let store = match self.local_store() {
            Ok(store) => store,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        self.operation = Some(Operation::Local);
        self.error = None;
        self.notice = None;
        let core = self.core.clone();
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.block_on(async {
                        action(store.clone()).await?;
                        store.load().map_err(|error| error.to_string())
                    })?
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.operation = None;
                match result {
                    Ok(rows) => {
                        this.local.rows = rows;
                        this.local.load_error = None;
                        if close {
                            this.local.dirty = false;
                            this.local.open = false;
                            window.close_dialog(cx);
                        }
                        this.notice = Some("本地资源已保存 · 下次启动生效".into());
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(super) fn identify_local_assets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        let content = self.local.content.read(cx).text().to_string();
        if content.trim().is_empty() {
            self.error = Some("请先填写配置片段".into());
            cx.notify();
            return;
        }
        let parsed = match parse_local_resource(&content, self.local.dialect) {
            Ok(parsed) => parsed,
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let blocking: Vec<_> = parsed.blocking_warnings().map(String::as_str).collect();
        if !blocking.is_empty() {
            self.error = Some(blocking.join("；"));
            cx.notify();
            return;
        }
        self.local.advisories = parsed.advisory_warnings.clone();
        let old: std::collections::BTreeMap<_, _> =
            self.local.asset_values(cx).into_iter().collect();
        let references: std::collections::BTreeSet<_> = parsed
            .scripts
            .iter()
            .map(|rule| rule.source_id.clone())
            .chain(
                parsed
                    .task_scripts
                    .iter()
                    .map(|(task, _)| task.source_id.clone()),
            )
            .collect();
        if old.keys().any(|reference| !references.contains(reference)) {
            let view = cx.weak_entity();
            window.open_alert_dialog(cx, move |dialog, _, _| {
                let view = view.clone();
                let references = references.clone();
                let old = old.clone();
                dialog
                    .title("移除不再引用的脚本？")
                    .description(
                        "识别后，已从片段移除引用的 JS 将从当前草稿移除；已保存资源保持不变。",
                    )
                    .confirm()
                    .ok_text("更新关联")
                    .cancel_text("保留草稿")
                    .on_ok(move |_, window, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.apply_local_references(&references, &old, window, cx);
                        });
                        true
                    })
            });
        } else {
            self.apply_local_references(&references, &old, window, cx);
        }
    }
    pub(super) fn apply_local_references(
        &mut self,
        references: &std::collections::BTreeSet<String>,
        old: &std::collections::BTreeMap<String, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.syncing = true;
        self.set_local_assets(
            references
                .iter()
                .map(|reference| {
                    (
                        reference.clone(),
                        old.get(reference).cloned().unwrap_or_default(),
                    )
                })
                .collect(),
            window,
            cx,
        );
        self.syncing = false;
        self.local_changed(cx);
        self.notice = Some(format!(
            "已识别 {} 个关联 JS；按片段中的地址绑定，无需联网",
            references.len()
        ));
    }
    pub(super) fn toggle_local(
        &mut self,
        id: String,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.locked || self.busy() || self.local.open {
            return;
        }
        self.run_local_write(
            window,
            cx,
            move |store| async move {
                store
                    .set_enabled(&id, enabled)
                    .await
                    .map_err(|error| error.to_string())
            },
            false,
        );
    }
    pub(super) fn delete_local(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() || self.local.open {
            return;
        }
        let view = cx.weak_entity();
        window.open_alert_dialog(cx,move |dialog,_,_| { let view = view.clone(); let id = id.clone(); dialog.title("删除本地资源？").description("配置片段、关联 JS 与本地图标将一起删除。正在运行的核心不受影响，下次启动时生效。").confirm().ok_text("删除资源").cancel_text("取消").on_ok(move |_,window,cx| { let id = id.clone(); let _ = view.update(cx,|this,cx| { if this.locked || this.busy() { return; } this.run_local_write(window,cx,move |store| async move {store.remove(&id).await.map_err(|error| error.to_string())},false); }); true }) });
    }
    pub(super) fn choose_local_file(
        &mut self,
        script: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.locked || self.busy() {
            return;
        }
        self.operation = Some(Operation::Local);
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                if script.is_some() {
                    "选择 JavaScript 文件"
                } else {
                    "选择 PNG、JPEG 或 WebP 图标"
                }
                .into(),
            ),
        });
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = match prompt.await {
                Ok(Ok(Some(paths))) if paths.len() == 1 => {
                    let path = paths[0].clone();
                    cx.background_spawn(async move {
                        use std::io::Read as _;
                        let mut bytes = Vec::new();
                        std::fs::File::open(&path)
                            .map_err(|error| error.to_string())?
                            .take(2 * 1024 * 1024 + 1)
                            .read_to_end(&mut bytes)
                            .map_err(|error| error.to_string())?;
                        if bytes.len() > 2 * 1024 * 1024 {
                            return Err("所选文件超过 2 MiB".to_owned());
                        }
                        let name = path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("icon")
                            .to_owned();
                        Ok(Some((name, bytes)))
                    })
                    .await
                }
                Ok(Ok(None)) => Ok(None),
                _ => Err("无法读取所选文件，请重新选择".into()),
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.operation = None;
                match result {
                    Ok(Some((name, bytes))) => {
                        let changed = if let Some(index) = script {
                            match String::from_utf8(bytes) {
                                Ok(text) => {
                                    if let Some(asset) = this.local.assets.get(index) {
                                        asset.editor.update(cx, |state, cx| {
                                            state.set_value(text, window, cx)
                                        });
                                    }
                                    true
                                }
                                Err(_) => {
                                    this.error = Some("JS 文件必须为 UTF-8 文本".into());
                                    false
                                }
                            }
                        } else {
                            match LocalIcon::from_bytes(&name, &bytes) {
                                Ok(icon) => {
                                    this.local.icon = Some(icon);
                                    true
                                }
                                Err(error) => {
                                    this.error = Some(error.to_string());
                                    false
                                }
                            }
                        };
                        if changed {
                            this.local_changed(cx);
                        }
                    }
                    Ok(None) => {}
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}
