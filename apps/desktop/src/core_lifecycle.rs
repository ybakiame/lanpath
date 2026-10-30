use super::*;
use crate::privileged::StopKind;

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
                    || self.system.read(cx).dirty()
                    || self.distribution.read(cx).dirty()
                    || self.distribution.read(cx).is_pending()
                    || self.core_recovery_needed
            }
            CoreOperationKind::Stop => !self.core_running && !self.core_recovery_needed,
        };
        if self.core_busy.is_some()
            || self.closing
            || self.proxy.read(cx).busy()
            || self.configuration.read(cx).busy()
            || self.subscriptions.read(cx).busy()
            || self.profiles.read(cx).busy()
            || self.structured.read(cx).busy()
            || self.rules.read(cx).busy()
            || self.scripts.read(cx).lifecycle_busy()
            || self.mitm.read(cx).busy()
            || self.requests.read(cx).busy()
            || self.system.read(cx).busy()
            || self.cores.read(cx).busy()
            || self.diagnostics.read(cx).busy()
            || self.backups.read(cx).busy()
            || self.backups.read(cx).is_pending()
            || self.distribution.read(cx).busy()
            || !self.core.is_configured()
            || invalid_state
        {
            return;
        }

        self.connections
            .update(cx, |view, cx| view.set_running(false, cx));
        self.proxy
            .update(cx, |view, cx| view.set_running(false, cx));
        self.system
            .update(cx, |view, cx| view.set_running(false, cx));
        self.core_busy = Some(kind);
        self.core_generation = self.core_generation.wrapping_add(1);
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
        self.core_stop_kind = None;
        self.core_task = None;
        let service = self.core.clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            // The root lock has requested automatic cancellation. Wait for its
            // real worker to release the publication/lifecycle lease before
            // entering a synchronous start/stop operation.
            loop {
                let Ok(busy) = this.update(cx, |this, cx| this.scripts.read(cx).busy()) else {
                    return;
                };
                if !busy {
                    break;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(20))
                    .await;
            }
            let (result, running, recovery) = cx
                .background_spawn(async move {
                    let result = match kind {
                        CoreOperationKind::Start => service.start().map(|()| None),
                        CoreOperationKind::Stop => service.stop(),
                    };
                    (
                        result,
                        service.running(),
                        service.network_recovery_required(),
                    )
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    let result = result.and_then(|stopped| {
                        running.as_ref().map(|_| stopped).map_err(ToOwned::to_owned)
                    });
                    let succeeded = result.is_ok();
                    if let Ok(running) = running {
                        this.core_running = running;
                    }
                    match recovery {
                        Ok(required) => this.core_recovery_needed = required,
                        Err(error) => this.core_error = Some(error),
                    }
                    match result {
                        Ok(stopped) => this.core_stop_kind = stopped,
                        Err(error) => this.core_error = Some(error),
                    }
                    this.publish_running(cx);
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
            if self.core_recovery_needed {
                return "核心状态待恢复；启动、配置写入和安装更新已锁定".into();
            }
            let stopped = match self.core_stop_kind.as_ref() {
                Some(StopKind::Graceful) => " · 正常退出，自有网络已恢复",
                Some(StopKind::Forced) => " · 强制退出，自有网络已恢复",
                Some(StopKind::AlreadyStopped) => " · 原已退出，自有网络已恢复",
                Some(StopKind::RecoveryRequired) => " · 自有网络仍待恢复",
                None => "",
            };
            return format!("核心已停止{stopped} · 隔离端口 {}", self.core.port());
        }
        format!(
            "核心未配置 · {}",
            self.core.configuration_error().unwrap_or("不可用")
        )
    }

    fn publish_running(&mut self, cx: &mut Context<Self>) {
        let running = self.core_running && !self.closing;
        self.proxy
            .update(cx, |view, cx| view.set_running(running, cx));
        self.connections
            .update(cx, |view, cx| view.set_running(running, cx));
        self.system
            .update(cx, |view, cx| view.set_running(running, cx));
        self.stats
            .update(cx, |view, cx| view.set_running(running, cx));
        self.scripts
            .update(cx, |view, cx| view.set_running(running, cx));
        self.mitm
            .update(cx, |view, cx| view.set_running(running, cx));
        self.logs.update(cx, |view, cx| view.refresh(cx));
    }

    pub(super) fn start_native_poll(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
                let Ok((skip, was_running, generation)) = this.update(cx, |this, _| {
                    (
                        this.closing || this.core_busy.is_some(),
                        this.core_running,
                        this.core_generation,
                    )
                }) else {
                    break;
                };
                if skip {
                    continue;
                }
                let core = core.clone();
                let (activation, status) = cx
                    .background_spawn(async move {
                        (core.poll_activation(), core.poll_lifecycle(was_running))
                    })
                    .await;
                if cx
                    .update(|window, cx| {
                        if matches!(activation, Ok(true)) {
                            window.activate_window();
                        }
                        let _ = this.update(cx, |this, cx| {
                            if this.closing
                                || this.core_busy.is_some()
                                || this.core_generation != generation
                            {
                                return;
                            }
                            match status {
                                Ok(status) => {
                                    let changed = this.core_running != status.running;
                                    this.core_running = status.running;
                                    this.core_recovery_needed = status.recovery_required;
                                    if let Some(detail) = status.detail {
                                        this.core_error = Some(detail);
                                    }
                                    if changed {
                                        this.publish_running(cx);
                                    }
                                }
                                Err(error) => this.core_error = Some(error),
                            }
                            if let Err(error) = activation {
                                this.core_error = Some(error);
                            }
                            this.sync_request_lock(cx);
                        });
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
    }
}
