mod operations;
mod render;

use std::sync::Arc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, EventEmitter, Subscription, Task, Window};

use crate::{
    distribution::{
        migration::{MigrationOptions, MigrationPreview, MigrationRecovery, SourceKind},
        updater::{AvailableUpdate, DownloadedUpdate, UpdateCheck},
    },
    runtime::DesktopRuntime,
};

pub(crate) enum DistributionEvent {
    RestartRequested,
}

enum Outcome {
    Preview(MigrationPreview),
    Prepared(String),
    State(MigrationRecovery),
    RollbackRequested,
    Checked(UpdateCheck),
    Downloaded(DownloadedUpdate),
    Opened,
}

pub(crate) struct DistributionView {
    core: Arc<DesktopRuntime>,
    source: Entity<InputState>,
    source_kind: SourceKind,
    options: MigrationOptions,
    preview: Option<MigrationPreview>,
    recovery: MigrationRecovery,
    pending: bool,
    draft: bool,
    locked: bool,
    closing: bool,
    working: Option<&'static str>,
    available: Option<AvailableUpdate>,
    downloaded: Option<DownloadedUpdate>,
    notice: Option<String>,
    error: Option<String>,
    task: Option<Task<()>>,
    _subscription: Subscription,
}

impl EventEmitter<DistributionEvent> for DistributionView {}

impl DistributionView {
    pub(crate) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let source = cx.new(|cx| {
            InputState::new(window, cx).placeholder("选择原 Tauri 或 LanpathExperimental 数据目录")
        });
        let subscription = cx.subscribe(&source, |this: &mut Self, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.changed(cx);
            }
        });
        let mut view = Self {
            core,
            source,
            source_kind: SourceKind::Tauri,
            options: MigrationOptions::default(),
            preview: None,
            recovery: MigrationRecovery::None,
            pending: false,
            draft: false,
            locked: false,
            closing: false,
            working: None,
            available: None,
            downloaded: None,
            notice: None,
            error: None,
            task: None,
            _subscription: subscription,
        };
        view.refresh(cx);
        view
    }

    pub(crate) fn busy(&self) -> bool {
        self.working.is_some()
    }
    pub(crate) fn dirty(&self) -> bool {
        self.draft
    }
    pub(crate) fn is_pending(&self) -> bool {
        self.pending
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
    pub(crate) fn restart_failed(&mut self, error: String, cx: &mut Context<Self>) {
        self.error = Some(error);
        cx.notify();
    }
    fn unavailable(&self) -> bool {
        self.locked || self.closing || self.busy()
    }
    fn editing_disabled(&self) -> bool {
        self.unavailable() || self.pending
    }
    fn sync_readonly(&self, cx: &mut Context<Self>) {
        self.source.update(cx, |state, cx| {
            state.set_readonly(self.editing_disabled(), cx)
        });
    }
    fn changed(&mut self, cx: &mut Context<Self>) {
        self.preview = None;
        self.draft = true;
        self.error = None;
        self.notice = None;
        cx.notify();
    }
    fn run(
        &mut self,
        label: &'static str,
        operation: impl FnOnce(Arc<DesktopRuntime>) -> Result<Outcome, String> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.unavailable() {
            return;
        }
        self.working = Some(label);
        self.error = None;
        self.notice = None;
        self.sync_readonly(cx);
        let core = self.core.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { operation(core) }).await;
            let _ = this.update(cx, |this, cx| {
                this.working = None;
                match result {
                    Err(error) => this.error = Some(error),
                    Ok(Outcome::Preview(preview)) => this.preview = Some(preview),
                    Ok(Outcome::Prepared(message)) => {
                        this.preview = None;
                        this.draft = false;
                        this.pending = true;
                        this.recovery = MigrationRecovery::Pending;
                        this.notice = Some(message);
                    }
                    Ok(Outcome::State(state)) => {
                        this.pending = matches!(
                            state,
                            MigrationRecovery::Pending | MigrationRecovery::RollbackPending
                        );
                        this.recovery = state;
                    }
                    Ok(Outcome::RollbackRequested) => {
                        this.pending = true;
                        this.draft = false;
                        this.preview = None;
                        this.recovery = MigrationRecovery::RollbackPending;
                        this.notice = Some(
                            "回退意图已持久保存；重启后在打开业务存储前恢复，不会在线替换数据。"
                                .into(),
                        );
                    }
                    Ok(Outcome::Checked(UpdateCheck::Current { version })) => {
                        this.available = None;
                        this.downloaded = None;
                        this.notice = Some(format!("签名更新源确认当前版本为 {version}"));
                    }
                    Ok(Outcome::Checked(UpdateCheck::Available(available))) => {
                        this.notice = Some(format!("发现已验证签名的更新 {}", available.version));
                        this.available = Some(available);
                        this.downloaded = None;
                    }
                    Ok(Outcome::Downloaded(downloaded)) => {
                        this.notice = Some(format!(
                            "版本 {} 已完成签名、长度和 SHA-256 校验",
                            downloaded.version()
                        ));
                        this.downloaded = Some(downloaded);
                    }
                    Ok(Outcome::Opened) => {}
                }
                this.sync_readonly(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }
}
