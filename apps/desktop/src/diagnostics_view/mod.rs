mod operations;
mod render;

use std::sync::Arc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Task, Window};
use lanpath_client::diagnose::DiagReport;

use crate::runtime::DesktopRuntime;

struct CompletedReport {
    sequence: u64,
    report: DiagReport,
}

pub(crate) struct DiagnosticsView {
    core: Arc<DesktopRuntime>,
    domain: Entity<InputState>,
    locked: bool,
    closing: bool,
    sequence: u64,
    pending_target: Option<String>,
    completed: Option<CompletedReport>,
    error: Option<String>,
    task: Option<Task<()>>,
    _input_subscription: Subscription,
}

impl DiagnosticsView {
    pub(crate) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let domain = cx.new(|cx| InputState::new(window, cx).placeholder("google.com"));
        let subscription = cx.subscribe(&domain, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.run(cx);
            }
        });
        Self {
            core,
            domain,
            locked: false,
            closing: false,
            sequence: 0,
            pending_target: None,
            completed: None,
            error: None,
            task: None,
            _input_subscription: subscription,
        }
    }

    pub(crate) fn busy(&self) -> bool {
        self.pending_target.is_some()
    }

    pub(crate) fn set_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        if self.locked != locked {
            self.locked = locked;
            self.sync_readonly(cx);
            cx.notify();
        }
    }

    pub(crate) fn set_closing(&mut self, closing: bool, cx: &mut Context<Self>) {
        if self.closing != closing {
            self.closing = closing;
            self.sync_readonly(cx);
            cx.notify();
        }
    }

    fn editing_disabled(&self) -> bool {
        self.locked || self.closing || self.busy()
    }

    fn sync_readonly(&self, cx: &mut Context<Self>) {
        self.domain.update(cx, |input, cx| {
            input.set_readonly(self.editing_disabled(), cx)
        });
    }
}
