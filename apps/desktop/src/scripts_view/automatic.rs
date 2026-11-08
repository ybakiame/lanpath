use gpui_kit::{AppContext as _, Context};

use super::{Operation, ScriptsView, operations::fetch_notice};

impl ScriptsView {
    // Root owns cross-feature policy. Exclude our accepted operation from that
    // policy so publishing its busy state cannot cancel itself through observers.
    pub(crate) fn set_automatic_refresh_eligible(&mut self, eligible: bool) {
        self.automatic_eligible = eligible;
        if !eligible {
            self.cancel_automatic_refresh();
        }
    }

    pub(super) fn cancel_automatic_refresh(&mut self) {
        // Dropping the sole sender also cancels on view destruction. Keep task and
        // operation retained until the worker returns and releases the write lock.
        self.automatic_cancel.take();
    }

    pub(crate) fn poll_automatic_refresh(&mut self, cx: &mut Context<Self>) {
        if !self.automatic_eligible
            || self.resource_form_open
            || self.local.open
            || self.busy()
            || self.dirty()
            || !self.resources.iter().any(|resource| resource.enabled)
        {
            return;
        }
        let Some(service) = self.service.clone() else {
            return;
        };
        let core = self.core.clone();
        let (cancel, cancelled) = tokio::sync::oneshot::channel::<()>();
        self.automatic_cancel = Some(cancel);
        self.operation = Some(Operation::AutomaticFetch);
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.block_on(async {
                        tokio::select! {
                            biased;
                            _ = cancelled => Ok::<_, String>(None),
                            result = core.fetch_due_remote_resources() => {
                                let Some(report) = result? else {
                                    return Ok(None);
                                };
                                let resources = service.remote_resources()
                                    .map_err(|error| error.to_string())?;
                                Ok(Some((report, resources)))
                            }
                        }
                    })?
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.automatic_cancel = None;
                this.operation = None;
                match result {
                    Ok(Some((report, resources))) => {
                        this.error = None;
                        this.notice = Some(format!(
                            "自动{}；缓存将在下次启动核心时加载",
                            fetch_notice(&report)
                        ));
                        this.fetch_report = Some(report);
                        // Refresh the saved list only. Do not sync inputs, clear
                        // dirty flags, change section, or replace the selection.
                        this.resources = resources;
                    }
                    Ok(None) => {}
                    Err(error) => this.error = Some(format!("自动更新失败：{error}")),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}
