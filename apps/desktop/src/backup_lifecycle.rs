use super::*;

impl DesktopView {
    fn backup_peer_busy(&self, cx: &gpui_kit::App) -> bool {
        self.core_busy.is_some()
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
    }

    fn backup_has_drafts(&self, cx: &gpui_kit::App) -> bool {
        self.subscriptions.read(cx).dirty()
            || self.profiles.read(cx).dirty()
            || self.structured.read(cx).dirty()
            || self.rules.read(cx).dirty()
            || self.scripts.read(cx).dirty()
            || self.mitm.read(cx).dirty()
            || self.requests.read(cx).dirty()
    }

    pub(super) fn request_restore(
        &mut self,
        preview_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.closing
            || self.restore_task.is_some()
            || self.backups.read(cx).busy()
            || self.backups.read(cx).is_pending()
            || self.backup_peer_busy(cx)
        {
            return;
        }
        let drafts = self.backup_has_drafts(cx);
        let description = if drafts {
            "将放弃订阅、覆写或设置中未保存的修改，停止实验核心，并准备替换当前实验配置。恢复前会保存同密码加密的回滚副本；随后须重启完成恢复。不会读取生产目录或开启系统代理、TUN。"
        } else {
            "将停止实验核心，并准备替换当前实验配置。恢复前会保存同密码加密的回滚副本；随后须重启完成恢复。不会读取生产目录或开启系统代理、TUN。"
        };
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            let id = preview_id.clone();
            dialog
                .title("恢复当前实验配置？")
                .description(description)
                .confirm()
                .ok_text("停止核心并准备恢复")
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| view.begin_restore(id.clone(), window, cx));
                    true
                })
        });
    }

    // Observation/query side effects also settle before snapshot preparation.
    // Other writers are locked through sync_request_lock while BackupView is busy.
    fn set_restore_guard(&mut self, quiescing: bool, cx: &mut Context<Self>) {
        self.stats
            .update(cx, |view, cx| view.set_closing(quiescing, cx));
        self.logs
            .update(cx, |view, cx| view.set_closing(quiescing, cx));
        self.about
            .update(cx, |view, cx| view.set_closing(quiescing, cx));
        let running = !quiescing && self.core_running;
        self.connections
            .update(cx, |view, cx| view.set_running(running, cx));
        self.proxy
            .update(cx, |view, cx| view.set_running(running, cx));
        self.stats
            .update(cx, |view, cx| view.set_running(running, cx));
        self.scripts
            .update(cx, |view, cx| view.set_running(running, cx));
        self.mitm
            .update(cx, |view, cx| view.set_running(running, cx));
        self.sync_request_lock(cx);
    }

    fn begin_restore(&mut self, preview_id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing
            || self.restore_task.is_some()
            || self.backups.read(cx).is_pending()
            || self.backups.read(cx).busy()
            || self.backup_peer_busy(cx)
        {
            return;
        }
        let appearance = cx
            .global::<appearance::AppearanceState>()
            .preferences()
            .backup_appearance();
        self.backups.update(cx, |view, cx| view.restore_started(cx));
        self.set_restore_guard(true, cx);
        let core = self.core.clone();
        self.restore_task = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                let Ok(busy) = this.update(cx, |this, cx| {
                    this.backup_peer_busy(cx)
                        || this.connections.read(cx).busy()
                        || this.stats.read(cx).busy()
                        || this.logs.read(cx).busy(cx)
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
            let (result, running) = cx
                .background_spawn(async move {
                    let result = core.prepare_restore(&preview_id, Some(appearance));
                    (result, core.running())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.restore_task = None;
                match running {
                    Ok(running) => this.core_running = running,
                    Err(error) => this.core_error = Some(error),
                }
                this.backups
                    .update(cx, |view, cx| view.restore_finished(result, cx));
                this.set_restore_guard(this.backups.read(cx).is_pending(), cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn restart_restored(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing
            || self.restore_task.is_some()
            || self.backups.read(cx).busy()
            || !self.backups.read(cx).is_pending()
        {
            return;
        }
        self.closing = true;
        self.backups
            .update(cx, |view, cx| view.set_closing(true, cx));
        self.set_restore_guard(true, cx);
        let core = self.core.clone();
        self.restore_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    // Restart also works after re-opening a prepared transaction.
                    core.stop()?;
                    restart::spawn_replacement()
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.restore_task = None;
                    match result {
                        Ok(()) => window.remove_window(),
                        Err(error) => {
                            this.closing = false;
                            this.backups.update(cx, |view, cx| {
                                view.set_closing(false, cx);
                                view.restart_failed(error, cx);
                            });
                            this.sync_request_lock(cx);
                        }
                    }
                })
                .ok();
            });
        }));
        cx.notify();
    }
}
