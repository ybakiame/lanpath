mod operations;
mod render;

use std::sync::Arc;

use gpui_kit::{Context, EventEmitter, Task, Window};
use lanpath_client::core_management::{CoreManagementService, ManagedCore, RemoteChannel};

use crate::{runtime::DesktopRuntime, startup::StartupProgress};

pub(crate) struct CoreSelectionChanged;

/// Owns inventory snapshots, in-flight work, and the last actual startup attempt.
pub(crate) struct CoreManagementView {
    core: Arc<DesktopRuntime>,
    service: Option<CoreManagementService>,
    local: Vec<ManagedCore>,
    remote: Vec<RemoteChannel>,
    remote_checked: bool,
    locked: bool,
    closing: bool,
    working: bool,
    stage: Option<String>,
    error: Option<String>,
    startup: StartupProgress,
    task: Option<Task<()>>,
}

impl EventEmitter<CoreSelectionChanged> for CoreManagementView {}

impl CoreManagementView {
    pub(crate) fn new(core: Arc<DesktopRuntime>, _: &mut Window, _: &mut Context<Self>) -> Self {
        let loaded = core
            .business()
            .map_err(|error| error.to_string())
            .and_then(|business| {
                business
                    .client_config()
                    .map_err(|error| error.to_string())?;
                Ok(CoreManagementService::experimental(
                    business.data_dir().to_path_buf(),
                ))
            });
        let (service, local, error) = match loaded {
            Ok(service) => match service.list_local() {
                Ok(local) => (Some(service), local, None),
                Err(error) => (Some(service), Vec::new(), Some(error.to_string())),
            },
            Err(error) => (None, Vec::new(), Some(error)),
        };
        Self {
            core,
            service,
            local,
            remote: Vec::new(),
            remote_checked: false,
            locked: false,
            closing: false,
            working: false,
            stage: None,
            error,
            startup: StartupProgress::default(),
            task: None,
        }
    }

    pub(crate) fn busy(&self) -> bool {
        self.working
    }

    pub(crate) fn set_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        if self.locked != locked {
            self.locked = locked;
            cx.notify();
        }
    }

    pub(crate) fn set_closing(&mut self, closing: bool, cx: &mut Context<Self>) {
        if self.closing != closing {
            self.closing = closing;
            cx.notify();
        }
    }

    pub(crate) fn set_startup(&mut self, startup: StartupProgress, cx: &mut Context<Self>) {
        self.startup = startup;
        cx.notify();
    }

    fn actions_disabled(&self) -> bool {
        self.locked || self.closing || self.working || self.service.is_none()
    }

    fn selection_disabled(&self) -> bool {
        self.actions_disabled() || self.core.core_override().is_some()
    }
}
