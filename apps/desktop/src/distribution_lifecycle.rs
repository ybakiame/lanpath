use super::*;

impl DesktopView {
    pub(super) fn restart_distribution(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing
            || self.restore_task.is_some()
            || self.core_busy.is_some()
            || self.distribution.read(cx).busy()
            || !self.distribution.read(cx).is_pending()
        {
            return;
        }
        self.closing = true;
        self.distribution
            .update(cx, |view, cx| view.set_closing(true, cx));
        self.sync_request_lock(cx);
        let core = self.core.clone();
        self.restore_task = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                let Ok(busy) = this.update(cx, |this, cx| {
                    this.core_busy.is_some()
                        || this.connections.read(cx).busy()
                        || this.stats.read(cx).busy()
                        || this.logs.read(cx).busy(cx)
                        || this.backups.read(cx).busy()
                        || this.distribution.read(cx).busy()
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
            let result = cx
                .background_spawn(async move {
                    let _ = core.stop()?;
                    restart::spawn_replacement()
                })
                .await;
            let _ = cx.update(|window, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.restore_task = None;
                    match result {
                        Ok(()) => window.remove_window(),
                        Err(error) => {
                            this.closing = false;
                            this.distribution.update(cx, |view, cx| {
                                view.set_closing(false, cx);
                                view.restart_failed(error, cx);
                            });
                            this.sync_request_lock(cx);
                        }
                    }
                });
            });
        }));
        cx.notify();
    }
}
