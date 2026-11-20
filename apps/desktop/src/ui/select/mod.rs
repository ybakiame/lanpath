//! 选择控件：Select/Switch/Checkbox/RadioRow/Segmented/Tabs/Menu 的语义薄封装，
//! 外观与动效全部来自语义 token，所有弹层共享同一浮层表面与进出场动效。

mod popup;

pub use popup::{
    menu_demo, popup_ending, popup_starting, popup_surface, styled_menu_item, styled_menu_separator,
};

use nsh_base::{
    checkbox as base_checkbox, radio as base_radio, radio_group as base_radio_group,
    select as base_select, tab, tabs, theme,
};
use nsh_core::prelude::*;

use super::motion;

/// 下拉选择（trigger 高 40；弹层 token 化并带进出场过渡）。
pub fn select(
    items: Vec<nsh_base::SelectItem>,
    value: impl Fn() -> Option<SharedString> + Copy + 'static,
    on_change: impl Fn(Option<SharedString>) + 'static,
) -> AnyElement {
    let trigger_items = items.clone();
    let trigger_value = value;
    let reduced = super::prefs::prefs().reduced;
    base_select()
        .items(items)
        .value(move || nsh_base::SelectValue::from(value()))
        .offset(px(4.0))
        .item_class("rounded-sm focus:bg-accent focus:text-accent-foreground selected:bg-secondary selected:text-secondary-foreground selected:font-medium disabled:opacity-50")
        .render_item(|item: &nsh_base::SelectEntry| {
            view()
                .h(px(32.0))
                .flex_row()
                .items_center()
                .px(px(8.0))
                .child(text(item.title.clone()).text_size(px(14.0)))
        })
        .trigger(
            view()
                .w_full()
                .h(px(36.0))
                .px(px(12.0))
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().input)
                .bg(move || theme().background)
                .cursor(Cursor::Default)
                .transition_colors()
                .duration(move || motion::motion_duration(motion::INSTANT_MS, reduced.get()))
                .hover(|s| {
                    s.bg(move || theme().accent.opacity(0.45))
                        .border_color(move || theme().ring)
                })
                .focus_visible(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
                .flex_row()
                .items_center()
                .justify_between()
                .gap_2()
                .child(text(move || {
                    let selected = trigger_value();
                    trigger_items
                        .iter()
                        .find(|item| Some(&item.value) == selected.as_ref())
                        .map(|item| item.title.to_string())
                        .unwrap_or_default()
                }))
                .child(super::icon::icon_sized(
                    super::icon::Icon::Chevron,
                    super::icon::Tone::Muted,
                    16.0,
                )),
        )
        .min_w(px(160.0))
        .text_color(move || theme().foreground)
        .text_size(px(14.0))
        .popup_style(popup_surface)
        .popup_starting_style(popup_starting)
        .on_change(move |chosen: &Option<SharedString>| on_change(chosen.clone()))
        .into_element()
}

/// 开关（track/thumb 颜色 token 化）。
pub fn switch(checked: Signal<bool>, on_change: impl Fn(bool) + 'static) -> AnyElement {
    switch_labeled(
        move || checked.get(),
        || SharedString::from(t!("common.switch")),
        on_change,
    )
}

/// 带可访问名称、checked 状态、焦点及 Enter/Space 键盘激活的开关。
pub fn switch_labeled(
    checked: impl Fn() -> bool + 'static,
    label: impl Fn() -> SharedString + 'static,
    on_change: impl Fn(bool) + 'static,
) -> AnyElement {
    let checked = std::rc::Rc::new(checked);
    let on_change = std::rc::Rc::new(on_change);
    let focused = signal(false);
    let reduced = super::prefs::prefs().reduced;
    let checked_for_state = checked.clone();
    let checked_for_track = checked.clone();
    let checked_for_thumb = checked.clone();
    let toggle = {
        let checked = checked.clone();
        let on_change = on_change.clone();
        std::rc::Rc::new(move || on_change(!checked()))
    };
    let press_toggle = toggle.clone();
    effect(move || {
        if !focused.get() {
            return;
        }
        for key in ["enter", "space"] {
            let toggle = toggle.clone();
            on_key_with(
                key,
                KeyOptions {
                    when: nsh_core::keymap::When::NotEditing,
                    ..KeyOptions::default()
                },
                move |_| toggle(),
            );
        }
    });
    view()
        .role(AccessibilityRole::Switch)
        .focusable(true)
        .on_focus(move || focused.set(true))
        .on_blur(move || focused.set(false))
        .accessibility_state(move || AccessibilityState {
            checked: Some(if checked_for_state() {
                nsh_core::Checked::True
            } else {
                nsh_core::Checked::False
            }),
            ..AccessibilityState::default()
        })
        .optional_label(move || Some(label()))
        .cursor(Cursor::Default)
        .rounded_full()
        .focus_visible(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .on_press(move || press_toggle())
        .child(
            view()
                .w(px(36.0))
                .h(px(20.0))
                .rounded_full()
                .p(px(2.0))
                .transition_colors()
                .duration(move || motion::motion_duration(motion::FAST_MS, reduced.get()))
                .bg(move || {
                    if checked_for_track() {
                        theme().primary
                    } else {
                        theme().input
                    }
                })
                .child(
                    view()
                        .size(px(16.0))
                        .rounded_full()
                        .shadow_sm()
                        .transition_transform()
                        .duration(move || motion::motion_duration(motion::FAST_MS, reduced.get()))
                        .translate_x(move || {
                            if checked_for_thumb() {
                                px(16.0)
                            } else {
                                px(0.0)
                            }
                        })
                        .bg(move || {
                            theme()
                                .color("switch-thumb")
                                .unwrap_or_else(|| theme().card)
                        }),
                ),
        )
        .into_element()
}

/// 复选框（标签在右，整行可点；自绘可见方框与勾选标记）。
pub fn checkbox(
    checked: Signal<bool>,
    label: impl Fn() -> String + 'static,
    on_change: impl Fn(bool) + 'static,
) -> AnyElement {
    let checked_for_indicator = checked;
    let checked_for_border = checked;
    let check_for_mark = checked;
    let reduced = super::prefs::prefs().reduced;
    let label = std::rc::Rc::new(label);
    let label_for_a11y = label.clone();
    base_checkbox()
        .flex_row()
        .checked(move || checked.get())
        .accessibility_label(move || Some(SharedString::from(label_for_a11y())))
        .items_center()
        .gap_2()
        .cursor(Cursor::Default)
        .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
        .focus_visible(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .hit_slop(EdgeInsets::all(px(4.0)))
        .indicator_style(move |s| {
            s.size(px(16.0))
                .items_center()
                .justify_center()
                .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
                .border(px(1.0))
                .transition_colors()
                .duration(move || motion::motion_duration(motion::INSTANT_MS, reduced.get()))
                .border_color(move || {
                    if checked_for_border.get() {
                        theme().primary
                    } else {
                        theme().input
                    }
                })
        })
        .indicator(
            view()
                .size_full()
                .items_center()
                .justify_center()
                .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
                .transition_colors()
                .duration(move || motion::motion_duration(motion::INSTANT_MS, reduced.get()))
                .bg(move || {
                    if checked_for_indicator.get() {
                        theme().primary
                    } else {
                        transparent_black().into()
                    }
                })
                .child(when(
                    move || check_for_mark.get(),
                    || {
                        text("✓")
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(move || theme().primary_foreground)
                    },
                )),
        )
        .on_change(move |next: &bool| on_change(*next))
        .child(
            text(move || label())
                .text_size(px(14.0))
                .text_color(move || theme().foreground),
        )
        .into_element()
}

/// A named radio group with one Tab stop and arrow-key selection.
pub fn radio_group(label: impl Fn() -> String + 'static, children: impl IntoElement) -> AnyElement {
    base_radio_group()
        .accessibility_label(move || Some(SharedString::from(label())))
        .flex_col()
        .gap_2()
        .child(children)
        .into_element()
}

/// 单选行（标签在右，整行可点；hover/pressed/focus-visible 与可见选中
/// 圆点；`test_id` 供测试与自动化）。
pub fn radio_row(
    checked: impl Fn() -> bool + 'static,
    label: impl Fn() -> String + 'static,
    test_id: &'static str,
    on_select: impl Fn() + 'static,
) -> AnyElement {
    let checked = std::rc::Rc::new(checked);
    let is_checked: std::rc::Rc<dyn Fn() -> bool> = std::rc::Rc::new(move || checked());
    let checked_for_prop = is_checked.clone();
    let checked_for_ring = is_checked.clone();
    let checked_for_dot = is_checked.clone();
    let reduced = super::prefs::prefs().reduced;
    base_radio()
        .test_id(test_id)
        .checked(move || checked_for_prop())
        .cursor(Cursor::Default)
        .min_h(px(40.0))
        .hit_slop(EdgeInsets::all(px(4.0)))
        .py(px(6.0))
        .px(px(8.0))
        .rounded(move || theme().radius)
        .transition_colors()
        .duration(move || motion::motion_duration(motion::INSTANT_MS, reduced.get()))
        .text_color(move || theme().foreground)
        .hover(|s| {
            s.bg(move || theme().accent)
                .text_color(move || theme().accent_foreground)
        })
        .active(|s| {
            s.bg(move || {
                theme()
                    .color("accent-active")
                    .unwrap_or_else(|| theme().accent)
            })
            .text_color(move || {
                theme()
                    .color("accent-active-foreground")
                    .unwrap_or_else(|| theme().accent_foreground)
            })
        })
        .focus_visible(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .on_change(move |_: &bool| on_select())
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    view()
                        .size(px(16.0))
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .border(px(1.5))
                        .border_color(move || {
                            if checked_for_ring() {
                                theme().primary
                            } else {
                                theme().input
                            }
                        })
                        .bg(move || theme().card)
                        .child(when(
                            move || checked_for_dot(),
                            || {
                                view()
                                    .size(px(8.0))
                                    .rounded_full()
                                    .bg(move || theme().primary)
                            },
                        )),
                )
                .child(text(label).text_size(px(14.0))),
        )
        .into_element()
}

/// 标签页容器（`tabs_list` 为 (test_id, 标签 i18n key)；原生滑动指示块 +
/// 同心圆角 + token 化文字层级）。
pub fn tab_bar(tabs_list: Vec<(&'static str, &'static str)>, active: Signal<usize>) -> AnyElement {
    let reduced = super::prefs::prefs().reduced;
    tabs()
        .flex_row()
        .self_start()
        .gap_1()
        .p(px(3.0))
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .bg(move || theme().muted)
        .indicator_style(|s| {
            s.top(px(3.0))
                .bottom(px(3.0))
                .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
                .bg(move || theme().card)
                .shadow_sm()
        })
        .children(
            tabs_list
                .into_iter()
                .enumerate()
                .map(move |(index, (test_id, label_key))| {
                    let selected = move || active.get() == index;
                    let el = tab()
                        .selected(selected)
                        .min_h(px(28.0))
                        .px(px(12.0))
                        .py(px(4.0))
                        .items_center()
                        .justify_center()
                        .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
                        .cursor(Cursor::Default)
                        .transition_colors()
                        .duration(move || {
                            motion::motion_duration(motion::INSTANT_MS, reduced.get())
                        })
                        .hover(move |s| s.text_color(move || theme().foreground))
                        .focus_visible(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
                        .on_press(move || active.set(index))
                        .child(
                            text(move || t!(label_key))
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(move || {
                                    if selected() {
                                        theme().foreground
                                    } else {
                                        theme().muted_foreground
                                    }
                                }),
                        )
                        .into_element();
                    view().test_id(test_id).child(el).into_element()
                }),
        )
        .into_element()
}

/// 分段单选外轮廓轨道（一个整体轮廓，内项用同心 radius-sm；总高 ≤ 38pt）。
pub fn segmented_group(test_id: &'static str, children: Vec<AnyElement>) -> AnyElement {
    view()
        .test_id(test_id)
        .self_start()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(2.0))
        .p(px(3.0))
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .bg(move || theme().muted)
        .children(children)
        .into_element()
}

/// Compact segmented radio; native radio semantics retain arrows and one Tab stop.
pub fn segmented_radio(
    checked: impl Fn() -> bool + 'static,
    label: impl Fn() -> String + 'static,
    test_id: impl Into<String>,
    on_select: impl Fn() + 'static,
) -> AnyElement {
    let test_id = test_id.into();
    let checked = std::rc::Rc::new(checked);
    let prop = checked.clone();
    let fill = checked.clone();
    let foreground = checked;
    let reduced = super::prefs::prefs().reduced;
    base_radio()
        .test_id(&test_id)
        .checked(move || prop())
        .cursor(Cursor::Default)
        .min_h(px(30.0))
        .px_3()
        .py_1()
        .items_center()
        .justify_center()
        .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
        .transition_colors()
        .duration(move || motion::motion_duration(motion::INSTANT_MS, reduced.get()))
        .bg(move || {
            if fill() {
                theme().card
            } else {
                transparent_black().into()
            }
        })
        .text_color(move || {
            if foreground() {
                theme().foreground
            } else {
                theme().muted_foreground
            }
        })
        .hover(|s| {
            s.bg(move || theme().accent.opacity(0.6))
                .text_color(move || theme().foreground)
        })
        .active(|s| {
            s.bg(move || {
                theme()
                    .color("accent-active")
                    .unwrap_or_else(|| theme().accent)
            })
        })
        .focus_visible(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .on_change(move |next: &bool| {
            if *next {
                on_select();
            }
        })
        .child(
            text(label)
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM),
        )
        .into_element()
}
