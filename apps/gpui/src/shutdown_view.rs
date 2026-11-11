use super::*;

impl DesktopView {
    pub(crate) fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing || window.has_active_dialog(cx) {
            return;
        }
        self.begin_close(false, window, cx);
    }

    fn confirm_discard_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .title("放弃修改并关闭实验版？")
                .description(
                    "订阅、覆写、配置、脚本、MITM、网络或迁移来源有未保存草稿。关闭会丢弃草稿；已准备的迁移或回退事务及已保存数据会保留。",
                )
                .confirm()
                .ok_text("放弃修改并关闭")
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| view.begin_close(true, window, cx));
                    true
                })
        });
    }

    fn begin_close(&mut self, discard: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.stats.update(cx, |view, cx| view.set_closing(true, cx));
        self.logs.update(cx, |view, cx| view.set_closing(true, cx));
        self.requests
            .update(cx, |view, cx| view.set_closing(true, cx));
        self.system.update(cx, |view, cx| {
            view.set_closing(true, cx);
            view.set_running(false, cx);
        });
        self.appearance
            .update(cx, |view, cx| view.set_closing(true, cx));
        self.cores.update(cx, |view, cx| view.set_closing(true, cx));
        self.diagnostics
            .update(cx, |view, cx| view.set_closing(true, cx));

        self.backups
            .update(cx, |view, cx| view.set_closing(true, cx));
        self.distribution
            .update(cx, |view, cx| view.set_closing(true, cx));
        self.sync_request_lock(cx);
        self.scripts
            .update(cx, |view, cx| view.set_running(false, cx));
        self.mitm.update(cx, |view, cx| {
            view.set_running(false, cx);
        });
        self.proxy.update(cx, |view, cx| view.set_closing(true, cx));
        self.connections
            .update(cx, |view, cx| view.set_running(false, cx));
        self.stats
            .update(cx, |view, cx| view.set_running(false, cx));
        let core = self.core.clone();
        self.close_task = Some(cx.spawn_in(window, async move |this, cx| {
            // Automatic network work is cancelled by the root lock above, but
            // remains busy until its real worker exits. Manual accepted work
            // still completes before runtime and directory ownership are released.
            loop {
                let Ok(busy) = this.update(cx, |this, cx| {
                    this.core_busy.is_some()
                        || this.proxy.read(cx).busy()
                        || this.configuration.read(cx).busy()
                        || this.subscriptions.read(cx).busy()
                        || this.profiles.read(cx).busy()
                        || this.structured.read(cx).busy()
                        || this.rules.read(cx).busy()
                        || this.scripts.read(cx).busy()
                        || this.mitm.read(cx).busy()
                        || this.connections.read(cx).busy()
                        || this.stats.read(cx).busy()
                        || this.logs.read(cx).busy(cx)
                        || this.requests.read(cx).busy()
                        || this.system.read(cx).busy()
                        || this.cores.read(cx).busy()
                        || this.diagnostics.read(cx).busy()
                        || this.backups.read(cx).busy()
                        || this.distribution.read(cx).busy()
                        || this.restore_task.is_some()
                }) else {
                    return;
                };
                if !busy {
                    break;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(20))
                    .await;
            }
            let Ok(dirty) = this.update(cx, |this, cx| {
                !this.backups.read(cx).is_pending()
                    && !this.distribution.read(cx).is_pending()
                    && (this.subscriptions.read(cx).dirty()
                        || this.profiles.read(cx).dirty()
                        || this.structured.read(cx).dirty()
                        || this.rules.read(cx).dirty()
                        || this.scripts.read(cx).dirty()
                        || this.mitm.read(cx).dirty()
                        || this.requests.read(cx).dirty()
                        || this.system.read(cx).dirty()
                        || this.distribution.read(cx).dirty())
            }) else {
                return;
            };
            if dirty && !discard {
                let _ = cx.update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.closing = false;
                        this.stats
                            .update(cx, |view, cx| view.set_closing(false, cx));
                        this.logs.update(cx, |view, cx| view.set_closing(false, cx));
                        this.requests
                            .update(cx, |view, cx| view.set_closing(false, cx));
                        this.system.update(cx, |view, cx| {
                            view.set_closing(false, cx);
                            view.set_running(this.core_running, cx);
                        });
                        this.appearance
                            .update(cx, |view, cx| view.set_closing(false, cx));
                        this.cores
                            .update(cx, |view, cx| view.set_closing(false, cx));
                        this.diagnostics
                            .update(cx, |view, cx| view.set_closing(false, cx));

                        this.backups
                            .update(cx, |view, cx| view.set_closing(false, cx));
                        this.distribution
                            .update(cx, |view, cx| view.set_closing(false, cx));
                        this.sync_request_lock(cx);
                        this.configuration
                            .update(cx, |view, cx| view.set_locked(this.core_running, cx));
                        this.subscriptions
                            .update(cx, |view, cx| view.set_locked(this.core_running, cx));
                        this.profiles
                            .update(cx, |view, cx| view.set_locked(this.core_running, cx));
                        this.structured
                            .update(cx, |view, cx| view.set_locked(this.core_running, cx));
                        this.rules
                            .update(cx, |view, cx| view.set_locked(this.core_running, cx));
                        this.scripts.update(cx, |view, cx| {
                            view.set_locked(this.core_running, cx);
                            view.set_running(this.core_running, cx);
                        });
                        this.mitm.update(cx, |view, cx| {
                            view.set_locked(this.core_running, cx);
                            view.set_running(this.core_running, cx);
                        });
                        this.proxy
                            .update(cx, |view, cx| view.set_closing(false, cx));
                        this.connections
                            .update(cx, |view, cx| view.set_running(this.core_running, cx));
                        this.stats
                            .update(cx, |view, cx| view.set_running(this.core_running, cx));
                        this.confirm_discard_close(window, cx);
                        this.sync_request_lock(cx);
                        cx.notify();
                    })
                    .ok();
                });
                return;
            }
            let result = cx
                .background_spawn(async move {
                    if core.is_configured() {
                        core.stop().map(|_| ())
                    } else {
                        Ok(())
                    }
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| match result {
                    Ok(()) => window.remove_window(),
                    Err(error) => {
                        this.closing = false;
                        this.stats
                            .update(cx, |view, cx| view.set_closing(false, cx));
                        this.logs.update(cx, |view, cx| view.set_closing(false, cx));
                        this.requests
                            .update(cx, |view, cx| view.set_closing(false, cx));
                        this.system.update(cx, |view, cx| {
                            view.set_closing(false, cx);
                            view.set_running(this.core_running, cx);
                        });
                        this.appearance
                            .update(cx, |view, cx| view.set_closing(false, cx));
                        this.cores
                            .update(cx, |view, cx| view.set_closing(false, cx));
                        this.diagnostics
                            .update(cx, |view, cx| view.set_closing(false, cx));

                        this.backups
                            .update(cx, |view, cx| view.set_closing(false, cx));
                        this.distribution
                            .update(cx, |view, cx| view.set_closing(false, cx));
                        this.sync_request_lock(cx);
                        this.core_error = Some(error);
                        this.configuration
                            .update(cx, |view, cx| view.set_locked(this.core_running, cx));
                        this.subscriptions
                            .update(cx, |view, cx| view.set_locked(this.core_running, cx));
                        this.profiles
                            .update(cx, |view, cx| view.set_locked(this.core_running, cx));
                        this.structured
                            .update(cx, |view, cx| view.set_locked(this.core_running, cx));
                        this.rules
                            .update(cx, |view, cx| view.set_locked(this.core_running, cx));
                        this.scripts.update(cx, |view, cx| {
                            view.set_locked(this.core_running, cx);
                            view.set_running(this.core_running, cx);
                        });
                        this.mitm.update(cx, |view, cx| {
                            view.set_locked(this.core_running, cx);
                            view.set_running(this.core_running, cx);
                        });
                        this.proxy
                            .update(cx, |view, cx| view.set_closing(false, cx));
                        this.connections
                            .update(cx, |view, cx| view.set_running(this.core_running, cx));
                        this.stats
                            .update(cx, |view, cx| view.set_running(this.core_running, cx));
                        this.sync_request_lock(cx);
                        cx.notify();
                    }
                })
                .ok();
            });
        }));
        cx.notify();
    }
}
