use gpui_kit::component::WindowExt as _;
use gpui_kit::{AppContext as _, ClipboardItem, Context, Window};
use lanpath_client::mitm::CaTrustStatus;

use super::{MitmChanged, MitmView, Operation};

impl MitmView {
    pub(super) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        let Some(service) = self.service.clone() else {
            self.error = Some("实验数据目录不可用".into());
            cx.notify();
            return;
        };
        let enabled = self.enabled;
        let text = self.hostnames.read(cx).value().to_string();
        let hostnames = text.lines().map(str::to_string).collect();
        let dialect = self.script_dialect;
        self.operation = Some(Operation::Save);
        self.error = None;
        self.notice = None;
        self.operation_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(
                    async move { service.save_mitm_settings(enabled, hostnames, dialect) },
                )
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.operation = None;
                    this.operation_task = None;
                    match result {
                        Ok(settings) => {
                            this.syncing = true;
                            this.hostnames.update(cx, |state, cx| {
                                state.set_value(settings.hostnames.join("\n"), window, cx);
                            });
                            this.syncing = false;
                            this.enabled = settings.enabled;
                            this.dirty = false;
                            this.script_dialect = settings.script_dialect;
                            this.derived = settings.whitelist.derived;
                            this.notice = Some("MITM 设置已保存；下次启动核心时生效".into());
                            cx.emit(MitmChanged);
                        }
                        Err(error) => this.error = Some(error.to_string()),
                    }
                    cx.notify();
                })
                .ok();
            });
        }));
        cx.notify();
    }

    pub(super) fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        if self.dirty() {
            let view = cx.weak_entity();
            window.open_alert_dialog(cx, move |dialog, _, _| {
                let view = view.clone();
                dialog
                    .title("放弃 MITM 设置修改？")
                    .description("重新载入会丢弃未保存的开关和手动 Hostname 白名单。")
                    .confirm()
                    .ok_text("放弃并重新载入")
                    .cancel_text("取消")
                    .on_ok(move |_, window, cx| {
                        let _ = view.update(cx, |view, cx| view.load_now(window, cx));
                        true
                    })
            });
        } else {
            self.load_now(window, cx);
        }
    }

    fn load_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self
            .service
            .as_ref()
            .ok_or_else(|| "实验数据目录不可用".to_string())
            .and_then(|service| service.mitm_settings().map_err(|error| error.to_string()));
        match result {
            Ok(settings) => {
                let text = settings.hostnames.join("\n");
                self.syncing = true;
                self.hostnames
                    .update(cx, |state, cx| state.set_value(text, window, cx));
                self.syncing = false;
                self.enabled = settings.enabled;
                self.dirty = false;
                self.script_dialect = settings.script_dialect;
                self.derived = settings.whitelist.derived;
                self.error = None;
                self.notice = Some("已重新载入 MITM 设置".into());
            }
            Err(error) => self.error = Some(error),
        }
        self.refresh_metadata(cx);
        cx.notify();
    }

    pub(super) fn refresh_metadata(&mut self, cx: &mut Context<Self>) {
        if self.operation.is_some() {
            return;
        }
        let core = self.core.clone();
        self.operation = Some(Operation::Refresh);
        self.error = None;
        self.operation_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    (
                        core.mitm_ca(),
                        core.mitm_trust_status(),
                        core.mitm_addr(),
                        core.mitm_traffic(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.operation = None;
                this.operation_task = None;
                let (ca, trust, addr, traffic) = result;
                let mut errors = Vec::new();
                match ca {
                    Ok(ca) => this.ca = Some(ca),
                    Err(error) => errors.push(error),
                }
                match trust {
                    Ok(trust) => this.trust = Some(trust),
                    Err(error) => errors.push(error),
                }
                match addr {
                    Ok(addr) => this.mitm_addr = addr,
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
                this.error = (!errors.is_empty()).then(|| errors.join("；"));
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn request_install_ca(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .title("信任实验 MITM 根证书？")
                .description("这会把当前实验数据根生成的 CA 写入当前用户系统信任库，使命中白名单的 HTTPS 流量可被解密。不会自动启用系统代理；仅在你理解影响时继续。")
                .confirm()
                .ok_text("安装并信任")
                .cancel_text("取消")
                .on_ok(move |_, _, cx| {
                    let _ = view.update(cx, |view, cx| view.install_ca(cx));
                    true
                })
        });
    }

    fn install_ca(&mut self, cx: &mut Context<Self>) {
        let core = self.core.clone();
        self.operation = Some(Operation::InstallCa);
        self.error = None;
        self.notice = None;
        self.operation_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { core.install_mitm_ca() })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.operation = None;
                this.operation_task = None;
                match result {
                    Ok(trust) => {
                        let trusted = trust.status == CaTrustStatus::Trusted;
                        if trusted {
                            this.notice = Some("实验 MITM CA 已安装并确认受信任".into());
                        } else {
                            this.error = Some(format!(
                                "CA 安装完成，但系统信任状态仍为 {:?}：{}",
                                trust.status, trust.detail
                            ));
                        }
                        this.trust = Some(trust);
                    }
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn copy_ca(&mut self, cx: &mut Context<Self>) {
        let Some(ca) = &self.ca else {
            self.error = Some("CA 尚未生成，请先刷新证书状态".into());
            cx.notify();
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(ca.pem.clone()));
        self.notice = Some("证书 PEM 已复制".into());
        cx.notify();
    }
}
