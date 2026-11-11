mod automatic;
mod editing;
mod inputs;
mod interval;
mod local;
mod operations;
mod render;
mod render_remotes;
mod render_tasks;
mod resource_dialog;
mod resource_form;

use std::{sync::Arc, time::Duration};

use crate::{business::BusinessService, runtime::DesktopRuntime, scripts_view::inputs::FormInputs};
use gpui_kit::{AppContext as _, Context, EventEmitter, Subscription, Task, Window};
use lanpath_client::{
    ArgSpec,
    remote::{FetchReport, RemoteKind, RemoteResource, RemoteTaskView, ScriptDialect},
};

const DIALECT_SCOPE_NOTICE: &str =
    "兼容范围：Surge 为主、QX 为辅；Loon / Stash 暂不可选，不代表完整厂商客户端支持。";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Section {
    Remotes,
    Tasks,
    Local,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Operation {
    Save,
    Delete,
    Toggle,
    Detect,
    Fetch,
    AutomaticFetch,
    Local,
    RunTask,
}

pub(crate) struct AutomaticRefreshPoll;

pub(crate) struct ScriptsView {
    core: Arc<DesktopRuntime>,
    service: Option<BusinessService>,
    resources: Vec<RemoteResource>,
    selected: Option<String>,
    creating: bool,
    resource_form_open: bool,
    resource_confirm_discard: bool,
    kind: RemoteKind,
    dialect: ScriptDialect,
    local: local::LocalState,
    arguments: Vec<ArgSpec>,
    enabled: bool,
    section: Section,
    inputs: FormInputs,
    syncing: bool,
    resource_dirty: bool,
    locked: bool,
    running: bool,
    active: bool,
    operation: Option<Operation>,
    automatic_eligible: bool,
    automatic_cancel: Option<tokio::sync::oneshot::Sender<()>>,
    error: Option<String>,
    notice: Option<String>,
    fetch_report: Option<FetchReport>,
    tasks: Vec<RemoteTaskView>,
    task_output: Option<(String, String)>,
    generation: u64,
    task: Option<Task<()>>,
    task_status_busy: bool,
    task_status_task: Option<Task<()>>,
    _poll_task: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<AutomaticRefreshPoll> for ScriptsView {}

impl ScriptsView {
    pub(crate) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let service = core.business().ok().cloned();
        let resources = service
            .as_ref()
            .and_then(|service| service.remote_resources().ok())
            .unwrap_or_default();
        let load_error = if service.is_none() {
            Some("实验数据目录不可用".to_string())
        } else {
            None
        };
        let (inputs, mut subscriptions) = FormInputs::new(window, cx);
        let (local, local_subscriptions) = local::LocalState::new(service.as_ref(), window, cx);
        subscriptions.extend(local_subscriptions);
        let poll_task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(5)).await;
                if this
                    .update(cx, |this, cx| {
                        if this.active && this.running {
                            this.load_tasks(cx);
                        }
                        cx.emit(AutomaticRefreshPoll);
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let selected = resources.first().map(|resource| resource.name.clone());
        let mut view = Self {
            core,
            service,
            resources,
            selected,
            creating: false,
            resource_form_open: false,
            resource_confirm_discard: false,
            local,
            kind: RemoteKind::Script,
            dialect: ScriptDialect::Surge,
            arguments: Vec::new(),
            enabled: true,
            section: Section::Remotes,
            inputs,
            syncing: false,
            resource_dirty: false,
            locked: false,
            running: false,
            active: false,
            operation: None,
            automatic_eligible: false,
            automatic_cancel: None,
            error: load_error,
            notice: None,
            fetch_report: None,
            tasks: Vec::new(),
            task_output: None,
            generation: 0,
            task: None,
            task_status_busy: false,
            task_status_task: None,
            _poll_task: poll_task,
            _subscriptions: subscriptions,
        };
        if view.selected.is_none() && view.service.is_some() {
            view.creating = true;
        }
        view.sync_resource(window, cx);
        view
    }

    pub(crate) fn save_from_shortcut(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.local.open {
            self.save_local(window, cx);
        } else if self.section == Section::Remotes && self.resource_form_open {
            self.save_resource(window, cx);
        }
    }

    pub(crate) fn dirty(&self) -> bool {
        self.resource_dirty || self.local.dirty
    }

    pub(crate) fn busy(&self) -> bool {
        self.operation.is_some()
    }

    pub(crate) fn lifecycle_busy(&self) -> bool {
        self.operation
            .is_some_and(|operation| operation != Operation::AutomaticFetch)
    }

    pub(crate) fn set_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        self.locked = locked;
        // Editing remains locked while running; background-cache eligibility is
        // independently owned by the root and must not cancel itself here.
        self.inputs
            .set_locked(locked, self.selected.is_some() && !self.creating, cx);
        cx.notify();
    }

    pub(crate) fn set_running(&mut self, running: bool, cx: &mut Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        self.running = running;
        if !running {
            self.cancel_automatic_refresh();
        }
        self.task_status_busy = false;
        self.tasks.clear();
        self.task_output = None;
        if running {
            self.load_tasks(cx);
        }
        cx.notify();
    }

    pub(crate) fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.active = active;
        if active && self.running && self.operation.is_none() {
            self.load_tasks(cx);
        }
        cx.notify();
    }

    fn set_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        if section == Section::Tasks && self.running && self.operation.is_none() {
            self.load_tasks(cx);
        }
        cx.notify();
    }

    fn selected_resource(&self) -> Option<&RemoteResource> {
        let selected = self.selected.as_deref()?;
        self.resources
            .iter()
            .find(|resource| resource.name == selected)
    }

    fn sync_resource(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.selected_resource().cloned();
        self.syncing = true;
        set_input(
            &self.inputs.name,
            selected
                .as_ref()
                .map(|resource| resource.name.as_str())
                .unwrap_or(""),
            window,
            cx,
        );
        set_input(
            &self.inputs.description,
            selected
                .as_ref()
                .and_then(|resource| resource.description.as_deref())
                .unwrap_or(""),
            window,
            cx,
        );
        set_input(
            &self.inputs.url,
            selected
                .as_ref()
                .map(|resource| resource.url.as_str())
                .unwrap_or(""),
            window,
            cx,
        );
        let seconds = selected
            .as_ref()
            .map(|resource| resource.update_interval_secs)
            .filter(|value| *value > 0)
            .unwrap_or(86_400);
        let (value, unit) = interval::split(seconds);
        set_input(&self.inputs.interval, &value.to_string(), window, cx);
        self.inputs
            .interval_unit
            .update(cx, |state, cx| state.set_selected_value(&unit, window, cx));
        let argument_values = selected
            .as_ref()
            .map(|resource| {
                resource
                    .argument_values
                    .iter()
                    .map(|(key, value)| format!("{key}={value}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        self.inputs
            .arguments
            .update(cx, |state, cx| state.set_value(argument_values, window, cx));
        self.kind = selected
            .as_ref()
            .map(|resource| resource.kind)
            .unwrap_or(RemoteKind::Script);
        self.dialect = selected
            .as_ref()
            .map(|resource| resource.dialect)
            .unwrap_or(ScriptDialect::Surge);
        self.arguments = selected
            .as_ref()
            .map(|resource| resource.arguments.clone())
            .unwrap_or_default();
        set_input(
            &self.inputs.icon,
            selected
                .as_ref()
                .and_then(|resource| resource.icon.as_deref())
                .unwrap_or(""),
            window,
            cx,
        );
        self.enabled = selected
            .as_ref()
            .map(|resource| resource.enabled)
            .unwrap_or(true);
        self.syncing = false;
        self.resource_dirty = false;
        self.inputs
            .set_locked(self.locked, self.selected.is_some() && !self.creating, cx);
        cx.notify();
    }

    fn reload_resources(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self
            .service
            .as_ref()
            .ok_or_else(|| "实验数据目录不可用".to_string())
            .and_then(|service| {
                service
                    .remote_resources()
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(resources) => {
                self.resources = resources;
                self.selected = self
                    .selected
                    .take()
                    .filter(|name| self.resources.iter().any(|resource| &resource.name == name));
                if self.selected.is_none() {
                    self.selected = self.resources.first().map(|resource| resource.name.clone());
                }
                self.creating = self.resources.is_empty();
                self.sync_resource(window, cx);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }

    fn resource_changed(&mut self, cx: &mut Context<Self>) {
        self.resource_dirty = true;
        self.error = None;
        self.notice = None;
        self.fetch_report = None;
        cx.notify();
    }

    fn load_tasks(&mut self, cx: &mut Context<Self>) {
        if !self.running || self.task_status_busy || self.operation == Some(Operation::RunTask) {
            return;
        }
        self.task_status_busy = true;
        let core = self.core.clone();
        let generation = self.generation;
        self.task_status_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { core.script_tasks() })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.task_status_busy = false;
                match result {
                    Ok(tasks) => {
                        this.tasks = tasks;
                        if this
                            .error
                            .as_deref()
                            .is_some_and(|error| error.starts_with("读取定时任务失败"))
                        {
                            this.error = None;
                        }
                    }
                    Err(error) => {
                        this.error = Some(format!("读取定时任务失败：{error}"));
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

fn set_input(
    input: &gpui_kit::Entity<gpui_kit::component::input::InputState>,
    value: &str,
    window: &mut Window,
    cx: &mut Context<ScriptsView>,
) {
    input.update(cx, |state, cx| {
        state.set_value(value.to_string(), window, cx)
    });
}
