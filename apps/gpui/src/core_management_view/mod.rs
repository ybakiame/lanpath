mod header;
mod operations;
mod render;
mod selection_card;

use std::sync::Arc;

use gpui_kit::{Context, EventEmitter, Task, Window};
use lanpath_client::{
    core_management::{CoreManagementService, ManagedCore, RemoteChannel},
    cores::{CoreChannel, channel_of_version},
};
use lanpath_common::CoreType;

use crate::runtime::DesktopRuntime;

pub(crate) struct CoreSelectionChanged;

/// Owns the selected inventory, the independent active core, and in-flight work.
pub(crate) struct CoreManagementView {
    core: Arc<DesktopRuntime>,
    service: Option<CoreManagementService>,
    selected_core: CoreType,
    selected_channel: CoreChannel,
    active_core: Option<ManagedCore>,
    local: Vec<ManagedCore>,
    remote: Vec<RemoteChannel>,
    remote_checked: bool,
    locked: bool,
    closing: bool,
    working: bool,
    stage: Option<String>,
    error: Option<String>,
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
        let (service, selected_core, active_core, local, error) = match loaded {
            Ok(service) => {
                let active = service.active_core().ok();
                let selected = active
                    .as_ref()
                    .map(ManagedCore::core_type)
                    .unwrap_or_default();
                match service.for_core(selected).list_local() {
                    Ok(local) => (Some(service), selected, active, local, None),
                    Err(error) => (
                        Some(service),
                        selected,
                        active,
                        Vec::new(),
                        Some(error.to_string()),
                    ),
                }
            }
            Err(error) => (None, CoreType::SingBox, None, Vec::new(), Some(error)),
        };
        let selected_channel = active_core
            .as_ref()
            .map(|core| supported_channel(channel_of_version(core.version())))
            .unwrap_or(CoreChannel::Stable);
        Self {
            core,
            service,
            selected_core,
            selected_channel,
            active_core,
            local,
            remote: Vec::new(),
            remote_checked: false,
            locked: false,
            closing: false,
            working: false,
            stage: None,
            error,
            task: None,
        }
    }

    /// Retained actual selection, independent of the inventory browsing type.
    pub(crate) fn selected_core_summary(&self) -> String {
        self.active_core
            .as_ref()
            .map(|core| format!("{} · {}", core.core_type(), core.version()))
            .unwrap_or_else(|| "尚未选择核心".into())
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

    fn actions_disabled(&self) -> bool {
        self.locked || self.closing || self.working || self.service.is_none()
    }

    fn selection_disabled(&self) -> bool {
        self.actions_disabled() || self.core.core_override().is_some()
    }
}

fn supported_channel(channel: CoreChannel) -> CoreChannel {
    match channel {
        CoreChannel::Stable => CoreChannel::Stable,
        CoreChannel::Beta | CoreChannel::Prerelease => CoreChannel::Prerelease,
    }
}
