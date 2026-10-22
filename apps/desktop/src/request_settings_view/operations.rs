use gpui_kit::component::WindowExt as _;
use gpui_kit::{AppContext as _, Context, Window};
use lanpath_common::LanpathError;

use super::{ProbeResult, RequestSettingsView};

impl RequestSettingsView {
    pub(super) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_disabled() || !self.dirty() {
            return;
        }
        let Some(service) = &self.service else {
            self.error = Some("实验数据目录不可用".into());
            cx.notify();
            return;
        };
        // This small atomic commit stays on the UI thread. Peer business writes
        // are already locked by the shell; no asynchronous save can race them.
        match service.save_request_settings(&self.draft(cx)) {
            Ok(settings) => {
                self.apply_saved(settings, window, cx);
                self.error = None;
                self.notice = Some("网络请求设置已保存；后续请求使用已保存的策略".into());
            }
            Err(error) => {
                self.error = Some(error.to_string());
                self.notice = None;
            }
        }
        cx.notify();
    }

    pub(super) fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_disabled() {
            return;
        }
        if self.dirty() {
            let view = cx.weak_entity();
            window.open_alert_dialog(cx, move |dialog, _, _| {
                let view = view.clone();
                dialog
                    .title("放弃网络请求设置修改？")
                    .description("重新载入会丢弃未保存的代理模式、地址和 GitHub 镜像前缀。")
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
        // A dialog may outlive a core-start or closing transition.
        if self.editing_disabled() {
            return;
        }
        let loaded = self
            .service
            .as_ref()
            .ok_or_else(|| "实验数据目录不可用".to_string())
            .and_then(|service| {
                service
                    .request_settings()
                    .map_err(|error| error.to_string())
            });
        match loaded {
            Ok(settings) => {
                self.apply_saved(settings, window, cx);
                self.error = None;
                self.notice = Some("已重新载入网络请求设置".into());
            }
            Err(error) => {
                self.error = Some(error);
                self.notice = None;
            }
        }
        cx.notify();
    }

    pub(super) fn probe(&mut self, cx: &mut Context<Self>) {
        if self.closing || self.busy() {
            return;
        }
        let captured = self
            .service
            .as_ref()
            .ok_or_else(|| "实验数据目录不可用".to_string())
            .and_then(|service| {
                service
                    .saved_request_policy()
                    .map_err(|error| error.to_string())
            });
        let (settings, config) = match captured {
            Ok(captured) => captured,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        self.probe_sequence += 1;
        let sequence = self.probe_sequence;
        self.probing = true;
        self.error = None;
        self.notice = None;
        self.sync_readonly(cx);
        let core = self.core.clone();
        self.probe_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.block_on(lanpath_client::test_github_connectivity(&config))
                        .map_err(LanpathError::Client)
                        .and_then(|result| result)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.probing = false;
                this.probe_task = None;
                this.probe_result = Some(ProbeResult {
                    sequence,
                    settings,
                    result,
                });
                this.sync_readonly(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }
}
