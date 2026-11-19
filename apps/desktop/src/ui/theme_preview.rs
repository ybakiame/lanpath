//! 主题预览块：外观页网格与画廊共用的主题卡片。样点为语义 token 的
//! 真实投影；选中态同时给名称与勾选（不只靠边框）。

use nsh_base::theme;
use nsh_core::prelude::*;

use super::theme::ThemeId;

/// 主题预览卡片。`applied` 响应式为真时显示勾选标记（选中主题同时
/// 有名称与勾选，不只靠边框——DESIGN 4.3）。
pub fn theme_preview(id: ThemeId, applied: impl Fn() -> bool + 'static) -> AnyElement {
    let applied = std::rc::Rc::new(applied);
    let applied_for_border = applied.clone();
    let applied_for_check = applied;
    let tokens = super::theme::theme_tokens(id);
    let dot = |name: &str| -> AnyElement {
        let color = tokens.color(name).unwrap_or(AdaptiveColor::TRANSPARENT);
        view()
            .size(px(12.0))
            .rounded_full()
            .border(px(1.0))
            .border_color(move || theme().border)
            .bg(color)
            .into_element()
    };
    let label = id.id_name();
    view()
        .flex_col()
        .w_full()
        .gap_2()
        .p_3()
        .rounded(move || theme().radius)
        .bg(move || theme().card)
        .border(px(1.0))
        .border_color(move || {
            if applied_for_border() {
                theme().primary
            } else {
                theme().border
            }
        })
        .child(
            view()
                .flex_row()
                .gap_1()
                .items_center()
                .child(
                    text("Aa")
                        .text_size(px(15.0))
                        .font_weight(FontWeight::MEDIUM),
                )
                .child(view().flex_1())
                .child(when(
                    move || applied_for_check(),
                    || text("✓").text_size(px(13.0)),
                ))
                .text_color(move || theme().foreground),
        )
        .child(
            view()
                .flex_row()
                .gap_1()
                .child(dot("background"))
                .child(dot("card"))
                .child(dot("foreground"))
                .child(dot("primary"))
                .child(dot("accent"))
                .child(dot("muted"))
                .child(dot("border")),
        )
        .child(
            text(label)
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || theme().foreground),
        )
        .into_element()
}
