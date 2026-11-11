use gpui_kit::component::{ActiveTheme as _, scroll::ScrollableElement as _};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

pub(crate) struct AboutView;

impl Render for AboutView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let information = |id: &'static str, value: SharedString| {
            div()
                .id(id)
                .accessibility_id(id)
                .role(Role::Group)
                .aria_label(value.clone())
                .text_sm()
                .child(value)
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scrollbar()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_shrink_0()
                    .min_w_0()
                    .w_full()
                    .max_w(gpui_kit::rems(64.0))
                    .gap_4()
                    .child(crate::design::page_header(
                        "关于岚径",
                        "Lanpath · 原生 GPUI 实验版",
                        div(),
                        cx,
                    ))
                    .child(
                        crate::design::surface(cx)
                            .flex()
                            .flex_col()
                            .gap_3()
                            .p_4()
                            .child("原生 GPUI 桌面客户端")
                            .child(information(
                                "about-app-version",
                                concat!("应用版本：", env!("CARGO_PKG_VERSION")).into(),
                            ))
                            .child(information(
                                "about-platform",
                                format!(
                                    "操作系统 / 架构：{} / {}",
                                    std::env::consts::OS,
                                    std::env::consts::ARCH
                                )
                                .into(),
                            ))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("程序身份：{}", crate::product::APP_ID)),
                            ),
                    ),
            )
    }
}
