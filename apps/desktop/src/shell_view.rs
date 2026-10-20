use gpui_kit::component::{ActiveTheme as _, Disableable as _, Selectable as _, button::Button};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

use crate::{CoreOperationKind, DesktopView, Page};

impl Render for DesktopView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let operation_busy = self.closing
            || self.core_busy.is_some()
            || self.proxy.read(cx).busy()
            || self.configuration.read(cx).busy()
            || self.subscriptions.read(cx).busy()
            || self.profiles.read(cx).busy()
            || self.structured.read(cx).busy()
            || self.rules.read(cx).busy()
            || self.scripts.read(cx).busy();
        let core_status: gpui_kit::SharedString = self.core_status().into();
        let content = match self.page {
            Page::Subscriptions => self.subscriptions.clone().into_any_element(),
            Page::Profiles => self.profiles.clone().into_any_element(),
            Page::Structured => self.structured.clone().into_any_element(),
            Page::Rules => self.rules.clone().into_any_element(),
            Page::Configuration => self.configuration.clone().into_any_element(),
            Page::Proxies => self.proxy.clone().into_any_element(),
            Page::Connections => self.connections.clone().into_any_element(),
            Page::Stats => self.stats.clone().into_any_element(),
            Page::Logs => self.logs.clone().into_any_element(),
            Page::Scripts => self.scripts.clone().into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .p_4()
            .gap_3()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child("Lanpath 实验版（GPUI）")
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Windows 实验性并行版本；独立数据，不接管系统代理/TUN"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("page-subscriptions")
                            .label("订阅")
                            .selected(self.page == Page::Subscriptions)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(Page::Subscriptions, cx);
                            })),
                    )
                    .child(
                        Button::new("page-profiles")
                            .label("覆写")
                            .selected(self.page == Page::Profiles)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(Page::Profiles, cx);
                            })),
                    )
                    .child(
                        Button::new("page-structured")
                            .label("结构化配置")
                            .selected(self.page == Page::Structured)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(Page::Structured, cx);
                            })),
                    )
                    .child(
                        Button::new("page-rules")
                            .label("规则")
                            .selected(self.page == Page::Rules)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(Page::Rules, cx);
                            })),
                    )
                    .child(
                        Button::new("page-configuration")
                            .label("预览")
                            .selected(self.page == Page::Configuration)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(Page::Configuration, cx);
                            })),
                    )
                    .child(
                        Button::new("page-proxies")
                            .label("代理策略")
                            .selected(self.page == Page::Proxies)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(Page::Proxies, cx);
                            })),
                    )
                    .child(
                        Button::new("page-connections")
                            .label("连接")
                            .selected(self.page == Page::Connections)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(Page::Connections, cx);
                            })),
                    )
                    .child(
                        Button::new("page-stats")
                            .label("统计")
                            .selected(self.page == Page::Stats)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(Page::Stats, cx);
                            })),
                    )
                    .child(
                        Button::new("page-logs")
                            .label("日志")
                            .selected(self.page == Page::Logs)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(Page::Logs, cx);
                            })),
                    )
                    .child(
                        Button::new("page-scripts")
                            .label("脚本")
                            .selected(self.page == Page::Scripts)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(Page::Scripts, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("core-start")
                            .label("启动实验核心")
                            .disabled(
                                operation_busy
                                    || self.core_running
                                    || !self.core.is_configured()
                                    || !self.configuration.read(cx).ready_for_start()
                                    || self.structured.read(cx).dirty()
                                    || self.rules.read(cx).dirty()
                                    || self.scripts.read(cx).dirty(),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run_core_operation(CoreOperationKind::Start, window, cx);
                            })),
                    )
                    .child(
                        Button::new("core-stop")
                            .label("停止核心")
                            .disabled(operation_busy || !self.core_running)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run_core_operation(CoreOperationKind::Stop, window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .id("core-status")
                    .role(Role::Status)
                    .aria_label(core_status.clone())
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(core_status),
            )
            .child(content)
    }
}
