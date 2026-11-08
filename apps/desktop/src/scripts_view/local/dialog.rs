use super::super::{Operation, ScriptsView};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{
    Context, Focusable as _, InteractiveElement as _, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
impl ScriptsView {
    pub(super) fn open_local_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.local.open = true;
        self.local.confirming = false;
        let view = cx.entity();
        window.open_dialog(cx, move |dialog, window, app| {
            let this = view.read(app);
            let disabled = this.locked || this.busy();
            let dialog = dialog
                .bg(app.theme().popover)
                .w(window.rem_size() * 46.)
                .max_h(window.viewport_size().height * 0.85)
                .margin_top(window.rem_size() * 2.)
                .overlay_closable(false);
            let dialog = if this.local.confirming {
                dialog
                    .title("放弃本地资源修改？")
                    .child("当前片段、JS 与图标修改尚未保存，放弃后已保存资源保持不变。")
                    .footer(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(Button::new("local-continue").label("继续编辑").on_click(
                                window.listener_for(&view, |this, _, window, cx| {
                                    this.local.confirming = false;
                                    this.local.name.read(cx).focus_handle(cx).focus(window, cx);
                                    cx.notify();
                                }),
                            ))
                            .child(
                                Button::new("local-discard")
                                    .danger()
                                    .label("放弃修改")
                                    .on_click(window.listener_for(&view, |this, _, window, cx| {
                                        this.local.dirty = false;
                                        this.close_local_dialog(window, cx);
                                    })),
                            ),
                    )
            } else {
                let mut status = this
                    .error
                    .clone()
                    .or_else(|| this.notice.clone())
                    .unwrap_or_else(|| {
                        if this.local.dirty {
                            "有未保存修改".into()
                        } else {
                            "配置片段、关联 JS 与图标一起保存".into()
                        }
                    });
                if this.error.is_none() && !this.local.advisories.is_empty() {
                    status.push_str(" · 兼容提示：");
                    status.push_str(&this.local.advisories.join("；"));
                }
                dialog
                    .title(if this.local.original.is_none() {
                        "添加本地资源"
                    } else {
                        "编辑本地资源"
                    })
                    .child(
                        div()
                            .id("local-form-body")
                            .key_context("Desktop")
                            .on_action(window.listener_for(
                                &view,
                                |this, _: &crate::keyboard::SaveCurrentForm, window, cx| {
                                    cx.stop_propagation();
                                    this.save_local(window, cx);
                                },
                            ))
                            .child(this.local_fields(disabled, window, &view, app)),
                    )
                    .footer(
                        div()
                            .key_context("Desktop")
                            .on_action(window.listener_for(
                                &view,
                                |this, _: &crate::keyboard::SaveCurrentForm, window, cx| {
                                    cx.stop_propagation();
                                    this.save_local(window, cx);
                                },
                            ))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .w_full()
                            .child(
                                div()
                                    .id("local-form-status")
                                    .accessibility_id("local-form-status")
                                    .role(Role::Status)
                                    .aria_label(status.clone())
                                    .text_sm()
                                    .max_h_16()
                                    .overflow_y_scroll()
                                    .text_color(if this.error.is_some() {
                                        app.theme().danger
                                    } else if !this.local.advisories.is_empty() {
                                        app.theme().warning
                                    } else {
                                        app.theme().muted_foreground
                                    })
                                    .child(status),
                            )
                            .child(
                                div()
                                    .flex()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        Button::new("local-cancel")
                                            .accessibility_id("local-cancel")
                                            .label("取消")
                                            .disabled(disabled)
                                            .on_click(window.listener_for(
                                                &view,
                                                |this, _, window, cx| {
                                                    this.cancel_local_dialog(window, cx);
                                                },
                                            )),
                                    )
                                    .child(
                                        Button::new("local-save")
                                            .accessibility_id("local-save")
                                            .primary()
                                            .label(if this.operation == Some(Operation::Local) {
                                                "保存中…"
                                            } else {
                                                "保存资源"
                                            })
                                            .disabled(disabled || !this.local.dirty)
                                            .on_click(
                                                window.listener_for(
                                                    &view,
                                                    |this, _, window, cx| {
                                                        this.save_local(window, cx)
                                                    },
                                                ),
                                            ),
                                    ),
                            ),
                    )
            };
            dialog
                .on_cancel({
                    let view = view.clone();
                    move |_, window, app| {
                        view.update(app, |this, cx| this.cancel_local_dialog(window, cx))
                    }
                })
                .on_close({
                    let view = view.clone();
                    move |_, _, app| {
                        view.update(app, |this, cx| {
                            this.local.open = false;
                            this.local.confirming = false;
                            cx.notify();
                        })
                    }
                })
        });
        cx.notify();
    }
    fn cancel_local_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.locked || self.busy() {
            return false;
        }
        if self.local.confirming {
            self.local.confirming = false;
            self.local.name.read(cx).focus_handle(cx).focus(window, cx);
            cx.notify();
            return false;
        }
        if self.local.dirty {
            self.local.confirming = true;
            cx.notify();
            return false;
        }
        self.close_local_dialog(window, cx);
        true
    }
    fn close_local_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.local.open = false;
        self.local.confirming = false;
        self.error = None;
        self.notice = None;
        window.close_dialog(cx);
        cx.notify();
    }
}
