use gpui_kit::component::{
    input::{EditorState, InputEvent, InputState},
    select::{SelectEvent, SelectState},
};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};

use super::ScriptsView;

pub(super) struct FormInputs {
    pub name: Entity<InputState>,
    pub description: Entity<InputState>,
    pub url: Entity<InputState>,
    pub icon: Entity<InputState>,
    pub interval: Entity<InputState>,
    pub interval_unit: Entity<SelectState<Vec<&'static str>>>,
    pub arguments: Entity<EditorState>,
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
        let icon = cx.new(|cx| InputState::new(window, cx).placeholder("图标 URL（可选）"));
        let interval = cx.new(|cx| InputState::new(window, cx).placeholder("1"));
        let arguments = cx.new(|cx| EditorState::new(window, cx).line_number(false));
        let interval_unit =
            cx.new(|cx| SelectState::new(vec!["秒", "分钟", "小时", "天"], None, window, cx));
        let mut subscriptions = vec![cx.subscribe(
            &interval_unit,
            |this, _, event: &SelectEvent<Vec<&'static str>>, cx| {
                if matches!(event, SelectEvent::Confirm(_)) && !this.syncing {
                    this.resource_changed(cx);
                }
            },
        )];
        for input in [&name, &description, &url, &icon, &interval] {
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
        (
            Self {
                name,
                description,
                url,
                icon,
                interval,
                interval_unit,
                arguments,
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
        for input in [&self.description, &self.url, &self.icon, &self.interval] {
            input.update(cx, |state, cx| state.set_readonly(locked, cx));
        }
        self.arguments
            .update(cx, |state, cx| state.set_readonly(locked, cx));
    }
}
