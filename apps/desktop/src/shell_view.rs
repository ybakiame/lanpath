use gpui_kit::component::{
    ActiveTheme as _, Disableable as _,
    button::Button,
    sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem},
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

use crate::{CoreOperationKind, DesktopView, Page};

impl DesktopView {
    fn navigation_group(
        &self,
        title: &'static str,
        pages: &[(Page, &'static str)],
        cx: &Context<Self>,
    ) -> SidebarGroup<SidebarMenu> {
        SidebarGroup::new(title).child(SidebarMenu::new().children(pages.iter().map(
            |&(page, label)| {
                SidebarMenuItem::new(label)
                    .active(self.page == page)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_page(page, cx)))
            },
        )))
    }
}

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
            || self.scripts.read(cx).busy()
            || self.mitm.read(cx).busy()
            || self.requests.read(cx).busy();
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
            Page::Mitm => self.mitm.clone().into_any_element(),
            Page::Requests => self.requests.clone().into_any_element(),
            Page::Appearance => self.appearance.clone().into_any_element(),
        };
        let sidebar = Sidebar::new("desktop-navigation")
            .collapsible(false)
            .w_48()
            .child(self.navigation_group(
                "配置",
                &[
                    (Page::Subscriptions, "订阅"),
                    (Page::Profiles, "覆写"),
                    (Page::Structured, "结构化配置"),
                    (Page::Rules, "规则"),
                    (Page::Configuration, "预览"),
                ],
                cx,
            ))
            .child(self.navigation_group(
                "运行",
                &[
                    (Page::Proxies, "代理策略"),
                    (Page::Connections, "连接"),
                    (Page::Stats, "统计"),
                    (Page::Logs, "日志"),
                ],
                cx,
            ))
            .child(self.navigation_group(
                "扩展",
                &[(Page::Scripts, "脚本"), (Page::Mitm, "MITM")],
                cx,
            ))
            .child(self.navigation_group(
                "设置",
                &[(Page::Requests, "网络请求"), (Page::Appearance, "外观")],
                cx,
            ));

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p_4()
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
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(sidebar)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .p_4()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        Button::new("core-start")
                                            .label("启动实验核心")
                                            .disabled(
                                                operation_busy
                                                    || self.core_running
                                                    || !self.core.is_configured()
                                                    || !self
                                                        .configuration
                                                        .read(cx)
                                                        .ready_for_start()
                                                    || self.structured.read(cx).dirty()
                                                    || self.rules.read(cx).dirty()
                                                    || self.scripts.read(cx).dirty()
                                                    || self.mitm.read(cx).dirty()
                                                    || self.requests.read(cx).dirty(),
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.run_core_operation(
                                                    CoreOperationKind::Start,
                                                    window,
                                                    cx,
                                                )
                                            })),
                                    )
                                    .child(
                                        Button::new("core-stop")
                                            .label("停止核心")
                                            .disabled(operation_busy || !self.core_running)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.run_core_operation(
                                                    CoreOperationKind::Stop,
                                                    window,
                                                    cx,
                                                )
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
                            .child(content),
                    ),
            )
    }
}
