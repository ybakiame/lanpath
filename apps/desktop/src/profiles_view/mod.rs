use std::sync::Arc;

use gpui_kit::component::{
    WindowExt as _,
    input::{EditorState, InputEvent, InputState},
};
use gpui_kit::{AppContext as _, Context, Entity, EventEmitter, Subscription, Task, Window};
use lanpath_client::Profile;
use uuid::Uuid;

use crate::{business::BusinessService, runtime::DesktopRuntime};

mod card;
mod render;

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
    editor_open: bool,
    editor_confirm_discard: bool,
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
            editor_open: false,
            editor_confirm_discard: false,
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
        self.editor_confirm_discard = false;
        self.error = None;
        self.notice = None;
        cx.notify();
    }

    fn request_select(
        &mut self,
        id: Option<Uuid>,
        window: &mut Window,
        cx: &mut Context<Self>,
        open_editor: bool,
    ) {
        if self.locked || self.busy() {
            return;
        }
        if !self.dirty {
            self.select(id, window, cx);
            if open_editor {
                self.open_editor(window, cx);
            }
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
                    let _ = view.update(cx, |view, cx| {
                        if !view.locked && !view.busy() {
                            view.select(id, window, cx);
                            if open_editor {
                                view.open_editor(window, cx);
                            }
                        }
                    });
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

    pub(crate) fn save_from_shortcut(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dirty {
            self.run(Operation::Save, window, cx);
        }
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
                            if matches!(operation, Operation::Save) {
                                this.close_editor(window, cx);
                            }
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
        cx.notify();
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
