use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
    input::{Editor, Input},
    scroll::ScrollableElement as _,
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

use super::{Operation, SubscriptionsView};

impl Render for SubscriptionsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let controls_max_height =
            (window.viewport_size().height * 0.35).min(window.rem_size() * 14.0);
        let disabled = self.locked || self.busy();
        let selected = self
            .selected
            .and_then(|id| self.subscriptions.iter().find(|item| item.id == id));
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
            .unwrap_or_else(|| "订阅内容保存在独立实验目录；刷新失败保留上次成功缓存".into());
        let controls = div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_wrap()
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
                            .tooltip("刷新已保存的订阅 URL；保留未保存的表单修改")
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
                    .flex_wrap()
                    .min_w_0()
                    .h_auto()
                    .max_h_24()
                    .overflow_y_scrollbar()
                    .gap_2()
                    .children(self.subscriptions.iter().map(|item| {
                        let id = item.id;
                        Button::new(format!("subscription-{id}"))
                            .min_w_0()
                            .max_w_full()
                            .tooltip(item.name.clone())
                            .disabled(disabled)
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
                    .min_w_0()
                    .gap_2()
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.name)
                                .aria_label("订阅名称")
                                .disabled(disabled),
                        ),
                    )
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.url)
                                .aria_label("订阅 URL")
                                .disabled(disabled),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .gap_2()
                    .child(
                        div().flex_1().min_w_0().child(
                            Input::new(&self.user_agent)
                                .aria_label("订阅 User-Agent")
                                .disabled(disabled),
                        ),
                    )
                    .child(
                        Button::new("subscription-profile")
                            .min_w_0()
                            .max_w_full()
                            .tooltip(self.profile_label())
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
            }));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap_2()
            .child(
                controls
                    .w_full()
                    .h_auto()
                    .flex_shrink_0()
                    .max_h(controls_max_height)
                    .overflow_y_scrollbar(),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("最近一次成功响应原文（只读；导出写入实验目录 exports/）"),
            )
            .child(
                div().flex_1().min_w_0().min_h_0().child(
                    Editor::new(&self.source)
                        .aria_label("订阅原文")
                        .readonly(true)
                        .size_full(),
                ),
            )
    }
}
