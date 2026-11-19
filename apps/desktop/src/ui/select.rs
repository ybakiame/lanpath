//! 选择控件：Select/Switch/Checkbox/RadioRow/Tabs 的语义薄封装，
//! 外观只来自 token。

use nsh_base::{
    checkbox as base_checkbox, menu, menu_item, menu_separator, radio as base_radio,
    radio_group as base_radio_group, select as base_select, tab, tabs, theme,
};
use nsh_core::prelude::*;

/// 下拉选择（trigger 高 40；弹层 token 化）。
pub fn select(
    items: Vec<nsh_base::SelectItem>,
    value: impl Fn() -> Option<SharedString> + Copy + 'static,
    on_change: impl Fn(Option<SharedString>) + 'static,
) -> AnyElement {
    let trigger_items = items.clone();
    let trigger_value = value;
    base_select()
        .items(items)
        .value(move || nsh_base::SelectValue::from(value()))
        .trigger(
            view()
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
                .child(text("▾")),
        )
        .min_w(px(160.0))
        .h(px(40.0))
        .px(px(12.0))
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().input)
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .text_size(px(14.0))
        .popup_style(move |s| {
            s.rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().border)
                .bg(move || theme().popover)
                .text_color(move || theme().popover_foreground)
        })
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
        .cursor_pointer()
        .focus_visible(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .on_press(move || press_toggle())
        .child(
            view()
                .w(px(36.0))
                .h(px(20.0))
                .rounded_full()
                .p(px(2.0))
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
                        .ml(move || {
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
    let check_for_mark = checked;
    let label = std::rc::Rc::new(label);
    let label_for_a11y = label.clone();
    base_checkbox()
        .flex_row()
        .checked(move || checked.get())
        .accessibility_label(move || Some(SharedString::from(label_for_a11y())))
        .items_center()
        .gap_3()
        .cursor_pointer()
        .focus_visible(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .hit_slop(EdgeInsets::all(px(4.0)))
        .indicator_style(move |s| {
            s.size(px(16.0))
                .items_center()
                .justify_center()
                .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
                .border(px(1.0))
                .border_color(move || theme().input)
        })
        .indicator(
            view()
                .size_full()
                .items_center()
                .justify_center()
                .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
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
                            .font_weight(FontWeight::MEDIUM)
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

/// 单选组：一个键盘焦点站，方向键在选项间移动并选择。分组名由所在
/// 分区的可见标题承担（NSH 0.5.1 的 RadioGroup 未提供标签 prop）。
pub fn radio_group(children: impl IntoElement) -> AnyElement {
    base_radio_group()
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
    let el = base_radio()
        .checked(move || checked_for_prop())
        .cursor_pointer()
        .hit_slop(EdgeInsets::all(px(4.0)))
        .py(px(6.0))
        .px(px(8.0))
        .rounded(move || theme().radius)
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
                        .border(px(1.0))
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
        .into_element();
    view().test_id(test_id).child(el).into_element()
}

/// 标签页容器（`tabs_list` 为 (test_id, 标签 i18n key)；选中指示与
/// 文字 token 化）。
pub fn tab_bar(tabs_list: Vec<(&'static str, &'static str)>, active: Signal<usize>) -> AnyElement {
    tabs()
        .flex_row()
        .gap_1()
        .p(px(3.0))
        .rounded(move || theme().radius)
        .bg(move || theme().muted)
        .children(
            tabs_list
                .into_iter()
                .enumerate()
                .map(move |(index, (test_id, label_key))| {
                    let el = tab()
                        .selected(move || active.get() == index)
                        .px(px(12.0))
                        .py(px(6.0))
                        .rounded(move || theme().radius)
                        .cursor_pointer()
                        .selected_style(move |s| s.bg(move || theme().background).shadow_sm())
                        .on_press(move || active.set(index))
                        .child(
                            text(move || t!(label_key))
                                .text_size(px(13.0))
                                .text_color(move || theme().foreground),
                        )
                        .into_element();
                    view().test_id(test_id).child(el).into_element()
                }),
        )
        .into_element()
}

/// 菜单演示入口：触发按钮 + 菜单内容（画廊用；业务页按同法组合）。
pub fn menu_demo(trigger: impl IntoElement) -> AnyElement {
    menu()
        .trigger(trigger)
        .child(
            menu_item()
                .child(text(|| t!("gallery.menu.item_one")))
                .on_select(|| {}),
        )
        .child(
            menu_item()
                .child(text(|| t!("gallery.menu.item_two")))
                .on_select(|| {}),
        )
        .child(menu_separator())
        .child(
            menu_item()
                .text_color(move || theme().destructive)
                .child(text(|| t!("gallery.menu.delete")))
                .on_select(|| {}),
        )
        .into_element()
}
