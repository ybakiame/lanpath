use super::*;

impl DesktopView {
    pub(super) fn run_core_operation(
        &mut self,
        kind: CoreOperationKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let invalid_state = match kind {
            CoreOperationKind::Start => {
                self.core_running
                    || !self.configuration.read(cx).ready_for_start()
                    || self.structured.read(cx).dirty()
                    || self.rules.read(cx).dirty()
                    || self.scripts.read(cx).dirty()
                    || self.mitm.read(cx).dirty()
                    || self.requests.read(cx).dirty()
            }
            CoreOperationKind::Stop => !self.core_running,
        };
        if self.core_busy.is_some()
            || self.closing
            || self.proxy.read(cx).busy()
            || self.configuration.read(cx).busy()
            || self.subscriptions.read(cx).busy()
            || self.profiles.read(cx).busy()
            || self.structured.read(cx).busy()
            || self.rules.read(cx).busy()
            || self.scripts.read(cx).busy()
            || self.mitm.read(cx).busy()
            || self.requests.read(cx).busy()
            || self.cores.read(cx).busy()
            || self.diagnostics.read(cx).busy()
            || self.backups.read(cx).busy()
            || self.backups.read(cx).is_pending()
            || !self.core.is_configured()
            || invalid_state
        {
            return;
        }

        self.connections
            .update(cx, |view, cx| view.set_running(false, cx));
        self.proxy
            .update(cx, |view, cx| view.set_running(false, cx));
        self.core_busy = Some(kind);
        if matches!(kind, CoreOperationKind::Start) {
            self.startup_status = "正在开始启动".into();
        }
        self.sync_request_lock(cx);
        self.scripts
            .update(cx, |view, cx| view.set_running(false, cx));
        self.mitm.update(cx, |view, cx| {
            view.set_running(false, cx);
        });
        self.core_error = None;
        self.core_task = None;
        let service = self.core.clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            let (result, running) = cx
                .background_spawn(async move {
                    let result = match kind {
                        CoreOperationKind::Start => service.start(),
                        CoreOperationKind::Stop => service.stop(),
                    };
                    (result, service.running())
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    let result = result
                        .and_then(|()| running.as_ref().map(|_| ()).map_err(ToOwned::to_owned));
                    let succeeded = result.is_ok();
                    if let Ok(running) = running {
                        this.core_running = running;
                    }
                    if let Err(error) = result {
                        this.core_error = Some(error);
                    }
                    let children_running = this.core_running && !this.closing;
                    this.proxy
                        .update(cx, |view, cx| view.set_running(children_running, cx));
                    this.connections
                        .update(cx, |view, cx| view.set_running(children_running, cx));
                    this.stats
                        .update(cx, |view, cx| view.set_running(children_running, cx));
                    this.logs.update(cx, |view, cx| view.refresh(cx));
                    this.scripts.update(cx, |view, cx| {
                        view.set_running(children_running, cx);
                    });
                    this.mitm.update(cx, |view, cx| {
                        view.set_running(children_running, cx);
                    });
                    this.core_busy = None;
                    this.sync_request_lock(cx);
                    if succeeded && matches!(kind, CoreOperationKind::Stop) {
                        this.configuration
                            .update(cx, |view, cx| view.refresh(window, cx));
                    }
                    cx.notify();
                })
                .ok();
            });
        });
        self.core_task = Some(task);
        cx.notify();
    }

    pub(super) fn core_status(&self) -> String {
        if self.closing {
            return "正在等待已接受操作并回收实验核心…".to_string();
        }
        if let Some(kind) = self.core_busy {
            return match kind {
                CoreOperationKind::Start => self.startup_status.clone(),
                CoreOperationKind::Stop => "正在停止核心…".to_string(),
            };
        }
        if let Some(error) = &self.core_error {
            return format!("核心错误：{error}");
        }
        if self.core_running {
            return format!("核心运行中 · 127.0.0.1:{}", self.core.port());
        }
        if self.core.is_configured() {
            return format!("核心已停止 · 隔离端口 {}", self.core.port());
        }
        format!(
            "核心未配置 · {}",
            self.core.configuration_error().unwrap_or("不可用")
        )
    }
}
