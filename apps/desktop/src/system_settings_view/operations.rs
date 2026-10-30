use gpui_kit::component::WindowExt as _;
use gpui_kit::{AppContext as _, Context, Window};

use super::{SystemSettingsChanged, SystemSettingsView};

impl SystemSettingsView {
    pub(super) fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.unavailable() {
            return;
        }
        self.working = Some("正在读取网络设置");
        self.error = None;
        self.sync_readonly(cx);
        let core = self.core.clone();
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.business()?
                        .system_settings()
                        .map_err(|error| error.to_string())
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.working = None;
                this.task = None;
                match result {
                    Ok(document) => {
                        this.apply(document, window, cx);
                        this.error = None;
                    }
                    Err(error) => this.error = Some(error),
                }
                this.sync_readonly(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(crate) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_disabled() || !self.dirty {
            return;
        }
        let document = match self.document(cx) {
            Ok(document) => document,
            Err(error) => {
                self.error = Some(error);
                cx.notify();
                return;
            }
        };
        self.working = Some("正在保存网络设置");
        self.error = None;
        self.notice = None;
        self.sync_readonly(cx);
        let core = self.core.clone();
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.business()?
                        .save_system_settings(&document)
                        .map_err(|error| error.to_string())?;
                    core.saved_proxy_port_changed(document.mixed_port);
                    Ok::<_, String>(document)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.working = None;
                this.task = None;
                match result {
                    Ok(document) => {
                        this.apply(document, window, cx);
                        this.notice = Some(
                            "已保存；下次启动核心生效。未修改当前系统代理、路由或证书。".into(),
                        );
                        cx.emit(SystemSettingsChanged);
                    }
                    Err(error) => this.error = Some(error),
                }
                this.sync_readonly(cx);
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_disabled() || !self.dirty {
            return;
        }
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .title("放弃网络设置修改？")
                .description("恢复页面已载入的保存值，不会修改磁盘或当前网络。")
                .confirm()
                .ok_text("放弃修改")
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| {
                        if view.editing_disabled() {
                            return;
                        }
                        if let Some(saved) = view.saved.clone() {
                            view.apply(saved, window, cx);
                        }
                        view.error = None;
                        view.notice = None;
                        cx.notify();
                    });
                    true
                })
        });
    }
}
