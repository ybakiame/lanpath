use std::sync::Arc;

use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    input::{Editor, EditorState, InputEvent},
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, InteractiveElement as _, IntoElement,
    KeyBinding, ParentElement as _, Render, Role, StatefulInteractiveElement as _, Styled as _,
    Subscription, Task, Window, div,
};

use crate::runtime::DesktopRuntime;

gpui_kit::actions!(
    configuration,
    [SaveConfiguration, CheckConfiguration, ReloadConfiguration]
);

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-s", SaveConfiguration, Some("Configuration")),
        // Match at input depth; the editor otherwise consumes Ctrl+Enter as a newline.
        KeyBinding::new(
            "ctrl-enter",
            CheckConfiguration,
            Some("Configuration > Input"),
        ),
        KeyBinding::new("ctrl-r", ReloadConfiguration, Some("Configuration")),
    ]);
}

pub(crate) struct ConfigEvent;

#[derive(Clone, Copy, PartialEq)]
enum Operation {
    Load,
    Check,
    Save,
}

enum Outcome {
    Loaded(String, bool),
    Checked,
    Saved,
}

pub(crate) struct ConfigView {
    core: Arc<DesktopRuntime>,
    editor: Entity<EditorState>,
    diagnostic: Entity<EditorState>,
    loaded: bool,
    has_saved: bool,
    dirty: bool,
    locked: bool,
    operation: Option<Operation>,
    error: bool,
    notice: Option<&'static str>,
    task: Option<Task<()>>,
    _input_subscription: Subscription,
}

impl EventEmitter<ConfigEvent> for ConfigView {}

impl ConfigView {
    pub(crate) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("json")
                .line_number(true)
                .folding(true)
        });
        let diagnostic = cx.new(|cx| EditorState::new(window, cx).line_number(false));
        let subscription = cx.subscribe(&editor, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change)
                && (!this.dirty || this.error || this.notice.is_some())
            {
                this.dirty = true;
                this.error = false;
                this.notice = None;
                this.changed(cx);
            }
        });
        let mut view = Self {
            core,
            editor,
            diagnostic,
            loaded: false,
            has_saved: false,
            dirty: false,
            locked: false,
            operation: None,
            error: false,
            notice: None,
            task: None,
            _input_subscription: subscription,
        };
        view.start(Operation::Load, window, cx);
        view
    }

    pub(crate) fn ready_for_start(&self) -> bool {
        self.loaded && self.has_saved && !self.dirty && self.operation.is_none() && !self.error
    }

    pub(crate) fn busy(&self) -> bool {
        self.operation.is_some()
    }
    pub(crate) fn dirty(&self) -> bool {
        self.dirty
    }

    pub(crate) fn set_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        if self.locked != locked {
            self.locked = locked;
            self.sync_readonly(cx);
            cx.notify();
        }
    }

    fn changed(&self, cx: &mut Context<Self>) {
        cx.emit(ConfigEvent);
        cx.notify();
    }

    fn sync_readonly(&self, cx: &mut Context<Self>) {
        let readonly = self.locked || self.busy();
        self.editor
            .update(cx, |editor, cx| editor.set_readonly(readonly, cx));
    }

    fn start(&mut self, operation: Operation, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked
            || self.busy()
            || (operation != Operation::Load && !self.loaded)
            || (operation == Operation::Save && self.has_saved && !self.dirty)
        {
            return;
        }
        let text = if operation == Operation::Load {
            String::new()
        } else {
            self.editor.read(cx).text().to_string()
        };
        self.operation = Some(operation);
        self.error = false;
        self.notice = None;
        self.sync_readonly(cx);
        self.changed(cx);
        let core = self.core.clone();
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.configuration().and_then(|store| {
                        core.block_on(async {
                            match operation {
                                Operation::Load => store.load().await.and_then(|text| match text {
                                    Some(text) => Ok(Outcome::Loaded(text, true)),
                                    None => store
                                        .starter_text()
                                        .map(|text| Outcome::Loaded(text, false)),
                                }),
                                Operation::Check => {
                                    store.validate(&text).await.map(|()| Outcome::Checked)
                                }
                                Operation::Save => store.save(&text).await.map(|()| Outcome::Saved),
                            }
                        })
                        .and_then(|result| result.map_err(|error| error.to_string()))
                    })
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.operation = None;
                    match result {
                        Ok(Outcome::Loaded(text, saved)) => {
                            this.editor
                                .update(cx, |editor, cx| editor.set_value(text, window, cx));
                            this.loaded = true;
                            this.has_saved = saved;
                            this.dirty = false;
                            this.notice = Some(if saved {
                                "已载入保存的原文；启动前仍会重新校验"
                            } else {
                                "新建直连配置，尚未保存；保存后才能启动"
                            });
                        }
                        Ok(Outcome::Checked) => {
                            this.notice = Some("配置校验通过；校验不会保存或启动核心")
                        }
                        Ok(Outcome::Saved) => {
                            this.has_saved = true;
                            this.dirty = false;
                            this.notice = Some("已保存；下次启动使用此配置");
                        }
                        Err(error) => {
                            this.error = true;
                            this.diagnostic.update(cx, |diagnostic, cx| {
                                diagnostic.set_value(error, window, cx)
                            });
                        }
                    }
                    this.sync_readonly(cx);
                    this.changed(cx);
                })
                .ok();
            });
        }));
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        if !self.dirty {
            self.start(Operation::Load, window, cx);
            return;
        }
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .title("放弃未保存的配置修改？")
                .description("将从实验目录重新载入配置，当前未保存修改会丢失。")
                .confirm()
                .ok_text("放弃修改")
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| view.start(Operation::Load, window, cx));
                    true
                })
        });
    }

    fn status(&self) -> &str {
        if let Some(operation) = self.operation {
            return match operation {
                Operation::Load => "正在载入配置…",
                Operation::Check => "正在执行核心校验…",
                Operation::Save => "正在校验并保存…",
            };
        }
        if self.error {
            return "操作失败；未更新已保存配置，错误详情如下";
        }
        if let Some(notice) = self.notice {
            return notice;
        }
        if self.dirty {
            return "配置有未保存修改；保存后才能启动";
        }
        if self.has_saved {
            "配置已保存"
        } else {
            "尚未保存配置"
        }
    }
}

impl Render for ConfigView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.locked || self.busy();
        let status = self.status().to_string();
        let directory = self
            .core
            .data_dir()
            .map(|path| format!("文档：{}", path.join("configuration.json").display()));
        div().flex().flex_col().flex_1().min_h_0().gap_2().key_context("Configuration")
            .on_action(cx.listener(|this, _: &SaveConfiguration, window, cx| this.start(Operation::Save, window, cx)))
            .on_action(cx.listener(|this, _: &CheckConfiguration, window, cx| this.start(Operation::Check, window, cx)))
            .on_action(cx.listener(|this, _: &ReloadConfiguration, window, cx| this.reload(window, cx)))
            .child(div().flex().items_center().gap_2().child("核心配置")
                .child(Button::new("configuration-check").label("校验").disabled(disabled || !self.loaded)
                    .on_click(cx.listener(|this, _, window, cx| this.start(Operation::Check, window, cx))))
                .child(Button::new("configuration-save").label("保存").primary()
                    .disabled(disabled || !self.loaded || (self.has_saved && !self.dirty))
                    .on_click(cx.listener(|this, _, window, cx| this.start(Operation::Save, window, cx))))
                .child(Button::new("configuration-reload").label(if self.dirty { "放弃修改…" } else { "重新载入" }).disabled(disabled)
                    .on_click(cx.listener(|this, _, window, cx| this.reload(window, cx)))))
            .children(directory.map(|path| div().text_sm().text_color(cx.theme().muted_foreground).child(path)))
            .child(div().id("configuration-status").role(Role::Status).aria_label(status.clone()).text_sm()
                .text_color(if self.error { cx.theme().danger } else { cx.theme().muted_foreground }).child(status))
            .children(self.error.then(|| div().h_24().flex_shrink_0()
                .child(Editor::new(&self.diagnostic).aria_label("配置操作错误").readonly(true).size_full())))
            .child(Editor::new(&self.editor).aria_label("核心配置正文").readonly(disabled).size_full())
            .child(div().text_sm().text_color(cx.theme().muted_foreground).child(if self.locked {
                "核心运行或切换期间配置只读；停止后编辑，保存后显式启动，不做热重载。"
            } else { "完整 sing-box JSON；仅允许实验 loopback 入站/API及目录内资源。Ctrl+S 保存，Ctrl+Enter 校验。" }))
    }
}
