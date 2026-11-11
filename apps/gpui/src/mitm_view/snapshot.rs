use super::{MitmView, Operation};
use gpui_kit::{AppContext as _, Context, Window};

impl MitmView {
    pub(super) fn refresh_metadata(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy() {
            self.refresh_requested = true;
            return;
        }
        let core = self.core.clone();
        let service = self.service.clone();
        self.refresh_requested = false;
        self.operation = Some(Operation::Refresh);
        self.operation_task = Some(cx.spawn_in(window, async move |this, cx| {
            let (settings, ca, trust, addr, traffic) = cx
                .background_spawn(async move {
                    (
                        service
                            .ok_or_else(|| "实验数据目录不可用".to_owned())
                            .and_then(|service| {
                                service.mitm_settings().map_err(|error| error.to_string())
                            }),
                        core.mitm_ca(),
                        core.mitm_trust_status(),
                        core.mitm_addr(),
                        core.mitm_traffic(),
                    )
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.operation = None;
                    this.operation_task = None;
                    let mut errors = Vec::new();
                    match settings {
                        Ok(settings) => {
                            // Resource-derived names are a live saved snapshot. The manual editor
                            // and switch remain the user's draft until explicitly saved.
                            this.derived = settings.whitelist.derived;
                            if !this.dirty {
                                this.syncing = true;
                                this.hostnames.update(cx, |state, cx| {
                                    let text = settings.hostnames.join("\n");
                                    if state.value().as_ref() != text {
                                        state.set_value(text, window, cx);
                                    }
                                });
                                this.syncing = false;
                                this.enabled = settings.enabled;
                                this.script_dialect = settings.script_dialect;
                            }
                        }
                        Err(error) => errors.push(error),
                    }
                    match ca {
                        Ok(ca) => this.ca = Some(ca),
                        Err(error) => {
                            this.ca = None;
                            errors.push(error);
                        }
                    }
                    match trust {
                        Ok(trust) => this.trust = Some(trust),
                        Err(error) => {
                            this.trust = None;
                            errors.push(error);
                        }
                    }
                    match addr {
                        Ok(addr) => this.mitm_addr = if this.running { addr } else { None },
                        Err(error) => errors.push(error),
                    }
                    match traffic {
                        Ok(records) => this.traffic.update(cx, |state, cx| {
                            if state.delegate_mut().replace(records) {
                                cx.notify();
                            }
                        }),
                        Err(error) => errors.push(error),
                    }
                    this.sync_error = (!errors.is_empty()).then(|| errors.join("；"));
                    cx.notify();
                })
            });
        }));
        cx.notify();
    }
}
