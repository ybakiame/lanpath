use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    form::{Form, field},
    input::Input,
    scroll::ScrollableElement as _,
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _,
};

use super::{Operation, SubscriptionsView};

const USER_AGENTS: [(&str, &str, &str); 4] = [
    ("subscription-ua-default", "", "默认 UA"),
    ("subscription-ua-clash-meta", "clash.meta", "clash.meta"),
    ("subscription-ua-clash-verge", "clash-verge", "clash-verge"),
    ("subscription-ua-sing-box", "sing-box", "sing-box"),
];

impl SubscriptionsView {
    pub(super) fn open_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor_open || self.locked || self.busy() {
            return;
        }
        self.editor_open = true;
        self.editor_confirm_discard = false;
        let creating = self.selected.is_none();
        let view = cx.entity();
        window.open_dialog(cx, move |dialog, window, app| {
            let view = view.clone();
            let confirming = view.read(app).editor_confirm_discard;
            let mut dialog = dialog
                .w(window.rem_size() * 36.0)
                .margin_top(window.viewport_size().height / 4.0)
                .shadow_lg();
            if confirming {
                dialog = dialog
                    .title("放弃未保存的订阅修改？")
                    .child("当前表单修改尚未保存，放弃后不会影响已保存的订阅。")
                    .footer(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("subscription-discard-cancel")
                                    .label("继续编辑")
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                this.editor_confirm_discard = false;
                                                cx.notify();
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new("subscription-discard-confirm")
                                    .danger()
                                    .label("放弃修改")
                                    .on_click({
                                        let view = view.clone();
                                        move |_, window, cx| {
                                            view.update(cx, |this, cx| {
                                                let selected = this.selected;
                                                this.select(selected, window, cx);
                                                this.close_editor(window, cx);
                                            });
                                        }
                                    }),
                            ),
                    );
            } else {
                let this = view.read(app);
                let disabled = this.locked || this.busy();
                let dirty = this.dirty;
                let current_user_agent = this.user_agent.read(app).value();
                dialog = dialog
                    .title(if creating {
                        "添加订阅"
                    } else {
                        "编辑订阅"
                    })
                    .child(
                        Form::new()
                            .columns(2)
                            .gap_3()
                            .child(
                                field().label("名称").required(true).child(
                                    Input::new(&view.read(app).name.clone())
                                        .accessibility_id("subscription-name")
                                        .aria_label("订阅名称")
                                        .disabled(disabled),
                                ),
                            )
                            .child(
                                field().label("User-Agent（可选）").child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_2()
                                        .child(
                                            Input::new(&view.read(app).user_agent.clone())
                                                .accessibility_id("subscription-user-agent")
                                                .aria_label("订阅 User-Agent")
                                                .disabled(disabled),
                                        )
                                        .child(div().flex().flex_wrap().gap_1().children(
                                            USER_AGENTS.iter().map(|(id, value, label)| {
                                                Button::new(*id)
                                                    .xsmall()
                                                    .outline()
                                                    .rounded_full()
                                                    .selected(current_user_agent.as_str() == *value)
                                                    .label(*label)
                                                    .disabled(disabled)
                                                    .on_click({
                                                        let view = view.clone();
                                                        move |_, window, cx| {
                                                            view.update(cx, |this, cx| {
                                                                this.set_user_agent(
                                                                    value, window, cx,
                                                                )
                                                            });
                                                        }
                                                    })
                                            }),
                                        )),
                                ),
                            )
                            .child(
                                field().label("订阅 URL").required(true).col_span(2).child(
                                    Input::new(&view.read(app).url.clone())
                                        .accessibility_id("subscription-url")
                                        .aria_label("订阅 URL")
                                        .disabled(disabled),
                                ),
                            )
                            .child(
                                field().label("覆写模板").col_span(2).child(
                                    Button::new("subscription-profile")
                                        .icon(IconName::Layers)
                                        .min_w_0()
                                        .max_w_full()
                                        .w_full()
                                        .tooltip(view.read(app).profile_label())
                                        .label(view.read(app).profile_label())
                                        .disabled(disabled)
                                        .on_click({
                                            let view = view.clone();
                                            move |_, _, cx| {
                                                view.update(cx, |this, cx| this.cycle_profile(cx));
                                            }
                                        }),
                                ),
                            ),
                    )
                    .footer(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("subscription-cancel-edit")
                                    .label("取消")
                                    .disabled(disabled)
                                    .on_click({
                                        let view = view.clone();
                                        move |_, window, cx| {
                                            view.update(cx, |this, cx| {
                                                this.cancel_editor(window, cx)
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new("subscription-save")
                                    .primary()
                                    .icon(IconName::Save)
                                    .label(if creating {
                                        "添加并刷新"
                                    } else {
                                        "保存"
                                    })
                                    .loading(matches!(view.read(app).busy, Some(Operation::Save)))
                                    .disabled(disabled || !dirty)
                                    .on_click({
                                        let view = view.clone();
                                        move |_, window, cx| {
                                            view.update(cx, |this, cx| {
                                                this.run(Operation::Save, window, cx)
                                            });
                                        }
                                    }),
                            ),
                    );
            }
            dialog
                .on_cancel({
                    let view = view.clone();
                    move |_, window, cx| view.update(cx, |this, cx| this.cancel_editor(window, cx))
                })
                .on_close({
                    let view = view.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, _| {
                            this.editor_open = false;
                            this.editor_confirm_discard = false;
                        });
                    }
                })
        });
        cx.notify();
    }

    pub(super) fn close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor_open {
            self.editor_open = false;
            self.editor_confirm_discard = false;
            window.close_dialog(cx);
        }
    }

    fn cancel_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.locked || self.busy() {
            return false;
        }
        if self.dirty {
            self.editor_confirm_discard = true;
            cx.notify();
            return false;
        }
        self.editor_open = false;
        window.close_dialog(cx);
        true
    }
}

impl Render for SubscriptionsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.locked || self.busy();
        let operation = self.busy.map(|operation| match operation {
            Operation::Save if self.selected.is_none() => "正在添加并刷新订阅…",
            Operation::Save => "正在保存订阅…",
            Operation::Refresh => "正在刷新已保存 URL 的缓存…",
            Operation::Activate => "正在切换当前订阅…",
            Operation::Toggle => "正在更新订阅可用状态…",
            Operation::Delete => "正在删除订阅与缓存…",
            Operation::Export => "正在导出订阅原文…",
        });
        let status = operation
            .map(str::to_string)
            .or_else(|| self.error.clone())
            .or_else(|| self.notice.clone())
            .unwrap_or_else(|| "刷新失败保留上次成功缓存".into());

        div()
            .id("subscriptions-page")
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap_4()
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(div().text_lg().font_semibold().child("订阅管理"))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("共 {} 个订阅源", self.subscriptions.len())),
                            ),
                    )
                    .child(
                        Button::new("subscription-new")
                            .small()
                            .icon(IconName::Plus)
                            .label("添加订阅")
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.request_select(None, window, cx, true)
                            })),
                    ),
            )
            .child(
                div()
                    .id("subscription-status")
                    .accessibility_id("subscription-status")
                    .role(Role::Status)
                    .aria_label(status.clone())
                    .text_sm()
                    .flex_shrink_0()
                    .text_color(if self.error.is_some() {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(status),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .gap_4()
                    .overflow_y_scrollbar()
                    .p_1()
                    .children(
                        self.subscriptions
                            .iter()
                            .map(|item| self.render_card(item, disabled, cx)),
                    )
                    .when(self.subscriptions.is_empty(), |element| {
                        element.child(
                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .flex_1()
                                .gap_3()
                                .p_8()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(IconName::Inbox)
                                .child("暂无订阅"),
                        )
                    }),
            )
    }
}
