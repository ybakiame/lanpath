//! 选择控件：Select/Switch/Checkbox/RadioRow/Tabs 的语义薄封装，
//! 外观只来自 token。

use nsh_base::{
    checkbox as base_checkbox, menu, menu_item, menu_separator, radio as base_radio,
    select as base_select, switch as base_switch, tab, tabs, theme,
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
    base_switch()
        .checked(move || checked.get())
        .track_style(move |s| {
            s.bg(move || {
                if checked.get() {
                    theme().primary
                } else {
                    theme().input
                }
            })
            .rounded_full()
        })
        .thumb_style(move |s| {
            s.rounded_full().bg(move || {
                theme()
                    .color("switch-thumb")
                    .unwrap_or_else(|| theme().card)
            })
        })
        .on_change(move |next: &bool| on_change(*next))
        .into_element()
}

/// 复选框（标签在右，整行可点）。
pub fn checkbox(
    checked: Signal<bool>,
    label: impl Fn() -> String + 'static,
    on_change: impl Fn(bool) + 'static,
) -> AnyElement {
    base_checkbox()
        .checked(move || checked.get())
        .items_center()
        .gap_3()
        .cursor_pointer()
        .on_change(move |next: &bool| on_change(*next))
        .child(
            text(label)
                .text_size(px(14.0))
                .text_color(move || theme().foreground),
        )
        .into_element()
}

/// 单选行（标签在右，整行可点；`test_id` 供测试与自动化）。
pub fn radio_row(
    checked: impl Fn() -> bool + 'static,
    label: impl Fn() -> String + 'static,
    test_id: &'static str,
    on_select: impl Fn() + 'static,
) -> AnyElement {
    let checked = std::rc::Rc::new(checked);
    let is_checked: std::rc::Rc<dyn Fn() -> bool> = std::rc::Rc::new(move || checked());
    let checked_for_prop = is_checked.clone();
    let checked_for_bg = is_checked.clone();
    let el = base_radio()
        .checked(move || checked_for_prop())
        .cursor_pointer()
        .on_change(move |_: &bool| on_select())
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    view()
                        .size(px(16.0))
                        .rounded_full()
                        .border(px(1.0))
                        .border_color(move || theme().muted_foreground)
                        .bg(move || {
                            if checked_for_bg() {
                                theme().primary
                            } else {
                                theme().card
                            }
                        }),
                )
                .child(
                    text(label)
                        .text_size(px(14.0))
                        .text_color(move || theme().foreground),
                ),
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
