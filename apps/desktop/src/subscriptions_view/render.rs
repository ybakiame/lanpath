use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
    input::{Editor, Input},
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

use super::{Operation, SubscriptionsView};

impl Render for SubscriptionsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.locked || self.busy();
        let selected = self
            .selected
            .and_then(|id| self.subscriptions.iter().find(|item| item.id == id));
        let status = self
            .error
            .clone()
            .or_else(|| self.notice.clone())
            .unwrap_or_else(|| "订阅内容保存在独立实验目录；刷新失败保留上次成功缓存".into());
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child("订阅")
                    .child(
                        Button::new("subscription-new")
                            .label("新建")
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.request_select(None, window, cx)
                            })),
                    )
                    .child(
                        Button::new("subscription-save")
                            .label(if self.selected.is_some() {
                                "保存"
                            } else {
                                "添加并刷新"
                            })
                            .primary()
                            .disabled(disabled || !self.dirty)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run(Operation::Save, window, cx)
                            })),
                    )
                    .child(
                        Button::new("subscription-refresh")
                            .label("刷新")
                            .disabled(disabled || self.selected.is_none())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run(Operation::Refresh, window, cx)
                            })),
                    )
                    .child(
                        Button::new("subscription-activate")
                            .label("设为当前")
                            .disabled(
                                disabled
                                    || self.selected.is_none()
                                    || self.selected == self.active
                                    || selected.is_some_and(|item| !item.enabled),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run(Operation::Activate, window, cx)
                            })),
                    )
                    .child(
                        Button::new("subscription-toggle")
                            .label(if selected.is_some_and(|item| item.enabled) {
                                "停用"
                            } else {
                                "启用"
                            })
                            .disabled(disabled || self.selected.is_none())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run(Operation::Toggle, window, cx)
                            })),
                    )
                    .child(
                        Button::new("subscription-export")
                            .label("导出原文")
                            .disabled(disabled || self.selected.is_none())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run(Operation::Export, window, cx)
                            })),
                    )
                    .child(
                        Button::new("subscription-delete")
                            .label("删除")
                            .danger()
                            .disabled(disabled || self.selected.is_none())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.request_delete(window, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .id("subscription-status")
                    .role(Role::Status)
                    .aria_label(status.clone())
                    .text_sm()
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
                    .gap_2()
                    .children(self.subscriptions.iter().map(|item| {
                        let id = item.id;
                        Button::new(format!("subscription-{id}"))
                            .label(format!(
                                "{}{} · {} 节点",
                                if Some(id) == self.active {
                                    "当前 · "
                                } else {
                                    ""
                                },
                                item.name,
                                item.node_count
                            ))
                            .selected(Some(id) == self.selected)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.request_select(Some(id), window, cx)
                            }))
                    })),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div().flex_1().child(
                            Input::new(&self.name)
                                .aria_label("订阅名称")
                                .disabled(disabled),
                        ),
                    )
                    .child(
                        div().flex_1().child(
                            Input::new(&self.url)
                                .aria_label("订阅 URL")
                                .disabled(disabled),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div().flex_1().child(
                            Input::new(&self.user_agent)
                                .aria_label("订阅 User-Agent")
                                .disabled(disabled),
                        ),
                    )
                    .child(
                        Button::new("subscription-profile")
                            .label(self.profile_label())
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, _, cx| this.cycle_profile(cx))),
                    ),
            )
            .children(selected.and_then(|item| item.error.as_ref()).map(|error| {
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone())
            }))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("最近一次成功响应原文（只读；导出写入实验目录 exports/）"),
            )
            .child(
                div().flex_1().min_h_0().child(
                    Editor::new(&self.source)
                        .aria_label("订阅原文")
                        .readonly(true)
                        .size_full(),
                ),
            )
    }
}
