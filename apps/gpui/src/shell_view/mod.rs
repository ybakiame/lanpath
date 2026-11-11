use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{ActiveTheme as _, ThemeStyled as _, TitleBar, WindowExt as _};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _,
};

use crate::{DesktopView, Page};

mod chrome;
mod shortcuts;
mod status_bar;

impl Page {
    pub(crate) fn group_index(self) -> usize {
        match self {
            Self::Subscriptions | Self::Proxies | Self::Connections | Self::Stats => 0,
            Self::Profiles
            | Self::Rules
            | Self::Structured
            | Self::Configuration
            | Self::System => 1,
            Self::Logs | Self::Diagnostics | Self::Scripts | Self::Mitm => 2,
            _ => 3,
        }
    }

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
        let (page_title, _) = self.page.presentation();
        let content = match self.page {
            Page::Subscriptions => self.subscriptions.clone().into_any_element(),
            Page::Profiles => self.profiles.clone().into_any_element(),
            Page::Structured => self
                .render_structured_workspace(window, cx)
                .into_any_element(),
            Page::Rules => self.rules.clone().into_any_element(),
            Page::Configuration
                if self.core.selected_core_type() == lanpath_common::CoreType::Mihomo =>
            {
                self.render_mihomo_placeholder(cx).into_any_element()
            }
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
        let sidebar = self.render_sidebar(window, cx);

        div()
            .id("desktop-workspace")
            .track_focus(&self.focus_handle)
            .key_context("Desktop")
            .on_action(
                cx.listener(|this, _: &crate::keyboard::CycleRegion, window, cx| {
                    this.cycle_region(false, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::keyboard::PreviousRegion, window, cx| {
                    this.cycle_region(true, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::keyboard::OpenAppearance, window, cx| {
                    if !window.has_active_dialog(cx) && !window.has_active_sheet(cx) {
                        this.set_page(Page::Appearance, cx);
                        this.content_focus.focus(window, cx);
                    }
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
            .text_color(cx.theme().foreground)
            .child(
                TitleBar::new().child(
                    div()
                        .flex_shrink_0()
                        .text_sm()
                        .font_semibold()
                        .child(crate::product::window_title()),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_stretch()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(sidebar)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            // Keep text contrast independent of the desktop
                            // behind the acrylic navigation and title bar.
                            .bg(cx.theme().background.opacity(1.0))
                            .child(
                                div()
                                    .id("desktop-content")
                                    .accessibility_id("desktop-content")
                                    .role(Role::Group)
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
                                    .p_5()
                                    .text_sm()
                                    .child(content),
                            ),
                    ),
            )
            .child(self.render_status_bar(operation_busy, dirty, window, cx))
    }
}
