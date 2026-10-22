mod operations;
mod render;

use std::sync::Arc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Task, Window};
use lanpath_client::RequestProxyMode;
use lanpath_common::LanpathResult;

use crate::{
    business::{BusinessService, RequestSettingsDocument},
    runtime::DesktopRuntime,
};

struct ProbeResult {
    sequence: u64,
    settings: RequestSettingsDocument,
    result: LanpathResult<u128>,
}

pub(crate) struct RequestSettingsView {
    core: Arc<DesktopRuntime>,
    service: Option<BusinessService>,
    custom_url: Entity<InputState>,
    github_prefix: Entity<InputState>,
    mode: RequestProxyMode,
    saved: Option<RequestSettingsDocument>,
    dirty: bool,
    syncing: bool,
    locked: bool,
    closing: bool,
    probing: bool,
    probe_sequence: u64,
    probe_result: Option<ProbeResult>,
    error: Option<String>,
    notice: Option<String>,
    probe_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl RequestSettingsView {
    pub(crate) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let service = core.business().ok().cloned();
        let loaded = service
            .as_ref()
            .ok_or_else(|| "实验数据目录不可用".to_string())
            .and_then(|service| {
                service
                    .request_settings()
                    .map_err(|error| error.to_string())
            });
        let (saved, error) = match loaded {
            Ok(settings) => (Some(settings), None),
            Err(error) => (None, Some(error)),
        };
        let custom_url = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("http://user:password@127.0.0.1:8080 或 socks5://…")
                .masked(true)
        });
        let github_prefix =
            cx.new(|cx| InputState::new(window, cx).placeholder("留空表示直接访问 GitHub"));
        if let Some(settings) = &saved {
            custom_url.update(cx, |state, cx| {
                state.set_value(settings.custom_url(), window, cx)
            });
            github_prefix.update(cx, |state, cx| {
                state.set_value(settings.github_prefix(), window, cx)
            });
        }
        let mut subscriptions = Vec::new();
        for input in [&custom_url, &github_prefix] {
            subscriptions.push(cx.subscribe(input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) && !this.syncing {
                    this.draft_changed(cx);
                }
            }));
        }
        let view = Self {
            core,
            service,
            mode: saved
                .as_ref()
                .map_or(RequestProxyMode::None, RequestSettingsDocument::mode),
            custom_url,
            github_prefix,
            saved,
            dirty: false,
            syncing: false,
            locked: false,
            closing: false,
            probing: false,
            probe_sequence: 0,
            probe_result: None,
            error,
            notice: None,
            probe_task: None,
            _subscriptions: subscriptions,
        };
        view.sync_readonly(cx);
        view
    }

    pub(crate) fn busy(&self) -> bool {
        self.probing
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
        let readonly = self.editing_disabled();
        self.custom_url.update(cx, |state, cx| {
            state.set_readonly(readonly || self.mode != RequestProxyMode::Custom, cx)
        });
        self.github_prefix
            .update(cx, |state, cx| state.set_readonly(readonly, cx));
    }

    fn draft(&self, cx: &Context<Self>) -> RequestSettingsDocument {
        RequestSettingsDocument::new(
            self.mode,
            self.custom_url.read(cx).value().to_string(),
            self.github_prefix.read(cx).value().to_string(),
        )
    }

    fn draft_changed(&mut self, cx: &mut Context<Self>) {
        self.dirty = self.saved.as_ref() != Some(&self.draft(cx));
        self.notice = None;
        cx.notify();
    }

    fn apply_saved(
        &mut self,
        settings: RequestSettingsDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.syncing = true;
        self.custom_url.update(cx, |state, cx| {
            state.set_value(settings.custom_url(), window, cx)
        });
        self.github_prefix.update(cx, |state, cx| {
            state.set_value(settings.github_prefix(), window, cx)
        });
        self.syncing = false;
        self.mode = settings.mode();
        self.saved = Some(settings);
        self.dirty = false;
        self.sync_readonly(cx);
    }
}

fn mode_label(mode: RequestProxyMode) -> &'static str {
    match mode {
        RequestProxyMode::None => "不使用代理",
        RequestProxyMode::System => "系统代理",
        RequestProxyMode::Core => "核心代理",
        RequestProxyMode::Custom => "自定义代理（地址和认证信息已隐藏）",
    }
}
