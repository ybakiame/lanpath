//! 与工作区一体的标题栏：参考官方 NSH Gallery，将侧栏收缩按钮置于右侧内容区标题左端，
//! 同时保留 Lanpath 应用图标；Windows 保留原生窗口按钮、贴靠和双击行为。

use nsh_base::theme;
use nsh_core::element::ImageSrc;
use nsh_core::prelude::*;

use super::icon::{Icon, Tone, icon_sized};

pub const TITLE_BAR_HEIGHT: f32 = 44.0;

pub fn title_bar() -> AnyElement {
    render_title_bar(|| t!("gallery.title"), false)
}

pub fn workspace_title_bar(title: impl Fn() -> String + 'static) -> AnyElement {
    render_title_bar(title, true)
}

fn render_title_bar(title: impl Fn() -> String + 'static, show_sidebar_toggle: bool) -> AnyElement {
    #[cfg(target_os = "windows")]
    {
        use nsh_core::desktop::{TitleBarOptions, TitleBarStyle, WindowAttributes};
        use nsh_core::element::desktop::main_window;
        let mut bar = TitleBarOptions::default();
        bar.style = TitleBarStyle::Hidden;
        bar.height = Some(px(TITLE_BAR_HEIGHT));
        let mut attributes = WindowAttributes::default();
        attributes.title_bar = Some(bar);
        main_window().set(attributes);
    }

    let source = ImageSrc::svg_markup(include_str!("../../../../assets/brand/lanpath.svg"));
    #[allow(unused_mut)]
    let mut bar = view()
        .test_id("app-title-bar")
        .h(px(TITLE_BAR_HEIGHT))
        .flex_none()
        .flex_row()
        .items_center()
        .bg(super::prefs::window_bg)
        .text_color(move || theme().muted_foreground)
        .border_b(px(1.0))
        .border_color(move || theme().border)
        .window_drag(true);

    #[cfg(target_os = "windows")]
    {
        bar = bar.pr(move || nsh_core::element::desktop::main_window().title_bar().right);
    }

    let expanded = move || show_sidebar_toggle && crate::shell::sidebar_shown();

    // 左侧品牌锚点：侧栏展开时与侧栏同宽（216pt）并延续侧栏底色；
    // 侧栏收起时紧缩为图标徽记，保留应用身份且不浪费水平空间。
    let brand_slot = view()
        .h_full()
        .flex_none()
        .flex_row()
        .items_center()
        .gap_2()
        .style_with(move |s| {
            if expanded() {
                s.w(px(crate::shell::SIDEBAR_WIDTH))
                    .px_4()
                    .bg(super::prefs::sidebar_bg)
                    .border_r(px(1.0))
                    .border_color(move || theme().sidebar_border)
            } else {
                s.pl_3().pr_2()
            }
        })
        .child(
            svg(source)
                .size(px(22.0))
                .flex_none()
                .accessibility_elements_hidden(true),
        )
        .child(when(
            move || expanded() || !show_sidebar_toggle,
            || {
                text("Lanpath")
                    .text_size(px(15.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(move || theme().foreground)
            },
        ));

    // 右侧标题栏内容区：参考官方画廊，收缩侧栏按钮位于此处最左端，后接当前页面名称与版本标记。
    let mut content_strip = view()
        .flex_1()
        .min_w(px(0.0))
        .h_full()
        .flex_row()
        .items_center()
        .gap_2()
        .px_3();

    if show_sidebar_toggle {
        content_strip = content_strip
            .child(
                view()
                    .window_drag(false)
                    .child(crate::shell::sidebar_toggle_button()),
            )
            .child(
                view()
                    .w(px(1.0))
                    .h(px(14.0))
                    .mx(px(2.0))
                    .bg(move || theme().border),
            );
    }

    content_strip = content_strip
        .child(
            text(title)
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || theme().foreground)
                .truncate(),
        )
        .child(view().flex_1())
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_1()
                .px_2()
                .py(px(2.0))
                .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
                .bg(move || theme().muted.opacity(0.75))
                .child(icon_sized(Icon::Sparkles, Tone::Primary, 12.0))
                .child(
                    text(concat!("v", env!("CARGO_PKG_VERSION")))
                        .text_size(px(11.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(move || theme().muted_foreground),
                ),
        );

    bar.child(brand_slot).child(content_strip).into_element()
}
