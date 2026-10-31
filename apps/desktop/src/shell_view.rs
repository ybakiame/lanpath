use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _, ThemeStyled as _, TitleBar,
    button::{Button, ButtonVariants as _},
    sidebar::{Sidebar, SidebarFooter, SidebarHeader},
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _,
};

use crate::{CoreOperationKind, DesktopView, Page};

impl Page {
    pub(crate) fn presentation(self) -> (&'static str, IconName) {
        match self {
            Self::Subscriptions => ("订阅", IconName::Rss),
            Self::Profiles => ("覆写", IconName::Layers),
            Self::Structured => ("结构化配置", IconName::SlidersHorizontal),
            Self::Rules => ("规则", IconName::ListFilter),
            Self::Configuration => ("预览", IconName::FileCode),
            Self::Proxies => ("代理策略", IconName::Waypoints),
            Self::Connections => ("连接", IconName::Network),
            Self::Stats => ("统计", IconName::ChartNoAxesCombined),
            Self::Logs => ("日志", IconName::ScrollText),
            Self::Scripts => ("脚本", IconName::Code),
            Self::Mitm => ("MITM", IconName::Shield),
            Self::System => ("系统网络", IconName::Settings),
            Self::Requests => ("网络请求", IconName::Globe),
            Self::Appearance => ("外观", IconName::Palette),
            Self::Cores => ("核心", IconName::Cpu),
            Self::Diagnostics => ("诊断", IconName::Activity),
            Self::Backup => ("备份", IconName::Archive),
            Self::Distribution => ("迁移与更新", IconName::Package),
            Self::About => ("关于", IconName::Info),
        }
    }
}

impl Render for DesktopView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let operation_busy = self.closing
            || self.core_busy.is_some()
            || self.proxy.read(cx).busy()
            || self.configuration.read(cx).busy()
            || self.subscriptions.read(cx).busy()
            || self.profiles.read(cx).busy()
            || self.structured.read(cx).busy()
            || self.rules.read(cx).busy()
            || self.scripts.read(cx).lifecycle_busy()
            || self.mitm.read(cx).busy()
            || self.requests.read(cx).busy()
            || self.cores.read(cx).busy()
            || self.diagnostics.read(cx).busy()
            || self.backups.read(cx).busy()
            || self.backups.read(cx).is_pending()
            || self.system.read(cx).busy()
            || self.distribution.read(cx).busy();
        let dirty = self.structured.read(cx).dirty()
            || self.rules.read(cx).dirty()
            || self.scripts.read(cx).dirty()
            || self.mitm.read(cx).dirty()
            || self.requests.read(cx).dirty()
            || self.system.read(cx).dirty()
            || self.distribution.read(cx).dirty();
        let core_status: gpui_kit::SharedString =
            format!("{} · {}", self.core.selected_core_name(), self.core.port()).into();
        let (page_title, _) = self.page.presentation();
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
            Page::System => self.system.clone().into_any_element(),
            Page::Appearance => self.appearance.clone().into_any_element(),
            Page::Cores => self.cores.clone().into_any_element(),
            Page::Diagnostics => self.diagnostics.clone().into_any_element(),
            Page::Backup => self.backups.clone().into_any_element(),
            Page::Distribution => self.distribution.clone().into_any_element(),
            Page::About => self.about.clone().into_any_element(),
        };
        let sidebar = Sidebar::new("desktop-navigation")
            .collapsible(false)
            .h_full()
            .min_h_0()
            .flex_shrink_0()
            .w(gpui_kit::rems(14.0))
            .min_w(gpui_kit::rems(14.0))
            .max_w(gpui_kit::rems(14.0))
            .header(
                SidebarHeader::new().child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .gap_1()
                        .child(div().font_semibold().child(crate::product::DISPLAY_NAME))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("独立桌面客户端"),
                        ),
                ),
            )
            .footer(
                SidebarFooter::new().child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .text_xs()
                        .whitespace_normal()
                        .text_color(cx.theme().muted_foreground)
                        .child(concat!("版本 ", env!("CARGO_PKG_VERSION")))
                        .child("独立数据 · 与 Tauri 并行")
                        .child("F6 / Shift+F6 切换区域")
                        .child("导航 ↑↓ · Enter 打开")
                        .child("Ctrl+S 保存当前表单"),
                ),
            )
            .child(crate::keyboard::NavigationMenu::new(self, cx));

        div()
            .id("desktop-workspace")
            .track_focus(&self.focus_handle)
            .key_context("Desktop")
            .on_action(
                cx.listener(|this, _: &crate::keyboard::CycleRegion, window, cx| {
                    this.cycle_region(window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::keyboard::SaveCurrentForm, window, cx| {
                    this.save_current_form(window, cx)
                }),
            )
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                TitleBar::new().child(
                    div()
                        .flex_shrink_0()
                        .text_sm()
                        .font_semibold()
                        .child(crate::product::DISPLAY_NAME),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_stretch()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    // A hidden Windows HWND can initially report non-positive geometry.
                    // Sidebar caches its virtual-list overdraw from the first viewport;
                    // mount it only after the native size notification supplies real bounds.
                    .children(
                        (window.viewport_size().height > gpui_kit::px(0.0)).then_some(sidebar),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .flex_wrap()
                                    .flex_shrink_0()
                                    .gap_2()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(cx.theme().border)
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .flex_1()
                                            .min_w_0()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .id("core-status")
                                                    .accessibility_id("core-status")
                                                    .role(Role::Status)
                                                    .aria_label(core_status.clone())
                                                    .text_sm()
                                                    .min_w_0()
                                                    .whitespace_normal()
                                                    .text_color(if self.core_error.is_some() {
                                                        cx.theme().danger
                                                    } else {
                                                        cx.theme().muted_foreground
                                                    })
                                                    .child(core_status),
                                            )
                                            .children((dirty && !self.core_running).then(|| {
                                                div()
                                                    .text_sm()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child("有未保存修改；保存或放弃后才能启动核心")
                                            })),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .flex_shrink_0()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                Button::new("core-start")
                                                    .accessibility_id("core-start")
                                                    .small()
                                                    .icon(IconName::Play)
                                                    .label("启动核心")
                                                    .disabled(
                                                        operation_busy
                                                            || self.core_running
                                                            || self.core_recovery_needed
                                                            || self
                                                                .distribution
                                                                .read(cx)
                                                                .is_pending()
                                                            || !self.core.is_configured()
                                                            || !self
                                                                .configuration
                                                                .read(cx)
                                                                .ready_for_start()
                                                            || dirty,
                                                    )
                                                    .on_click(cx.listener(
                                                        |this, _, window, cx| {
                                                            this.run_core_operation(
                                                                CoreOperationKind::Start,
                                                                window,
                                                                cx,
                                                            )
                                                        },
                                                    )),
                                            )
                                            .child(
                                                Button::new("core-stop")
                                                    .accessibility_id("core-stop")
                                                    .small()
                                                    .ghost()
                                                    .icon(IconName::Square)
                                                    .label(
                                                        if self.core_recovery_needed
                                                            && !self.core_running
                                                        {
                                                            "恢复自有控制"
                                                        } else {
                                                            "停止核心"
                                                        },
                                                    )
                                                    .disabled(
                                                        operation_busy
                                                            || (!self.core_running
                                                                && !self.core_recovery_needed),
                                                    )
                                                    .on_click(cx.listener(
                                                        |this, _, window, cx| {
                                                            this.run_core_operation(
                                                                CoreOperationKind::Stop,
                                                                window,
                                                                cx,
                                                            )
                                                        },
                                                    )),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .id("desktop-content")
                                    .track_focus(&self.content_focus)
                                    .aria_label(page_title)
                                    .when(self.content_focus.is_focused(window), |element| {
                                        element.focus_ring_style(window, cx)
                                    })
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w_0()
                                    .min_h_0()
                                    .p_4()
                                    .child(content),
                            ),
                    ),
            )
    }
}
