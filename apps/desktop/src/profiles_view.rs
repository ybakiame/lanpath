use std::sync::Arc;

use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    input::{Editor, EditorState, Input, InputEvent, InputState},
};
use gpui_kit::{
    AppContext as _, Context, Entity, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Role, StatefulInteractiveElement as _, Styled as _, Subscription,
    Task, Window, div,
};
use lanpath_client::Profile;
use uuid::Uuid;

use crate::{business::BusinessService, runtime::DesktopRuntime};

pub(crate) struct ProfilesChanged;

#[derive(Clone, Copy)]
enum Operation {
    Save,
    Delete,
}

pub(crate) struct ProfilesView {
    service: Option<BusinessService>,
    profiles: Vec<Profile>,
    selected: Option<Uuid>,
    name: Entity<InputState>,
    yaml_url: Entity<InputState>,
    js_url: Entity<InputState>,
    yaml: Entity<EditorState>,
    js: Entity<EditorState>,
    dirty: bool,
    locked: bool,
    busy: Option<Operation>,
    error: Option<String>,
    notice: Option<String>,
    task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ProfilesChanged> for ProfilesView {}

impl ProfilesView {
    pub(crate) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let service = core.business().ok().cloned();
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("模板名称"));
        let yaml_url =
            cx.new(|cx| InputState::new(window, cx).placeholder("远程 YAML URL（可选）"));
        let js_url = cx.new(|cx| InputState::new(window, cx).placeholder("远程 JS URL（可选）"));
        let yaml = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("yaml")
                .line_number(true)
        });
        let js = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("javascript")
                .line_number(true)
        });
        let mut subscriptions = Vec::new();
        for input in [&name, &yaml_url, &js_url] {
            subscriptions.push(cx.subscribe(input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.changed(cx);
                }
            }));
        }
        for editor in [&yaml, &js] {
            subscriptions.push(cx.subscribe(editor, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.changed(cx);
                }
            }));
        }
        let profiles = service
            .as_ref()
            .and_then(|service| service.profiles().ok())
            .unwrap_or_default();
        let selected = profiles.first().map(|profile| profile.id);
        let mut view = Self {
            service,
            profiles,
            selected: None,
            name,
            yaml_url,
            js_url,
            yaml,
            js,
            dirty: false,
            locked: false,
            busy: None,
            error: None,
            notice: None,
            task: None,
            _subscriptions: subscriptions,
        };
        view.select(selected, window, cx);
        view
    }

    pub(crate) fn dirty(&self) -> bool {
        self.dirty
    }

    pub(crate) fn busy(&self) -> bool {
        self.busy.is_some()
    }

    pub(crate) fn set_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        self.locked = locked;
        for input in [&self.name, &self.yaml_url, &self.js_url] {
            input.update(cx, |state, cx| state.set_readonly(locked, cx));
        }
        for editor in [&self.yaml, &self.js] {
            editor.update(cx, |state, cx| state.set_readonly(locked, cx));
        }
        cx.notify();
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        self.error = None;
        self.notice = None;
        cx.notify();
    }

    fn select(&mut self, id: Option<Uuid>, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = id;
        let profile = id.and_then(|id| self.profiles.iter().find(|profile| profile.id == id));
        set_input(
            &self.name,
            profile.map(|item| item.name.as_str()).unwrap_or(""),
            window,
            cx,
        );
        set_input(
            &self.yaml_url,
            profile
                .and_then(|item| item.yaml_url.as_deref())
                .unwrap_or(""),
            window,
            cx,
        );
        set_input(
            &self.js_url,
            profile
                .and_then(|item| item.js_url.as_deref())
                .unwrap_or(""),
            window,
            cx,
        );
        self.yaml.update(cx, |state, cx| {
            state.set_value(
                profile
                    .map(|item| item.yaml_override.as_str())
                    .unwrap_or("")
                    .to_string(),
                window,
                cx,
            )
        });
        self.js.update(cx, |state, cx| {
            state.set_value(
                profile
                    .map(|item| item.js_override.as_str())
                    .unwrap_or("")
                    .to_string(),
                window,
                cx,
            )
        });
        self.dirty = false;
        self.error = None;
        self.notice = None;
        cx.notify();
    }

    fn request_select(&mut self, id: Option<Uuid>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dirty {
            self.select(id, window, cx);
            return;
        }
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .title("放弃未保存的覆写修改？")
                .description("切换模板会丢弃当前表单修改，已保存模板不受影响。")
                .confirm()
                .ok_text("放弃修改")
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| view.select(id, window, cx));
                    true
                })
        });
    }

    fn request_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selected) = self.selected else {
            return;
        };
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .title("删除覆写模板？")
                .description("有关联订阅时删除会被拒绝；生产 Lanpath 数据不会受到影响。")
                .confirm()
                .ok_text("删除")
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| {
                        if view.selected == Some(selected) {
                            view.run(Operation::Delete, window, cx);
                        }
                    });
                    true
                })
        });
    }

    fn run(&mut self, operation: Operation, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked
            || self.busy()
            || (matches!(operation, Operation::Delete) && self.selected.is_none())
        {
            return;
        }
        let selected = self.selected;
        let name = self.name.read(cx).value().to_string();
        let yaml_url = text_option(self.yaml_url.read(cx).value().as_ref());
        let js_url = text_option(self.js_url.read(cx).value().as_ref());
        let yaml = self.yaml.read(cx).text().to_string();
        let js = self.js.read(cx).text().to_string();
        self.busy = Some(operation);
        self.error = None;
        self.notice = None;
        let Some(service) = self.service.clone() else {
            self.busy = None;
            self.error = Some("实验数据目录不可用".into());
            cx.notify();
            return;
        };
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let selected = match operation {
                        Operation::Save => Some(
                            service
                                .save_profile(selected, &name, yaml, js, yaml_url, js_url)?
                                .id,
                        ),
                        Operation::Delete => {
                            let id = selected.ok_or_else(|| {
                                lanpath_common::LanpathError::Client("未选择覆写模板".into())
                            })?;
                            service.remove_profile(id)?;
                            None
                        }
                    };
                    Ok::<_, lanpath_common::LanpathError>((service.profiles()?, selected))
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.busy = None;
                    match result {
                        Ok((profiles, selected)) => {
                            this.profiles = profiles;
                            let next = selected
                                .or_else(|| this.profiles.first().map(|profile| profile.id));
                            this.select(next, window, cx);
                            this.notice = Some(
                                match operation {
                                    Operation::Save => "覆写模板已校验并保存",
                                    Operation::Delete => "覆写模板已删除",
                                }
                                .into(),
                            );
                            cx.emit(ProfilesChanged);
                        }
                        Err(error) => this.error = Some(error.to_string()),
                    }
                    cx.notify();
                })
                .ok()
            });
        }));
    }
}

impl Render for ProfilesView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.locked || self.busy();
        let status = self
            .error
            .clone()
            .or_else(|| self.notice.clone())
            .unwrap_or_else(|| "YAML 深合并后执行 JS；远程覆写失败时使用独立目录缓存".into());
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child("覆写模板")
                    .child(
                        Button::new("profile-new")
                            .label("新建")
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.request_select(None, window, cx)
                            })),
                    )
                    .child(
                        Button::new("profile-save")
                            .label("保存")
                            .primary()
                            .disabled(disabled || !self.dirty)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run(Operation::Save, window, cx)
                            })),
                    )
                    .child(
                        Button::new("profile-delete")
                            .label("删除")
                            .danger()
                            .disabled(disabled || self.selected.is_none())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.request_delete(window, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .id("profile-status")
                    .role(Role::Status)
                    .aria_label(status.clone())
                    .text_sm()
                    .text_color(if self.error.is_some() {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(status),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .children(self.profiles.iter().map(|profile| {
                        let id = profile.id;
                        Button::new(format!("profile-{id}"))
                            .label(profile.name.clone())
                            .selected(Some(id) == self.selected)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.request_select(Some(id), window, cx)
                            }))
                    })),
            )
            .child(
                Input::new(&self.name)
                    .aria_label("覆写模板名称")
                    .disabled(disabled),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div().flex_1().child(
                            Input::new(&self.yaml_url)
                                .aria_label("远程 YAML URL")
                                .disabled(disabled),
                        ),
                    )
                    .child(
                        div().flex_1().child(
                            Input::new(&self.js_url)
                                .aria_label("远程 JS URL")
                                .disabled(disabled),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .gap_1()
                            .child("YAML 深合并")
                            .child(
                                Editor::new(&self.yaml)
                                    .aria_label("YAML 覆写正文")
                                    .readonly(disabled)
                                    .size_full(),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .gap_1()
                            .child("JS main(config)")
                            .child(
                                Editor::new(&self.js)
                                    .aria_label("JS 覆写正文")
                                    .readonly(disabled)
                                    .size_full(),
                            ),
                    ),
            )
    }
}

fn text_option(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn set_input(
    input: &Entity<InputState>,
    value: &str,
    window: &mut Window,
    cx: &mut Context<ProfilesView>,
) {
    input.update(cx, |state, cx| {
        state.set_value(value.to_string(), window, cx)
    });
}
