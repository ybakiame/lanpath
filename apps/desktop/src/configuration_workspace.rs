use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _,
    tab::{Tab, TabBar},
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use lanpath_common::CoreType;

use crate::{DesktopView, Page, config_slices_view::Section};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum StructuredSection {
    Network,
    Dns,
    Outbounds,
    RouteCache,
    Rules,
}

const SECTIONS: [StructuredSection; 5] = [
    StructuredSection::Network,
    StructuredSection::Dns,
    StructuredSection::Outbounds,
    StructuredSection::RouteCache,
    StructuredSection::Rules,
];

impl StructuredSection {
    fn slice(self) -> Option<Section> {
        match self {
            Self::Dns => Some(Section::Dns),
            Self::Outbounds => Some(Section::Outbounds),
            Self::RouteCache => Some(Section::RouteCache),
            _ => None,
        }
    }
}

pub(crate) fn configuration_title(kind: CoreType) -> &'static str {
    match kind {
        CoreType::SingBox => "sing-box 配置",
        CoreType::Mihomo => "Mihomo 配置",
    }
}

impl Page {
    pub(crate) fn visible_for(self, kind: CoreType) -> bool {
        match self {
            Self::System | Self::Rules => false,
            Self::Structured | Self::Profiles => kind == CoreType::SingBox,
            _ => true,
        }
    }
}

impl DesktopView {
    pub(crate) fn available_page(&self, page: Page) -> Page {
        let kind = self.core.selected_core_type();
        if page.visible_for(kind) {
            page
        } else if kind == CoreType::SingBox {
            Page::Structured
        } else {
            Page::Configuration
        }
    }

    pub(crate) fn reconcile_configuration_page(&mut self, cx: &mut Context<Self>) {
        let page = self.available_page(self.page);
        if page != self.page {
            self.set_page(page, cx);
        }
    }

    pub(crate) fn save_structured_section(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.configuration_section {
            StructuredSection::Network => self.system.update(cx, |view, cx| view.save(window, cx)),
            StructuredSection::Rules => self.rules.update(cx, |view, cx| view.save(window, cx)),
            _ => self.structured.update(cx, |view, cx| view.save(window, cx)),
        }
    }

    pub(crate) fn render_structured_workspace(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let compact = window.viewport_size().width < window.rem_size() * 64.0;
        let selected = SECTIONS
            .iter()
            .position(|section| *section == self.configuration_section)
            .unwrap_or(1);
        let content = match self.configuration_section {
            StructuredSection::Network => self.system.clone().into_any_element(),
            StructuredSection::Rules => self.rules.clone().into_any_element(),
            _ => self.structured.clone().into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap_3()
            .child(if compact {
                div()
                    .text_base()
                    .font_semibold()
                    .child("sing-box 结构化配置")
            } else {
                crate::design::page_header(
                    "sing-box 结构化配置",
                    "按分区编辑，保存后下次启动生效",
                    div(),
                    cx,
                )
            })
            .child(
                TabBar::new("structured-workspace-sections")
                    .segmented()
                    .selected_index(selected)
                    .children(
                        ["网络", "DNS", "出站", "路由与缓存", "规则"]
                            .map(|label| Tab::new().label(label)),
                    )
                    .on_click(cx.listener(|this, index: &usize, window, cx| {
                        let Some(section) = SECTIONS.get(*index).copied() else {
                            return;
                        };
                        // Flush/validate the slice form before leaving it, including
                        // for peer editors. The entity and its unsaved draft survive.
                        if let Some(slice) = section.slice().or(this.configuration_section.slice())
                            && !this
                                .structured
                                .update(cx, |view, cx| view.set_section(slice, window, cx))
                        {
                            return;
                        }
                        this.configuration_section = section;
                        cx.notify();
                    })),
            )
            .child(content)
    }

    pub(crate) fn render_mihomo_placeholder(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        div()
            .id("mihomo-configuration-placeholder")
            .accessibility_id("mihomo-configuration-placeholder")
            .role(Role::Group)
            .aria_label("Mihomo 配置预览占位")
            .flex()
            .flex_col()
            .gap_4()
            .min_w_0()
            .child(crate::design::page_header(
                "Mihomo 配置预览",
                "Mihomo 专属配置",
                div(),
                cx,
            ))
            .child(
                crate::design::surface(cx)
                    .p_5()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child("配置编辑正在开发")
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("此处暂保留预览入口。后续将提供独立的 Mihomo 配置与校验。"),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::PAGES;

    #[test]
    fn configuration_navigation_follows_selected_core_capabilities() {
        let sing_box: Vec<_> = PAGES
            .into_iter()
            .filter(|page| page.visible_for(CoreType::SingBox))
            .collect();
        let mihomo: Vec<_> = PAGES
            .into_iter()
            .filter(|page| page.visible_for(CoreType::Mihomo))
            .collect();
        assert_eq!(sing_box.len(), 17);
        assert_eq!(mihomo.len(), 15);
        assert!(sing_box.contains(&Page::Structured));
        assert!(!mihomo.contains(&Page::Structured));
        assert!(!mihomo.contains(&Page::Profiles));
        for pages in [sing_box, mihomo] {
            assert!(!pages.contains(&Page::System));
            assert!(!pages.contains(&Page::Rules));
            assert!(pages.contains(&Page::Configuration));
            assert!(pages.first() == Some(&Page::Subscriptions));
            assert!(pages.last() == Some(&Page::About));
        }
    }
}
