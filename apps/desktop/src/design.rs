//! Product presentation shared by desktop features. Behavior stays in GPUI Kit
//! controls and the feature entities; surfaces consume the active semantic theme.
mod disclosure;
pub(crate) use disclosure::help;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _};
use gpui_kit::{App, Div, IntoElement, ParentElement as _, SharedString, Styled as _, div};

pub(crate) fn page_header(
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    actions: impl IntoElement,
    cx: &App,
) -> Div {
    div()
        .flex()
        .items_center()
        .flex_wrap()
        .flex_shrink_0()
        .gap_4()
        .pb_2()
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap_1()
                .child(div().text_xl().font_semibold().child(title.into()))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(description.into()),
                ),
        )
        .child(actions)
}

pub(crate) fn toolbar() -> Div {
    div()
        .flex()
        .items_center()
        .flex_wrap()
        .flex_shrink_0()
        .gap_2()
}

pub(crate) fn surface(cx: &App) -> Div {
    div()
        .min_w_0()
        .border_1()
        .border_color(cx.theme().border)
        .rounded(cx.theme().radius_lg)
        .bg(cx.theme().group_box)
}

pub(crate) fn settings_section(
    title: &'static str,
    description: &'static str,
    content: impl IntoElement,
    cx: &App,
) -> Div {
    surface(cx)
        .flex()
        .flex_col()
        .flex_shrink_0()
        .p_5()
        .gap_4()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().font_semibold().child(title))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(description),
                ),
        )
        .child(content)
}

pub(crate) fn empty_state(
    icon: gpui_kit::assets::IconName,
    title: &'static str,
    description: &'static str,
    cx: &App,
) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .flex_1()
        .min_w_0()
        .gap_3()
        .p_8()
        .child(
            Icon::new(icon)
                .large()
                .text_color(cx.theme().muted_foreground),
        )
        .child(div().font_medium().child(title))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(description),
        )
}

pub(crate) fn metric(label: &'static str, value: impl Into<SharedString>, cx: &App) -> Div {
    surface(cx)
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap_1()
        .p_3()
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(div().text_lg().font_semibold().child(value.into()))
}

pub(crate) fn format_bytes(bytes: impl Into<i128>) -> String {
    let bytes = bytes.into() as f64;
    for (unit, divisor) in [
        ("TiB", 1_099_511_627_776.),
        ("GiB", 1_073_741_824.),
        ("MiB", 1_048_576.),
        ("KiB", 1024.),
    ] {
        if bytes.abs() >= divisor {
            return format!("{:.1} {unit}", bytes / divisor);
        }
    }
    format!("{bytes:.0} B")
}
