mod operations;
mod render;

use std::sync::Arc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, EventEmitter, Subscription, Task, Window};

use crate::{business::SystemSettingsDocument, runtime::DesktopRuntime};

pub(crate) struct SystemSettingsChanged;

pub(crate) struct SystemSettingsView {
    core: Arc<DesktopRuntime>,
    saved: Option<SystemSettingsDocument>,
    draft: Option<SystemSettingsDocument>,
    mixed_port: Entity<InputState>,
    api_port: Entity<InputState>,
    api_secret: Entity<InputState>,
    api_ui: Entity<InputState>,
    dirty: bool,
    syncing: bool,
    locked: bool,
    closing: bool,
    running: bool,
    working: Option<&'static str>,
    error: Option<String>,
    notice: Option<String>,
    task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<SystemSettingsChanged> for SystemSettingsView {}

impl SystemSettingsView {
    pub(crate) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mixed_port = cx.new(|cx| InputState::new(window, cx).placeholder("1–65535"));
        let api_port = cx.new(|cx| InputState::new(window, cx).placeholder("1–65535"));
        let api_secret = cx.new(|cx| InputState::new(window, cx).masked(true));
        let api_ui = cx.new(|cx| InputState::new(window, cx).placeholder("metacubexd / zashboard"));
        let mut subscriptions = Vec::new();
        for input in [&mixed_port, &api_port, &api_secret, &api_ui] {
            subscriptions.push(cx.subscribe(input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) && !this.syncing {
                    this.changed(cx);
                }
            }));
        }
        let mut view = Self {
            core,
            saved: None,
            draft: None,
            mixed_port,
            api_port,
            api_secret,
            api_ui,
            dirty: false,
            syncing: false,
            locked: false,
            closing: false,
            running: false,
            working: None,
            error: None,
            notice: None,
            task: None,
            _subscriptions: subscriptions,
        };
        view.load(window, cx);
        view
    }

    pub(crate) fn busy(&self) -> bool {
        self.working.is_some()
    }
    pub(crate) fn dirty(&self) -> bool {
        self.dirty
    }

    pub(crate) fn set_locked(&mut self, value: bool, cx: &mut Context<Self>) {
        if self.locked != value {
            self.locked = value;
            self.sync_readonly(cx);
            cx.notify();
        }
    }
    pub(crate) fn set_closing(&mut self, value: bool, cx: &mut Context<Self>) {
        if self.closing != value {
            self.closing = value;
            self.sync_readonly(cx);
            cx.notify();
        }
    }
    pub(crate) fn set_running(&mut self, value: bool, cx: &mut Context<Self>) {
        if self.running != value {
            self.running = value;
            self.sync_readonly(cx);
            cx.notify();
        }
    }
    fn unavailable(&self) -> bool {
        self.locked || self.closing || self.running || self.busy()
    }
    fn editing_disabled(&self) -> bool {
        self.unavailable() || self.draft.is_none()
    }

    fn sync_readonly(&self, cx: &mut Context<Self>) {
        let readonly = self.editing_disabled();
        for input in [
            &self.mixed_port,
            &self.api_port,
            &self.api_secret,
            &self.api_ui,
        ] {
            input.update(cx, |state, cx| state.set_readonly(readonly, cx));
        }
    }

    fn document(&self, cx: &Context<Self>) -> Result<SystemSettingsDocument, String> {
        let mut document = self.draft.clone().ok_or("尚未载入网络设置")?;
        let port = |input: &Entity<InputState>, label: &str, allow_zero: bool| {
            parse_port(input.read(cx).value().as_ref(), allow_zero).ok_or_else(|| {
                format!(
                    "{label}必须是 {}–65535 之间的整数",
                    if allow_zero { 0 } else { 1 }
                )
            })
        };
        document.mixed_port = port(&self.mixed_port, "混合代理端口", false)?;
        document.clash_api_port = port(
            &self.api_port,
            "Clash API 端口",
            !document.clash_api_enabled,
        )?;
        document.clash_api_secret = self.api_secret.read(cx).value().to_string();
        document.clash_api_ui = self.api_ui.read(cx).value().to_string();
        Ok(document)
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.dirty = match (&self.draft, &self.saved) {
            (Some(draft), Some(saved)) => {
                draft != saved
                    || parse_port(self.mixed_port.read(cx).value().as_ref(), false)
                        != Some(saved.mixed_port)
                    || parse_port(
                        self.api_port.read(cx).value().as_ref(),
                        !draft.clash_api_enabled,
                    ) != Some(saved.clash_api_port)
                    || self.api_secret.read(cx).value().as_ref() != saved.clash_api_secret.as_str()
                    || self.api_ui.read(cx).value().as_ref() != saved.clash_api_ui.as_str()
            }
            (draft, _) => draft.is_some(),
        };
        self.notice = None;
        self.error = None;
        cx.notify();
    }

    fn apply(
        &mut self,
        document: SystemSettingsDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.syncing = true;
        self.mixed_port.update(cx, |state, cx| {
            state.set_value(document.mixed_port.to_string(), window, cx)
        });
        self.api_port.update(cx, |state, cx| {
            state.set_value(document.clash_api_port.to_string(), window, cx)
        });
        self.api_secret.update(cx, |state, cx| {
            state.set_value(document.clash_api_secret.clone(), window, cx)
        });
        self.api_ui.update(cx, |state, cx| {
            state.set_value(document.clash_api_ui.clone(), window, cx)
        });
        self.syncing = false;
        self.saved = Some(document.clone());
        self.draft = Some(document);
        self.dirty = false;
        self.sync_readonly(cx);
    }
}

fn parse_port(value: &str, allow_zero: bool) -> Option<u16> {
    value
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| allow_zero || *port != 0)
}

#[cfg(test)]
mod tests {
    use super::parse_port;

    #[test]
    fn disabled_api_can_preserve_zero_but_listeners_require_positive_ports() {
        assert_eq!(parse_port("0", true), Some(0));
        assert_eq!(parse_port("0", false), None);
        for allow_zero in [false, true] {
            assert_eq!(parse_port(" 65535 ", allow_zero), Some(65535));
            for invalid in ["", "-1", "65536", "1.5", "invalid"] {
                assert_eq!(parse_port(invalid, allow_zero), None);
            }
        }
    }
}
