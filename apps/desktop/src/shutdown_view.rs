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
                    "订阅、覆写、结构化配置、规则、脚本、MITM 或网络请求有未保存修改。关闭会丢弃这些修改；已保存数据不会删除。",
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
        self.appearance
            .update(cx, |view, cx| view.set_closing(true, cx));
        self.configuration
            .update(cx, |view, cx| view.set_locked(true, cx));
        self.subscriptions
            .update(cx, |view, cx| view.set_locked(true, cx));
        self.profiles
            .update(cx, |view, cx| view.set_locked(true, cx));
        self.structured
            .update(cx, |view, cx| view.set_locked(true, cx));
        self.rules.update(cx, |view, cx| view.set_locked(true, cx));
        self.scripts
            .update(cx, |view, cx| view.set_locked(true, cx));
        self.scripts
            .update(cx, |view, cx| view.set_running(false, cx));
        self.mitm.update(cx, |view, cx| {
            view.set_locked(true, cx);
            view.set_running(false, cx);
        });
        self.proxy.update(cx, |view, cx| view.set_closing(true, cx));
        self.connections
            .update(cx, |view, cx| view.set_running(false, cx));
        self.stats
            .update(cx, |view, cx| view.set_running(false, cx));
        let core = self.core.clone();
        self.close_task = Some(cx.spawn_in(window, async move |this, cx| {
            // Finish accepted config/check/save and core/proxy operations before
            // checking draft state or releasing runtime and directory ownership.
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
                this.subscriptions.read(cx).dirty()
                    || this.profiles.read(cx).dirty()
                    || this.structured.read(cx).dirty()
                    || this.rules.read(cx).dirty()
                    || this.scripts.read(cx).dirty()
                    || this.mitm.read(cx).dirty()
                    || this.requests.read(cx).dirty()
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
                        this.appearance
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
                        cx.notify();
                    })
                    .ok();
                });
                return;
            }
            let result = cx
                .background_spawn(async move {
                    if core.is_configured() {
                        core.stop()
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
                        this.appearance
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
                        cx.notify();
                    }
                })
                .ok();
            });
        }));
        cx.notify();
    }
}
