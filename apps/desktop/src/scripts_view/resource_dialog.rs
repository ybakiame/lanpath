use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{
    Context, Focusable as _, InteractiveElement as _, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

use super::{Operation, ScriptsView};

impl ScriptsView {
    pub(super) fn open_resource_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.resource_form_open {
            return;
        }
        self.resource_form_open = true;
        self.resource_confirm_discard = false;
        let view = cx.entity();
        window.open_dialog(cx, move |dialog, window, app| {
            let this = view.read(app);
            let disabled = this.locked || this.busy();
            let confirming = this.resource_confirm_discard;
            let dialog = dialog
                .bg(app.theme().popover)
                .w(window.rem_size() * 42.0)
                .max_h(window.viewport_size().height * 0.85)
                .margin_top(window.rem_size() * 2.0)
                .overlay_closable(false);
            let dialog = if confirming {
                dialog
                    .title("放弃资源修改？")
                    .child("当前表单修改尚未保存，放弃后不会影响已保存的资源。")
                    .footer(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("remote-discard-cancel")
                                    .label("继续编辑")
                                    .on_click(window.listener_for(&view, |this, _, window, cx| {
                                        this.resource_confirm_discard = false;
                                        this.inputs
                                            .description
                                            .read(cx)
                                            .focus_handle(cx)
                                            .focus(window, cx);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("remote-discard-confirm")
                                    .danger()
                                    .label("放弃修改")
                                    .on_click(window.listener_for(&view, |this, _, window, cx| {
                                        this.discard_resource_dialog(window, cx);
                                    })),
                            ),
                    )
            } else {
                let status = this
                    .error
                    .clone()
                    .or_else(|| this.notice.clone())
                    .unwrap_or_else(|| {
                        if this.resource_dirty {
                            "有未保存修改".into()
                        } else {
                            "保存后可更新资源缓存".into()
                        }
                    });
                dialog
                    .title(if this.creating {
                        "添加远程资源"
                    } else {
                        "编辑远程资源"
                    })
                    .child(
                        div()
                            .id("resource-form-scroll")
                            .key_context("Desktop")
                            .on_action(window.listener_for(
                                &view,
                                |this, _: &crate::keyboard::SaveCurrentForm, window, cx| {
                                    cx.stop_propagation();
                                    this.save_resource(window, cx);
                                },
                            ))
                            .accessibility_id("resource-form-scroll")
                            .role(Role::Group)
                            .aria_label("远程资源表单")
                            .child(this.render_resource_fields(disabled, window, &view, app)),
                    )
                    .footer(
                        div()
                            .key_context("Desktop")
                            .on_action(window.listener_for(
                                &view,
                                |this, _: &crate::keyboard::SaveCurrentForm, window, cx| {
                                    cx.stop_propagation();
                                    this.save_resource(window, cx);
                                },
                            ))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .w_full()
                            .child(
                                div()
                                    .id("remote-form-status")
                                    .accessibility_id("remote-form-status")
                                    .role(Role::Status)
                                    .aria_label(status.clone())
                                    .max_h_16()
                                    .overflow_y_scroll()
                                    .text_sm()
                                    .text_color(if this.error.is_some() {
                                        app.theme().danger
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
                                        Button::new("remote-cancel")
                                            .accessibility_id("remote-cancel")
                                            .label("取消")
                                            .disabled(disabled)
                                            .on_click(window.listener_for(
                                                &view,
                                                |this, _, window, cx| {
                                                    this.cancel_resource_dialog(window, cx);
                                                },
                                            )),
                                    )
                                    .child(
                                        Button::new("remote-save")
                                            .accessibility_id("remote-save")
                                            .primary()
                                            .label(if this.operation == Some(Operation::Save) {
                                                "保存中…"
                                            } else {
                                                "保存资源"
                                            })
                                            .disabled(
                                                disabled
                                                    || !this.resource_dirty
                                                    || !this.resource_dialect_allowed(),
                                            )
                                            .on_click(window.listener_for(
                                                &view,
                                                |this, _, window, cx| {
                                                    this.save_resource(window, cx)
                                                },
                                            )),
                                    ),
                            ),
                    )
            };
            dialog
                .on_cancel({
                    let view = view.clone();
                    move |_, window, app| {
                        view.update(app, |this, cx| this.cancel_resource_dialog(window, cx))
                    }
                })
                .on_close({
                    let view = view.clone();
                    move |_, _, app| {
                        view.update(app, |this, cx| {
                            this.resource_form_open = false;
                            this.resource_confirm_discard = false;
                            cx.notify();
                        });
                    }
                })
        });
        cx.notify();
    }

    fn cancel_resource_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.locked || self.busy() {
            return false;
        }
        if self.resource_confirm_discard {
            self.resource_confirm_discard = false;
            self.inputs
                .description
                .read(cx)
                .focus_handle(cx)
                .focus(window, cx);
            cx.notify();
            return false;
        }
        if self.resource_dirty {
            self.resource_confirm_discard = true;
            cx.notify();
            return false;
        }
        self.finish_resource_dialog(window, cx);
        true
    }

    fn discard_resource_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        self.sync_resource(window, cx);
        self.error = None;
        self.notice = None;
        self.finish_resource_dialog(window, cx);
    }

    pub(super) fn finish_resource_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.resource_form_open {
            self.resource_form_open = false;
            self.resource_confirm_discard = false;
            window.close_dialog(cx);
        }
        cx.notify();
    }
}
