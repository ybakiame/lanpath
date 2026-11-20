//! 弹出浮层与菜单共享样式：Select / Menu / Popover 共用边框、阴影与进出场过渡。

use nsh_base::{menu, menu_item, menu_separator, theme};
use nsh_core::element::StyleRefinement;
use nsh_core::prelude::*;

use crate::ui::{motion, prefs};

/// 所有弹出菜单、下拉列表与 Popover 共用的浮层表面样式。
pub fn popup_surface(s: StyleRefinement) -> StyleRefinement {
    let reduced = prefs::prefs().reduced;
    s.p(px(4.0))
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .bg(move || theme().popover)
        .text_color(move || theme().popover_foreground)
        .shadow_lg()
        .transition_all()
        .duration(move || motion::motion_duration(motion::FAST_MS, reduced.get()))
}

/// 弹层进入首帧样式（opacity 0 + 0.97 缩放 + 4pt 位移；减少动效时原地显现）。
pub fn popup_starting(s: StyleRefinement) -> StyleRefinement {
    let reduced = prefs::prefs().reduced;
    s.opacity(0.0)
        .scale(move || if reduced.get() { 1.0_f32 } else { 0.97_f32 })
        .translate_y(move || {
            px(if reduced.get() {
                0.0
            } else {
                -motion::DISTANCE_POPOVER_PT
            })
        })
}

/// 弹层退出末帧样式（快速收起；减少动效时即时消失）。
pub fn popup_ending(s: StyleRefinement) -> StyleRefinement {
    let reduced = prefs::prefs().reduced;
    s.opacity(0.0)
        .scale(move || if reduced.get() { 1.0_f32 } else { 0.98_f32 })
        .translate_y(move || {
            px(if reduced.get() {
                0.0
            } else {
                -motion::DISTANCE_POPOVER_PT * 0.5
            })
        })
}

/// 统一样式的菜单项（普通或危险语义）。
pub fn styled_menu_item(
    label: impl Fn() -> String + 'static,
    destructive: bool,
    on_select: impl Fn() + 'static,
) -> impl IntoElement {
    let reduced = prefs::prefs().reduced;
    menu_item()
        .px_3()
        .py_2()
        .min_h(px(32.0))
        .cursor(Cursor::Default)
        .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
        .transition_colors()
        .duration(move || motion::motion_duration(motion::INSTANT_MS, reduced.get()))
        .text_color(move || {
            if destructive {
                theme().destructive
            } else {
                theme().popover_foreground
            }
        })
        .hover(move |s| {
            s.bg(move || {
                if destructive {
                    theme()
                        .color("danger-surface")
                        .unwrap_or_else(|| theme().accent)
                } else {
                    theme().accent
                }
            })
            .text_color(move || {
                if destructive {
                    theme().destructive
                } else {
                    theme().accent_foreground
                }
            })
        })
        .focus(move |s| {
            s.bg(move || {
                if destructive {
                    theme()
                        .color("danger-surface")
                        .unwrap_or_else(|| theme().accent)
                } else {
                    theme().accent
                }
            })
        })
        .active(|s| {
            s.bg(move || {
                theme()
                    .color("accent-active")
                    .unwrap_or_else(|| theme().accent)
            })
        })
        .on_select(on_select)
        .child(text(label).text_size(px(13.0)))
}

/// 统一样式的菜单分隔线（1pt 高度 + border 语义色）。
pub fn styled_menu_separator() -> impl IntoElement {
    menu_separator()
        .h(px(1.0))
        .my_1()
        .bg(move || theme().border)
}

/// 菜单演示入口：触发按钮 + 统一浮层与项样式的菜单内容（画廊用）。
pub fn menu_demo(trigger: impl IntoElement) -> AnyElement {
    menu()
        .offset(px(4.0))
        .popup_style(|s| popup_surface(s).min_w(px(184.0)))
        .popup_starting_style(popup_starting)
        .popup_ending_style(popup_ending)
        .trigger(trigger)
        .child(styled_menu_item(
            || t!("gallery.menu.item_one"),
            false,
            || {},
        ))
        .child(styled_menu_item(
            || t!("gallery.menu.item_two"),
            false,
            || {},
        ))
        .child(styled_menu_separator())
        .child(styled_menu_item(|| t!("gallery.menu.delete"), true, || {}))
        .into_element()
}
