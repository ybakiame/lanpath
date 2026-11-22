//! 主题预览块：外观页网格与画廊共用的主题卡片。
//! 参考 Doona 缩略窗口色卡，在真实语义 token 上绘制微缩应用骨架（侧栏、卡片、
//! 正文条、主色胶囊与状态点），选中态同时提供边框强调、主题名与勾选标记。

use nsh_base::theme;
use nsh_core::prelude::*;

use super::theme::ThemeId;

/// 主题预览卡片。`applied` 响应式为真时显示勾选标记（选中主题同时
/// 有名称与勾选，不只靠边框——DESIGN 4.3）。
pub fn theme_preview(id: ThemeId, applied: impl Fn() -> bool + 'static) -> AnyElement {
    let applied = std::rc::Rc::new(applied);
    let applied_for_border = applied.clone();
    let applied_for_check = applied;
    let custom_sig = super::prefs::prefs().custom_palette;
    let tok = move |name: &'static str| {
        let tokens = if id == ThemeId::Custom {
            super::theme::custom_tokens(custom_sig.get())
        } else {
            super::theme::theme_tokens(id)
        };
        tokens.color(name).unwrap_or(AdaptiveColor::TRANSPARENT)
    };

    let dot = |name: &'static str| -> AnyElement {
        view()
            .size(px(7.0))
            .rounded_full()
            .border(px(1.0))
            .border_color(move || theme().border)
            .bg(move || tok(name))
            .into_element()
    };

    let mini_window = view()
        .w_full()
        .h(px(46.0))
        .flex_row()
        .overflow_hidden()
        .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
        .border(px(1.0))
        .border_color(move || tok("border"))
        .bg(move || tok("background"))
        .child(
            view()
                .w(px(24.0))
                .h_full()
                .flex_none()
                .flex_col()
                .gap(px(3.0))
                .p(px(4.0))
                .bg(move || tok("sidebar"))
                .border_r(px(1.0))
                .border_color(move || tok("border"))
                .child(
                    view()
                        .w_full()
                        .h(px(5.0))
                        .rounded_full()
                        .bg(move || tok("primary")),
                )
                .child(
                    view()
                        .w_full()
                        .h(px(4.0))
                        .rounded(px(2.0))
                        .bg(move || tok("accent")),
                )
                .child(
                    view()
                        .w(px(12.0))
                        .h(px(3.0))
                        .rounded(px(2.0))
                        .bg(move || tok("border")),
                ),
        )
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .flex_col()
                .justify_between()
                .p(px(5.0))
                .gap(px(4.0))
                .child(
                    view()
                        .w_full()
                        .flex_col()
                        .gap(px(3.0))
                        .p(px(4.0))
                        .rounded(px(3.0))
                        .border(px(1.0))
                        .border_color(move || tok("border"))
                        .bg(move || tok("card"))
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .gap(px(3.0))
                                .child(
                                    view()
                                        .w(px(32.0))
                                        .h(px(3.0))
                                        .rounded_full()
                                        .bg(move || tok("foreground")),
                                )
                                .child(
                                    view()
                                        .size(px(5.0))
                                        .rounded_full()
                                        .bg(move || tok("success")),
                                ),
                        )
                        .child(
                            view()
                                .w(px(22.0))
                                .h(px(2.5))
                                .rounded_full()
                                .bg(move || tok("muted-foreground")),
                        ),
                )
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(px(4.0))
                        .child(
                            view()
                                .w(px(20.0))
                                .h(px(6.0))
                                .rounded_full()
                                .bg(move || tok("primary")),
                        )
                        .child(
                            view()
                                .w(px(14.0))
                                .h(px(6.0))
                                .rounded_full()
                                .bg(move || tok("accent")),
                        ),
                ),
        );

    let label = id.id_name();
    view()
        .flex_col()
        .w_full()
        .gap_2()
        .p_2()
        .rounded(move || theme().radius)
        .bg(move || theme().card)
        .border(px(1.5))
        .border_color(move || {
            if applied_for_border() {
                theme().primary
            } else {
                theme().border
            }
        })
        .transition_colors()
        .duration(move || {
            super::motion::motion_duration(super::motion::INSTANT_MS, super::prefs().reduced.get())
        })
        .child(mini_window)
        .child(
            view()
                .flex_row()
                .gap_1()
                .items_center()
                .child(
                    text(label)
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(move || theme().foreground)
                        .truncate()
                        .flex_1()
                        .min_w(px(0.0)),
                )
                .child(
                    view()
                        .flex_row()
                        .gap(px(3.0))
                        .items_center()
                        .child(dot("primary"))
                        .child(dot("accent")),
                )
                .child(when(
                    move || applied_for_check(),
                    || {
                        text("✓")
                            .text_size(px(12.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(move || theme().primary)
                    },
                )),
        )
        .into_element()
}
