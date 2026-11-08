use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _,
    scroll::ScrollableElement as _,
    sidebar::{SidebarHeader, SidebarItem as _},
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, div,
};

use crate::DesktopView;

impl DesktopView {
    pub(super) fn render_sidebar(
        &self,
        window: &mut gpui_kit::Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        // Bounded destinations do not need virtualization. A regular
        // scroll owner also permits group reflow and focus autoscroll in one
        // accessibility pass (the virtual Sidebar retries prepaint on reveal).
        div()
            .id("desktop-navigation")
            .role(Role::Navigation)
            .aria_label("主导航")
            .flex()
            .flex_col()
            .h_full()
            .min_h_0()
            .flex_shrink_0()
            .w(gpui_kit::rems(13.5))
            .min_w(gpui_kit::rems(13.5))
            .max_w(gpui_kit::rems(13.5))
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            .child(
                div().px_3().pt_3().child(
                    SidebarHeader::new().child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .py_3()
                            .child(gpui_kit::img("brand/lanpath.svg").size_9())
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .min_w_0()
                                    .child(div().font_semibold().child("岚径 · Lanpath"))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(crate::product::SLOGAN),
                                    ),
                            ),
                    ),
                ),
            )
            .child(
                div()
                    .id("desktop-navigation-scroll")
                    .accessibility_id("desktop-navigation-scroll")
                    .role(Role::Group)
                    .aria_label("导航项目")
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .size_full()
                            .px_3()
                            .py_3()
                            .overflow_y_scrollbar()
                            .child(
                                crate::keyboard::NavigationMenu::new(self, cx)
                                    .render("navigation-items", window, cx)
                                    .into_any_element(),
                            ),
                    ),
            )
    }
}
