use std::sync::Arc;

use gpui_kit::component::{
    ActiveTheme as _, Disableable as _,
    button::Button,
    input::{Editor, EditorState},
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, InteractiveElement as _, IntoElement,
    KeyBinding, ParentElement as _, Render, Role, StatefulInteractiveElement as _, Styled as _,
    Task, Window, div,
};

use crate::runtime::DesktopRuntime;

gpui_kit::actions!(configuration, [ReloadConfiguration]);

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new(
        "ctrl-r",
        ReloadConfiguration,
        Some("Configuration"),
    )]);
}

pub(crate) struct ConfigEvent;

pub(crate) struct ConfigView {
    core: Arc<DesktopRuntime>,
    editor: Entity<EditorState>,
    diagnostic: Entity<EditorState>,
    loaded: bool,
    locked: bool,
    busy: bool,
    refresh_pending: bool,
    error: bool,
    notice: String,
    task: Option<Task<()>>,
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
        editor.update(cx, |state, cx| state.set_readonly(true, cx));
        let diagnostic = cx.new(|cx| EditorState::new(window, cx).line_number(false));
        diagnostic.update(cx, |state, cx| state.set_readonly(true, cx));
        let mut view = Self {
            core,
            editor,
            diagnostic,
            loaded: false,
            locked: false,
            busy: false,
            refresh_pending: false,
            error: false,
            notice: "尚未生成配置预览".into(),
            task: None,
        };
        view.refresh(window, cx);
        view
    }

    pub(crate) fn ready_for_start(&self) -> bool {
        self.loaded && !self.busy && !self.error
    }

    pub(crate) fn busy(&self) -> bool {
        self.busy
    }

    pub(crate) fn set_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        self.locked = locked;
        if locked {
            self.refresh_pending = false;
        }
        cx.notify();
    }

    pub(crate) fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked {
            return;
        }
        if self.busy {
            self.refresh_pending = true;
            return;
        }
        self.busy = true;
        self.error = false;
        self.loaded = false;
        self.notice = "正在从当前订阅、配置片段与覆写模板生成预览…".into();
        cx.emit(ConfigEvent);
        cx.notify();
        let core = self.core.clone();
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let service = core.business()?.clone();
                    core.block_on(service.preview_config())?
                        .map_err(|error| error.to_string())
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.busy = false;
                    match result {
                        Ok(text) => {
                            this.editor
                                .update(cx, |state, cx| state.set_value(text, window, cx));
                            this.loaded = true;
                            this.error = false;
                            this.notice =
                                "生成配置已通过实验能力边界和真实核心校验；启动时会重新生成并校验"
                                    .into();
                        }
                        Err(error) => {
                            this.error = true;
                            this.diagnostic
                                .update(cx, |state, cx| state.set_value(error, window, cx));
                            this.notice = "配置预览失败；修复订阅或覆写后重新生成".into();
                        }
                    }
                    cx.emit(ConfigEvent);
                    if this.refresh_pending && !this.locked {
                        this.refresh_pending = false;
                        this.refresh(window, cx);
                    }
                    cx.notify();
                })
                .ok();
            });
        }));
    }
}

impl Render for ConfigView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = self.notice.clone();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_3()
            .key_context("Configuration")
            .on_action(cx.listener(|this, _: &ReloadConfiguration, window, cx| {
                this.refresh(window, cx)
            }))
            .child(crate::design::page_header("sing-box 配置预览", "查看最终合成结果与核心校验信息",
crate::design::toolbar()
                    .child(
                        Button::new("configuration-reload")
                            .label("重新生成")
                            .disabled(self.locked || self.busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.refresh(window, cx)
                            })),
                    ), cx))
            .child(
                div()
                    .id("configuration-status")
                    .role(Role::Status)
                    .aria_label(status.clone())
                    .text_sm()
                    .text_color(if self.error {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(status),
            )
            .children(self.error.then(|| {
                div().h_24().flex_shrink_0().child(
                    Editor::new(&self.diagnostic)
                        .aria_label("配置生成错误")
                        .readonly(true)
                        .size_full(),
                )
            }))
            .child(
                Editor::new(&self.editor)
                    .aria_label("生成核心配置")
                    .readonly(true)
                    .size_full(),
            )
            .child(crate::design::help("configuration-help", "配置合成顺序与快捷键", div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("只读结果：订阅节点 → 配置片段 → 远程/本地 YAML → 远程/本地 JS → 固定实验 loopback 入站/API。Ctrl+R 重新生成。")))
    }
}
