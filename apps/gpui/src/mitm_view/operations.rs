use gpui_kit::component::WindowExt as _;
use gpui_kit::{AppContext as _, ClipboardItem, Context, Window};
use lanpath_client::mitm::{CaOwnershipStatus, CaTrustStatus};

use super::{MitmChanged, MitmView, Operation};

impl MitmView {
    pub(crate) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() || !self.dirty() {
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

    pub(super) fn request_install_ca(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        let Some(ca) = self.ca.clone() else {
            return;
        };
        let description = format!(
            "指纹 SHA-256：{}\n范围：{:?}\n这会允许解密白名单 HTTPS 流量，不会启用系统代理。停核、退出或重启不会撤销信任；仅本应用实际新增且所有权未变的证书可被移除。",
            ca.sha256, ca.scope
        );
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            let expected = ca.sha256.clone();
            dialog
                .title("信任实验 MITM 根证书？")
                .description(description.clone())
                .confirm()
                .ok_text("安装并信任")
                .cancel_text("取消")
                .on_ok(move |_, _, cx| {
                    let _ = view.update(cx, |view, cx| view.install_ca(expected.clone(), cx));
                    true
                })
        });
    }

    fn install_ca(&mut self, expected_sha256: String, cx: &mut Context<Self>) {
        let core = self.core.clone();
        self.operation = Some(Operation::InstallCa);
        self.error = None;
        self.notice = None;
        self.operation_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { core.install_mitm_ca(&expected_sha256) })
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

    pub(super) fn request_remove_ca(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        let Some(trust) = &self.trust else {
            return;
        };
        if !matches!(
            trust.ownership,
            CaOwnershipStatus::Owned | CaOwnershipStatus::Absent
        ) {
            return;
        }
        let Some(expected) = trust.receipt_sha256.clone() else {
            return;
        };
        let description = format!(
            "指纹 SHA-256：{expected}\n范围：Windows CurrentUser Root\n仅移除本应用新增且原始所有权令牌仍匹配的证书。既有根和所有权已改变的证书保持不动。"
        );
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            let expected = expected.clone();
            dialog
                .title("移除本应用添加的 CA 信任？")
                .description(description.clone())
                .confirm()
                .ok_text("移除拥有的信任")
                .cancel_text("取消")
                .on_ok(move |_, _, cx| {
                    let _ = view.update(cx, |view, cx| view.remove_ca(expected.clone(), cx));
                    true
                })
        });
    }

    fn remove_ca(&mut self, expected_sha256: String, cx: &mut Context<Self>) {
        let core = self.core.clone();
        self.operation = Some(Operation::RemoveCa);
        self.error = None;
        self.notice = None;
        self.operation_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.remove_mitm_ca(&expected_sha256)
                        .map(|receipt| (receipt, core.mitm_trust_status()))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.operation = None;
                this.operation_task = None;
                match result {
                    Ok((receipt, status)) => {
                        this.notice = Some(
                            if receipt.disposition
                                == lanpath_mitm::CaRemovalDisposition::AlreadyAbsent
                            {
                                "拥有的证书已不在信任库；旧回执已清理"
                            } else {
                                "本应用拥有的 CA 信任已移除；既有根未改动"
                            }
                            .into(),
                        );
                        match status {
                            Ok(trust) => this.trust = Some(trust),
                            Err(error) => {
                                this.trust = None;
                                this.error =
                                    Some(format!("信任移除已完成，但状态查询失败：{error}"));
                            }
                        }
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
            self.error = Some("CA 尚未生成，正在自动同步证书状态".into());
            cx.notify();
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(ca.pem.clone()));
        self.notice = Some("证书 PEM 已复制".into());
        cx.notify();
    }
}
