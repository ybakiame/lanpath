mod operations;
mod render;
mod traffic_list;

use std::{sync::Arc, time::Duration};

use gpui_kit::component::{
    input::{EditorState, InputEvent},
    list::ListState,
};
use gpui_kit::{AppContext as _, Context, Entity, EventEmitter, Subscription, Task, Window};
use lanpath_client::{
    ScriptDialect,
    mitm::{CaTrustView, DerivedHostnameView, MitmCaView},
};

use crate::{
    business::{BusinessService, MitmSettingsDocument},
    runtime::DesktopRuntime,
};
use traffic_list::TrafficListDelegate;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Operation {
    Save,
    Refresh,
    InstallCa,
}

pub(crate) struct MitmChanged;

pub(crate) struct MitmView {
    core: Arc<DesktopRuntime>,
    service: Option<BusinessService>,
    hostnames: Entity<EditorState>,
    traffic: Entity<ListState<TrafficListDelegate>>,
    enabled: bool,
    script_dialect: ScriptDialect,
    dirty: bool,
    syncing: bool,
    derived: Vec<DerivedHostnameView>,
    ca: Option<MitmCaView>,
    trust: Option<CaTrustView>,
    mitm_addr: Option<std::net::SocketAddr>,
    locked: bool,
    active: bool,
    running: bool,
    operation: Option<Operation>,
    error: Option<String>,
    notice: Option<String>,
    operation_task: Option<Task<()>>,
    _poll_task: Task<()>,
    _input_subscription: Subscription,
}

impl EventEmitter<MitmChanged> for MitmView {}

impl MitmView {
    pub(crate) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let service = core.business().ok().cloned();
        let loaded = service
            .as_ref()
            .ok_or_else(|| "实验数据目录不可用".to_string())
            .and_then(|service| service.mitm_settings().map_err(|error| error.to_string()));
        let (settings, error) = match loaded {
            Ok(settings) => (Some(settings), None),
            Err(error) => (None, Some(error)),
        };
        let hostnames = cx.new(|cx| EditorState::new(window, cx).line_number(false));
        let traffic = cx.new(|cx| ListState::new(TrafficListDelegate::default(), window, cx));
        let initial = settings
            .as_ref()
            .map(|settings| settings.hostnames.join("\n"))
            .unwrap_or_default();
        hostnames.update(cx, |state, cx| state.set_value(initial, window, cx));
        let input_subscription = cx.subscribe(&hostnames, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) && !this.syncing {
                this.dirty = true;
                cx.notify();
            }
        });
        let poll_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                if this
                    .update(cx, |this, cx| {
                        if this.active {
                            this.refresh_traffic(cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let settings = settings.unwrap_or_else(default_settings);
        Self {
            core,
            service,
            hostnames,
            traffic,
            enabled: settings.enabled,
            script_dialect: settings.script_dialect,
            dirty: false,
            syncing: false,
            ca: None,
            derived: settings.whitelist.derived,
            trust: None,
            mitm_addr: None,
            locked: false,
            active: false,
            running: false,
            operation: None,
            error,
            notice: None,
            operation_task: None,
            _poll_task: poll_task,
            _input_subscription: input_subscription,
        }
    }

    pub(crate) fn dirty(&self) -> bool {
        self.dirty
    }

    pub(crate) fn busy(&self) -> bool {
        self.operation.is_some()
    }

    pub(crate) fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.active = active;
        if active {
            self.refresh_metadata(cx);
            self.refresh_traffic(cx);
        }
    }

    pub(crate) fn set_running(&mut self, running: bool, cx: &mut Context<Self>) {
        self.running = running;
        if running && self.active {
            self.refresh_metadata(cx);
        } else if !running {
            self.mitm_addr = None;
        }
        cx.notify();
    }

    pub(crate) fn set_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        self.locked = locked;
        self.hostnames
            .update(cx, |state, cx| state.set_readonly(locked, cx));
        cx.notify();
    }

    fn refresh_traffic(&mut self, cx: &mut Context<Self>) {
        match self.core.mitm_traffic() {
            Ok(records) => self.traffic.update(cx, |state, cx| {
                if state.delegate_mut().replace(records) {
                    cx.notify();
                }
            }),
            Err(error) if self.error.is_none() => self.error = Some(error),
            Err(_) => {}
        }
        cx.notify();
    }
}

fn default_settings() -> MitmSettingsDocument {
    MitmSettingsDocument {
        enabled: false,
        hostnames: Vec::new(),
        script_dialect: ScriptDialect::Surge,
        whitelist: Default::default(),
    }
}
