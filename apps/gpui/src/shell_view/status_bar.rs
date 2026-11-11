use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, ThemeStyled as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    status_bar::StatusBar,
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _,
};

use crate::{CoreOperationKind, DesktopView, Page};

impl DesktopView {
    pub(super) fn render_status_bar(
        &self,
        operation_busy: bool,
        dirty: bool,
        window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let compact = window.viewport_size().width < window.rem_size() * 68.0;
        let narrow = window.viewport_size().width < window.rem_size() * 48.0;
        let reason = self.start_block_reason(operation_busy, dirty, cx);
        let state = if self.closing {
            "正在关闭"
        } else if let Some(operation) = self.core_busy {
            match operation {
                CoreOperationKind::Start => "正在启动",
                CoreOperationKind::Stop => "正在停止",
            }
        } else if self.core_recovery_needed {
            "需要恢复"
        } else if self.core_error.is_some() {
            if self.core_running {
                "运行异常"
            } else {
                "启动失败"
            }
        } else if self.core_running {
            "运行中"
        } else if reason.is_some() {
            "待配置"
        } else {
            "未启动"
        };
        let state_color = if self.core_error.is_some() || self.core_recovery_needed {
            cx.theme().danger
        } else if self.core_running {
            cx.theme().success
        } else {
            cx.theme().muted_foreground
        };
        let mut detail = format!(
            "{state}\n{} · 127.0.0.1:{}",
            self.cores.read(cx).selected_core_summary(),
            self.core.port()
        );
        if matches!(self.core_busy, Some(CoreOperationKind::Start)) {
            detail.push_str(&format!("\n{}", self.startup_status));
        }
        if let Some(error) = &self.core_error {
            detail.push_str(&format!("\n\n{error}"));
        } else if !self.core_running
            && let Some(reason) = reason
        {
            detail.push_str(&format!("\n\n{reason}"));
        }
        let subscription = self.subscriptions.read(cx).active_subscription();
        let subscription_label = subscription
            .map(|item| {
                if item.enabled {
                    item.name.clone()
                } else {
                    format!("{} · 已停用", item.name)
                }
            })
            .unwrap_or_else(|| "未选择订阅".into());
        let subscription_detail = subscription
            .map(|item| format!("当前订阅：{subscription_label}\n{} 个节点", item.node_count))
            .unwrap_or_else(|| "未选择订阅，打开订阅页选择".into());
        let core_detail = self.cores.read(cx).selected_core_summary();
        let core_label = core_detail.clone();
        let node = self.proxy.read(cx).selection_status();
        let app_version = concat!("GPUI 试验版 · v", env!("CARGO_PKG_VERSION"));
        let version = Button::new("desktop-about")
            .accessibility_id("desktop-about")
            .small()
            .ghost()
            .icon(IconName::Info)
            .accessibility_label(format!("关于岚径 · {app_version}"))
            .tooltip(format!("关于岚径 · {app_version}"))
            .when(!narrow, |button| {
                button.label(if compact {
                    "GPUI 试验版"
                } else {
                    app_version
                })
            })
            .disabled(self.closing)
            .on_click(cx.listener(|this, _, window, cx| {
                this.set_page(Page::About, cx);
                this.content_focus.focus(window, cx);
            }));
        // Each StatusBar region clips overflow. Inset its controls so keyboard
        // focus rings retain clearance instead of being cut off at the region edge.
        let bar = StatusBar::new()
            .flex_shrink_0()
            .min_w_0()
            .py_0()
            .px_2()
            .left(
                div()
                    .flex()
                    .items_center()
                    .p_1()
                    .when(!compact, |row| row.w(gpui_kit::rems(12.0)))
                    .child(version),
            );
        let bar = bar
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .p_1()
                    .child(self.status_destination(
                        "status-subscription",
                        subscription_label,
                        subscription_detail,
                        Page::Subscriptions,
                        cx,
                    ))
                    .child(self.status_destination(
                        "status-core",
                        core_label,
                        core_detail,
                        Page::Cores,
                        cx,
                    ))
                    .child(self.status_destination(
                        "status-node",
                        node.label().into(),
                        node.detail().into(),
                        Page::Proxies,
                        cx,
                    )),
            )
            .right(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .p_1()
                    .child(self.render_shortcuts(cx))
                    .child(
                        Button::new("core-status")
                            .accessibility_id("core-status")
                            .accessibility_label(detail.clone())
                            .small()
                            .ghost()
                            .icon(if self.core_error.is_some() || self.core_recovery_needed {
                                IconName::CircleAlert
                            } else if self.core_running {
                                IconName::CircleCheck
                            } else {
                                IconName::Circle
                            })
                            .label(state)
                            .text_color(state_color)
                            .tooltip(detail.clone())
                            .disabled(self.closing)
                            .on_click(move |_, window, cx| {
                                let detail = detail.clone();
                                window.open_dialog(cx, move |dialog, _, _| {
                                    dialog.title("运行状态").child(
                                        div()
                                            .id("desktop-session-details")
                                            .accessibility_id("desktop-session-details")
                                            .role(Role::Group)
                                            .aria_label(detail.clone())
                                            .text_sm()
                                            .child(detail.clone()),
                                    )
                                });
                            }),
                    )
                    .child(self.session_command(operation_busy, reason, cx)),
            );
        div()
            .id("desktop-status-bar")
            .accessibility_id("desktop-status-bar")
            .role(Role::Group)
            .aria_label("应用状态栏")
            .track_focus(&self.status_focus)
            .when(self.status_focus.is_focused(window), |element| {
                element.focus_ring_style(window, cx)
            })
            .flex_shrink_0()
            .min_w_0()
            .bg(cx.theme().background.opacity(1.0))
            .child(bar)
    }

    fn start_block_reason(
        &self,
        operation_busy: bool,
        dirty: bool,
        cx: &Context<Self>,
    ) -> Option<&'static str> {
        if dirty {
            Some("保存或放弃未保存修改后可启动")
        } else if !self.core.is_configured() {
            Some("请先在核心页选择可用核心")
        } else if let Some(reason) = self.configuration.read(cx).start_block_reason() {
            Some(reason)
        } else if self.distribution.read(cx).is_pending() {
            Some("完成待处理的迁移或更新后可启动")
        } else if operation_busy {
            Some("等待当前操作完成")
        } else {
            None
        }
    }

    fn session_command(
        &self,
        operation_busy: bool,
        reason: Option<&'static str>,
        cx: &Context<Self>,
    ) -> Button {
        if self.core_running || self.core_recovery_needed {
            Button::new("core-stop")
                .accessibility_id("core-stop")
                .small()
                .outline()
                .icon(IconName::Square)
                .label(if self.core_recovery_needed && !self.core_running {
                    "恢复自有控制"
                } else {
                    "停止核心"
                })
                .loading(matches!(self.core_busy, Some(CoreOperationKind::Stop)))
                .disabled(operation_busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.run_core_operation(CoreOperationKind::Stop, window, cx)
                }))
        } else {
            Button::new("core-start")
                .accessibility_id("core-start")
                .small()
                .outline()
                .icon(IconName::Play)
                .label("启动核心")
                .loading(matches!(self.core_busy, Some(CoreOperationKind::Start)))
                .disabled(operation_busy || reason.is_some())
                .tooltip(reason.unwrap_or("启动已保存的配置"))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.run_core_operation(CoreOperationKind::Start, window, cx)
                }))
        }
    }

    fn status_destination(
        &self,
        id: &'static str,
        label: String,
        detail: String,
        page: Page,
        cx: &Context<Self>,
    ) -> Button {
        Button::new(id)
            .accessibility_id(id)
            .small()
            .ghost()
            .icon(page.presentation().1)
            .accessibility_label(detail.clone())
            .tooltip(detail)
            .min_w_0()
            .flex_1()
            .child(div().min_w_0().flex_1().truncate().child(label))
            .disabled(self.closing)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.set_page(page, cx);
                this.content_focus.focus(window, cx);
            }))
    }
}
