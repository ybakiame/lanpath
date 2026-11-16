//! 画廊分区：按钮 / 输入 / 选择 / 反馈 / 浮层 / 数据展示。全部为真实
//! 可用组件的验收排列，无占位假控件；文案与状态由 `t!` 与语义
//! token 驱动。分区容器见 `mod.rs` 的 `section`。

use nsh_base::{DataTableColumn, DataTableRows, popover, theme};
use nsh_core::prelude::*;

use super::section;
use crate::ui::{button, feedback, field, input, layout, overlay, prefs, select, table, textarea};

pub(super) fn buttons_section() -> AnyElement {
    let loading = signal(true);
    let disabled = signal(true);
    let variants = [
        (button::ButtonVariant::Primary, "gallery.variant.primary"),
        (
            button::ButtonVariant::Secondary,
            "gallery.variant.secondary",
        ),
        (button::ButtonVariant::Outline, "gallery.variant.outline"),
        (button::ButtonVariant::Ghost, "gallery.variant.ghost"),
        (
            button::ButtonVariant::Destructive,
            "gallery.variant.destructive",
        ),
    ];
    section(
        "gallery.section.buttons",
        view()
            .flex_col()
            .gap_4()
            .child(
                view().flex_col().gap_2().children(
                    [
                        button::ButtonSize::Sm,
                        button::ButtonSize::Md,
                        button::ButtonSize::Lg,
                    ]
                    .into_iter()
                    .map(|size| {
                        view()
                            .flex_row()
                            .gap_2()
                            .items_center()
                            .children(variants.into_iter().map(move |(variant, label_key)| {
                                button::button(variant)
                                    .size(size)
                                    .label(move || t!(label_key))
                                    .on_press(|| {})
                            }))
                            .into_element()
                    }),
                ),
            )
            .child(
                view()
                    .flex_row()
                    .gap_2()
                    .items_center()
                    .child(
                        button::button(button::ButtonVariant::Primary)
                            .label(|| t!("gallery.action.press_me")),
                    )
                    .child(
                        view().test_id("loading-button").child(
                            button::button(button::ButtonVariant::Secondary)
                                .label(move || t!("gallery.state.loading"))
                                .loading(move || loading.get()),
                        ),
                    )
                    .child(
                        button::button(button::ButtonVariant::Outline)
                            .label(move || t!("gallery.state.disabled"))
                            .disabled(move || disabled.get()),
                    )
                    .child(
                        button::icon_button()
                            .accessibility_label(move || {
                                SharedString::from(t!("gallery.action.refresh"))
                            })
                            .child(text("⟳").text_size(px(15.0)))
                            .on_press(|| {}),
                    ),
            )
            .child(
                view()
                    .flex_row()
                    .gap_3()
                    .items_center()
                    .child(view().test_id("loading-toggle").child(select::checkbox(
                        loading,
                        || t!("gallery.state.loading"),
                        move |next| loading.set(next),
                    )))
                    .child(view().test_id("disabled-toggle").child(select::checkbox(
                        disabled,
                        || t!("gallery.state.disabled"),
                        move |next| disabled.set(next),
                    ))),
            ),
    )
}

pub(super) fn inputs_section() -> AnyElement {
    let value = signal(String::new());
    let error = signal(None::<String>);
    section(
        "gallery.section.inputs",
        view()
            .flex_col()
            .gap_4()
            .child(
                field()
                    .label(|| t!("gallery.field.name"))
                    .hint(|| t!("gallery.field.name_hint"))
                    .error(move || error.get())
                    .control(
                        input(value)
                            .placeholder(|| t!("gallery.field.name_placeholder"))
                            .invalid(move || error.get().is_some())
                            .on_change(move |text| {
                                value.set(text.clone());
                                error.set(if text.trim().is_empty() {
                                    Some(t!("gallery.field.error_required"))
                                } else {
                                    None
                                });
                            }),
                    ),
            )
            .child(
                view()
                    .flex_row()
                    .gap_2()
                    .child(
                        input(signal(String::new()))
                            .placeholder(|| t!("gallery.field.name_placeholder"))
                            .disabled(|| true),
                    )
                    .child(input(signal(String::new())).masked()),
            )
            .child(
                field()
                    .label(|| t!("gallery.field.name"))
                    .control(textarea(signal(String::new())).on_change(|_| {})),
            ),
    )
}

pub(super) fn selection_section() -> AnyElement {
    let selected_tab = signal(0_usize);
    let switched = signal(false);
    let checked = signal(false);
    let radio = signal(0_usize);
    let language = prefs::prefs().language;
    let items = prefs::LanguagePref::ALL
        .iter()
        .map(|pref| {
            let mut item = nsh_base::SelectItem::default();
            item.value = SharedString::from(match pref {
                prefs::LanguagePref::System => "system",
                prefs::LanguagePref::ZhCn => "zh-CN",
                prefs::LanguagePref::En => "en",
            });
            item.title = t!(pref.label_key()).into();
            item
        })
        .collect();
    section(
        "gallery.section.selection",
        view()
            .flex_col()
            .gap_4()
            .child(
                view()
                    .flex_row()
                    .gap_4()
                    .items_center()
                    .child(select::select(
                        items,
                        move || match language.get() {
                            prefs::LanguagePref::System => Some(SharedString::from("system")),
                            prefs::LanguagePref::ZhCn => Some(SharedString::from("zh-CN")),
                            prefs::LanguagePref::En => Some(SharedString::from("en")),
                        },
                        move |chosen| {
                            prefs::prefs().apply_language(match chosen.as_deref() {
                                Some("zh-CN") => prefs::LanguagePref::ZhCn,
                                Some("en") => prefs::LanguagePref::En,
                                _ => prefs::LanguagePref::System,
                            });
                        },
                    ))
                    .child(select::switch(switched, move |next| switched.set(next)))
                    .child(select::checkbox(
                        checked,
                        || t!("gallery.state.normal"),
                        move |next| checked.set(next),
                    )),
            )
            .child(
                view()
                    .flex_col()
                    .gap_2()
                    .children([0_usize, 1, 2].into_iter().map(|index| {
                        select::radio_row(
                            move || radio.get() == index,
                            || t!("gallery.state.normal"),
                            match index {
                                0 => "radio-0",
                                1 => "radio-1",
                                _ => "radio-2",
                            },
                            move || radio.set(index),
                        )
                    })),
            )
            .child(select::tab_bar(
                vec![
                    ("tab-0", "gallery.variant.primary"),
                    ("tab-1", "gallery.variant.secondary"),
                    ("tab-2", "gallery.variant.outline"),
                ],
                selected_tab,
            )),
    )
}

pub(super) fn feedback_section() -> AnyElement {
    section(
        "gallery.section.feedback",
        view()
            .flex_col()
            .gap_4()
            .child(
                view()
                    .flex_row()
                    .gap_2()
                    .items_center()
                    .child(feedback::badge(feedback::BadgeVariant::Default, || {
                        t!("gallery.badge.default")
                    }))
                    .child(feedback::badge(feedback::BadgeVariant::Success, || {
                        t!("gallery.badge.success")
                    }))
                    .child(feedback::badge(feedback::BadgeVariant::Warning, || {
                        t!("gallery.badge.warning")
                    }))
                    .child(feedback::badge(feedback::BadgeVariant::Destructive, || {
                        t!("gallery.badge.destructive")
                    }))
                    .child(feedback::badge(feedback::BadgeVariant::Outline, || {
                        t!("gallery.badge.outline")
                    }))
                    .child(feedback::spinner()),
            )
            .child(feedback::skeleton())
            .child(
                layout::surface(layout::SurfaceLevel::Card).child(feedback::empty(
                    || t!("gallery.empty.title"),
                    || t!("gallery.empty.description"),
                )),
            )
            .child(
                button::button(button::ButtonVariant::Primary)
                    .label(|| t!("gallery.action.show_toast"))
                    .on_press(|| feedback::show_toast(t!("gallery.toast.message"))),
            ),
    )
}

pub(super) fn overlays_section() -> AnyElement {
    let dialog_open = signal(false);
    let confirmed = signal(false);
    section(
        "gallery.section.overlays",
        view()
            .flex_col()
            .gap_4()
            .child(
                view()
                    .flex_row()
                    .gap_2()
                    .items_center()
                    .child(
                        button::button(button::ButtonVariant::Primary)
                            .label(|| t!("gallery.action.open_dialog"))
                            .on_press(move || dialog_open.set(true)),
                    )
                    .child(overlay::tooltip(
                        view().child(
                            button::button(button::ButtonVariant::Outline)
                                .label(|| t!("gallery.variant.ghost")),
                        ),
                        || t!("gallery.tooltip.content"),
                    ))
                    .child(
                        popover()
                            .trigger(
                                view().child(
                                    button::button(button::ButtonVariant::Outline)
                                        .label(|| t!("gallery.action.open_popover")),
                                ),
                            )
                            .child(
                                view()
                                    .p_3()
                                    .w(px(260.0))
                                    .rounded(move || theme().radius)
                                    .bg(move || theme().popover)
                                    .text_color(move || theme().popover_foreground)
                                    .border(px(1.0))
                                    .border_color(move || theme().border)
                                    .shadow_md()
                                    .child(
                                        text(|| t!("gallery.popover.content")).text_size(px(13.0)),
                                    ),
                            ),
                    )
                    .child(select::menu_demo(
                        button::button(button::ButtonVariant::Outline)
                            .label(|| t!("gallery.action.open_menu")),
                    )),
            )
            .child(when(
                move || confirmed.get(),
                || text(|| t!("common.confirm")).text_size(px(12.0)),
            ))
            .child(
                overlay::confirm_dialog(move || dialog_open.get())
                    .title(|| t!("gallery.dialog.title"))
                    .description(|| t!("gallery.dialog.description"))
                    .on_confirm(move || {
                        confirmed.set(true);
                        dialog_open.set(false);
                    })
                    .on_cancel(move || dialog_open.set(false)),
            ),
    )
}

pub(super) fn data_section() -> AnyElement {
    let selected = signal(1_usize);
    let mut name = DataTableColumn::default();
    name.key = "name".into();
    name.title = Some(t!("gallery.field.name").into());
    name.width = Some(px(180.0));
    let mut state = DataTableColumn::default();
    state.key = "state".into();
    state.width = Some(px(120.0));
    let mut count = DataTableColumn::default();
    count.key = "count".into();
    count.width = Some(px(90.0));
    let columns = vec![name, state, count];
    let rows = DataTableRows::new(4)
        .texts(
            "name",
            (1..=4).map(|i| t!("gallery.list.item_title", index = i)),
        )
        .texts(
            "state",
            [true, false, true, false].map(|on| {
                if on {
                    t!("gallery.state.normal")
                } else {
                    t!("gallery.state.disabled")
                }
            }),
        )
        .numbers("count", [12, 8, 30, 4].map(f64::from));
    section(
        "gallery.section.data",
        view()
            .flex_col()
            .gap_4()
            .child(
                view()
                    .flex_col()
                    .gap_1()
                    .children([0_usize, 1, 2].into_iter().map(|index| {
                        layout::list_item(move || t!("gallery.list.item_title", index = index + 1))
                            .subtitle(|| t!("gallery.list.item_subtitle"))
                            .selected(move || selected.get() == index)
                            .test_id(match index {
                                0 => "list-item-0",
                                1 => "list-item-1",
                                _ => "list-item-2",
                            })
                            .on_press(move || selected.set(index))
                    })),
            )
            .child(table(columns, rows)),
    )
}
