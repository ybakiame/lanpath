use super::*;

impl ValidationView {
    pub(crate) fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.connections
            .update(cx, |view, cx| view.set_running(false, cx));
        self.stats
            .update(cx, |view, cx| view.set_running(false, cx));
        let core = self.core.clone();
        self.close_task = Some(cx.spawn_in(window, async move |this, cx| {
            // Finish already accepted commands before stopping their shared
            // tracker/runtime. New commands are disabled while closing.
            loop {
                let Ok(busy) = this.update(cx, |this, _| {
                    this.core_busy.is_some() || this.proxy_busy.is_some()
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
                        this.core_error = Some(error);
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
