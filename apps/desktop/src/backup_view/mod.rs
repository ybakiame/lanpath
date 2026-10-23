mod files;
mod operations;
mod render;
#[cfg(test)]
mod tests;
mod transfer;

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, EventEmitter, Subscription, Task, Window};
use lanpath_client::backup::{BackupPreview, BackupService, BackupSummary};

use crate::runtime::DesktopRuntime;

pub(crate) enum BackupEvent {
    RestoreRequested { preview_id: String },
    RestartRequested,
    StartupNotice,
}

struct Source {
    label: String,
    encrypted: Arc<Vec<u8>>,
}

struct Inspected {
    preview: BackupPreview,
    source: String,
    inspected_at: Instant,
    expired: bool,
}

pub(crate) struct BackupView {
    _core: Arc<DesktopRuntime>,
    service: Option<Arc<BackupService>>,
    root: Option<PathBuf>,
    export_password: Entity<InputState>,
    import_password: Entity<InputState>,
    source: Option<Source>,
    inspected: Option<Inspected>,
    restored: Option<BackupSummary>,
    pending: bool,
    locked: bool,
    closing: bool,
    working: Option<&'static str>,
    notice: Option<String>,
    error: Option<String>,
    startup_error: Option<String>,
    startup_notice_sent: bool,
    task: Option<Task<()>>,
    expiry_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<BackupEvent> for BackupView {}

impl BackupView {
    pub(crate) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let service = core.backup().ok().cloned();
        let root = core
            .business()
            .ok()
            .map(|service| service.data_dir().to_path_buf());
        let export_password = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("生成或输入至少 12 个字符的密码")
        });
        let import_password = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("输入此备份的密码")
        });
        let subscription = cx.subscribe(&import_password, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.invalidate_preview();
                cx.notify();
            } else if matches!(event, InputEvent::PressEnter { .. }) {
                this.inspect(cx);
            }
        });
        let mut view = Self {
            error: service.is_none().then(|| {
                core.backup()
                    .err()
                    .unwrap_or_else(|| "备份服务不可用".into())
            }),
            _core: core,
            service,
            root,
            export_password,
            import_password,
            source: None,
            inspected: None,
            restored: None,
            pending: false,
            locked: false,
            closing: false,
            working: None,
            notice: None,
            startup_error: None,
            startup_notice_sent: false,
            task: None,
            expiry_task: None,
            _subscriptions: vec![subscription],
        };
        view.sync_readonly(cx);
        view.refresh(cx);
        view
    }

    pub(crate) fn busy(&self) -> bool {
        self.working.is_some()
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.pending
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

    pub(crate) fn restore_started(&mut self, cx: &mut Context<Self>) {
        self.inspected = None;
        self.expiry_task = None;
        self.begin("正在准备事务恢复；等待运行时停止与文件写入完成", cx);
    }

    pub(crate) fn restore_finished(
        &mut self,
        result: Result<String, String>,
        cx: &mut Context<Self>,
    ) {
        self.finish(cx);
        match result {
            Ok(message) => {
                self.pending = true;
                self.notice = Some(format!(
                    "恢复事务已准备；加密回滚备份：{message}。重启后应用；回滚使用本次导入密码。"
                ));
                self.error = None;
            }
            Err(error) => {
                self.error = Some(format!("恢复准备失败：{error}。请重新检查备份后再试。"))
            }
        }
        self.sync_readonly(cx);
        self.refresh(cx);
    }

    pub(crate) fn restart_failed(&mut self, error: String, cx: &mut Context<Self>) {
        self.error = Some(format!("重启失败：{error}。待恢复事务仍保留，可再次重启。"));
        cx.notify();
    }

    fn editing_disabled(&self) -> bool {
        self.locked || self.closing || self.busy() || self.pending || self.service.is_none()
    }

    fn sync_readonly(&self, cx: &mut Context<Self>) {
        let readonly = self.editing_disabled();
        for input in [&self.export_password, &self.import_password] {
            input.update(cx, |state, cx| state.set_readonly(readonly, cx));
        }
    }

    fn begin(&mut self, label: &'static str, cx: &mut Context<Self>) {
        self.working = Some(label);
        self.error = None;
        self.notice = None;
        self.sync_readonly(cx);
        cx.notify();
    }

    fn finish(&mut self, cx: &mut Context<Self>) {
        self.working = None;
        self.task = None;
        self.sync_readonly(cx);
        cx.notify();
    }

    fn invalidate_preview(&mut self) {
        if let Some(inspected) = self.inspected.take()
            && let Some(service) = &self.service
        {
            service.discard(&inspected.preview.preview_id);
            self.notice = None;
        }
        self.expiry_task = None;
    }

    fn preview_valid(&self) -> bool {
        self.inspected.as_ref().is_some_and(|inspected| {
            !inspected.expired && inspected.inspected_at.elapsed() < Duration::from_secs(300)
        })
    }
}
