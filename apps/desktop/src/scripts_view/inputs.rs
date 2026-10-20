use gpui_kit::component::input::{EditorState, InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};

use super::ScriptsView;

pub(super) struct FormInputs {
    pub name: Entity<InputState>,
    pub description: Entity<InputState>,
    pub url: Entity<InputState>,
    pub interval: Entity<InputState>,
    pub arguments: Entity<EditorState>,
    pub import_content: Entity<EditorState>,
}

impl FormInputs {
    pub(super) fn new(
        window: &mut Window,
        cx: &mut Context<ScriptsView>,
    ) -> (Self, Vec<Subscription>) {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("资源名称"));
        let description = cx.new(|cx| InputState::new(window, cx).placeholder("资源描述（可选）"));
        let url = cx.new(|cx| {
            InputState::new(window, cx).placeholder("https://example.com/module.sgmodule")
        });
        let interval = cx.new(|cx| InputState::new(window, cx).placeholder("86400"));
        let arguments = cx.new(|cx| EditorState::new(window, cx).line_number(false));
        let import_content = cx.new(|cx| EditorState::new(window, cx).line_number(true));
        let mut subscriptions = Vec::new();
        for input in [&name, &description, &url, &interval] {
            subscriptions.push(cx.subscribe(input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) && !this.syncing {
                    this.resource_changed(cx);
                }
            }));
        }
        subscriptions.push(cx.subscribe(&arguments, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) && !this.syncing {
                this.resource_changed(cx);
            }
        }));
        subscriptions.push(
            cx.subscribe(&import_content, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) && !this.syncing {
                    this.import_changed(cx);
                }
            }),
        );
        (
            Self {
                name,
                description,
                url,
                interval,
                arguments,
                import_content,
            },
            subscriptions,
        )
    }

    pub(super) fn set_locked(
        &self,
        locked: bool,
        editing_existing: bool,
        cx: &mut Context<ScriptsView>,
    ) {
        self.name.update(cx, |state, cx| {
            state.set_readonly(locked || editing_existing, cx)
        });
        for input in [&self.description, &self.url, &self.interval] {
            input.update(cx, |state, cx| state.set_readonly(locked, cx));
        }
        for editor in [&self.arguments, &self.import_content] {
            editor.update(cx, |state, cx| state.set_readonly(locked, cx));
        }
    }
}
