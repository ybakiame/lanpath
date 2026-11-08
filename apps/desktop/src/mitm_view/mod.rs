mod certificate;
mod operations;
mod render;
mod snapshot;
mod traffic_list;

use std::{sync::Arc, time::Duration};

use gpui_kit::component::{
    input::{EditorState, InputEvent},
    list::ListState,
};
use gpui_kit::{AppContext as _, Context, Entity, EventEmitter, Subscription, Task, Window};
use lanpath_client::{
    ScriptDialect,
    mitm::{CaOwnedTrustView, DerivedHostnameView, MitmCaView},
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
    RemoveCa,
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
    trust: Option<CaOwnedTrustView>,
    mitm_addr: Option<std::net::SocketAddr>,
    locked: bool,
    active: bool,
    running: bool,
    operation: Option<Operation>,
    error: Option<String>,
    notice: Option<String>,
    sync_error: Option<String>,
    refresh_requested: bool,
    section: usize,
    ca_details_open: bool,
    manual_open: bool,
    operation_task: Option<Task<()>>,
    runtime_task: Option<Task<()>>,
    runtime_generation: u64,
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
        let traffic =
            cx.new(|cx| crate::keyboard::list_state(TrafficListDelegate::default(), window, cx));
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
        let poll_task = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                if cx
                    .update(|window, cx| {
                        this.update(cx, |this, cx| {
                            if this.active {
                                if this.running && this.section == 1 {
                                    this.refresh_traffic(cx);
                                }
                                if this.refresh_requested {
                                    this.refresh_metadata(window, cx);
                                }
                            }
                        })
                        .is_err()
                    })
                    .unwrap_or(true)
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
            sync_error: None,
            refresh_requested: false,
            section: 0,
            ca_details_open: false,
            manual_open: false,
            operation_task: None,
            runtime_task: None,
            runtime_generation: 0,
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

    pub(crate) fn set_active(&mut self, active: bool, _cx: &mut Context<Self>) {
        let entering = active && !self.active;
        self.active = active;
        if entering {
            self.refresh_requested = true;
        }
    }

    pub(crate) fn set_running(&mut self, running: bool, cx: &mut Context<Self>) {
        if self.running == running {
            return;
        }
        self.notice = None;
        self.runtime_generation += 1;
        self.runtime_task = None;
        self.running = running;
        if !running {
            self.mitm_addr = None;
        } else if self.active {
            self.refresh_runtime(cx);
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
            Ok(records) => {
                let changed = self.traffic.update(cx, |state, cx| {
                    let changed = state.delegate_mut().replace(records);
                    if changed {
                        cx.notify();
                    }
                    changed
                });
                if changed {
                    cx.notify();
                }
            }
            Err(error) if self.error.is_none() => {
                self.error = Some(error);
                cx.notify();
            }
            Err(_) => {}
        }
    }

    fn refresh_runtime(&mut self, cx: &mut Context<Self>) {
        let core = self.core.clone();
        let generation = self.runtime_generation;
        self.runtime_task = Some(cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { core.mitm_addr() }).await;
            let _ = this.update(cx, |this, cx| {
                if this.runtime_generation != generation {
                    return;
                }
                this.runtime_task = None;
                match result {
                    Ok(addr) => this.mitm_addr = addr,
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }));
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
