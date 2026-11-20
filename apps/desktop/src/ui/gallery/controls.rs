//! 画廊分区：按钮矩阵与输入/字段组件。

use nsh_base::theme;
use nsh_core::prelude::*;

use super::section;
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::{button, field, input, select, textarea};

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
    let sizes = [
        (button::ButtonSize::Sm, "gallery.size.sm"),
        (button::ButtonSize::Md, "gallery.size.md"),
        (button::ButtonSize::Lg, "gallery.size.lg"),
    ];
    section(
        "gallery.section.buttons",
        view()
            .flex_col()
            .gap_4()
            .child(
                view()
                    .flex_col()
                    .gap_2()
                    .children(sizes.into_iter().map(|(size, size_key)| {
                        view()
                            .flex_row()
                            .flex_wrap()
                            .gap_2()
                            .items_center()
                            .child(
                                text(move || t!(size_key))
                                    .w(px(56.0))
                                    .text_size(px(12.0))
                                    .text_color(move || theme().muted_foreground),
                            )
                            .children(variants.into_iter().map(move |(variant, label_key)| {
                                button::button(variant)
                                    .size(size)
                                    .label(move || t!(label_key))
                                    .on_press(|| {})
                            }))
                            .into_element()
                    })),
            )
            .child(crate::ui::separator())
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .items_center()
                    .child(
                        button::button(button::ButtonVariant::Primary)
                            .prefix(|| icon_sized(Icon::Play, Tone::OnPrimary, 14.0))
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
                        view().test_id("prefix-loading-button").child(
                            button::button(button::ButtonVariant::Secondary)
                                .label(|| t!("gallery.action.press_me"))
                                .loading(move || loading.get())
                                .prefix(|| text("◆").text_size(px(13.0)).into_element()),
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
                            .child(icon_sized(Icon::Refresh, Tone::Foreground, 16.0))
                            .on_press(|| {}),
                    )
                    .child(
                        button::icon_button()
                            .tool_size()
                            .accessibility_label(|| SharedString::from("Copy"))
                            .child(icon_sized(Icon::Copy, Tone::Muted, 14.0))
                            .on_press(|| {}),
                    )
                    .child(
                        button::icon_button()
                            .tool_size()
                            .accessibility_label(|| SharedString::from("Search"))
                            .child(icon_sized(Icon::Search, Tone::Muted, 14.0))
                            .on_press(|| {}),
                    ),
            )
            .child(
                view()
                    .flex_row()
                    .gap_4()
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
    let readonly_value = signal("127.0.0.1:7890".to_owned());
    section(
        "gallery.section.inputs",
        view()
            .flex_col()
            .gap_4()
            .max_w(px(640.0))
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
                    .flex_wrap()
                    .gap_3()
                    .child(
                        view().flex_1().min_w(px(180.0)).child(
                            input(signal(String::new()))
                                .placeholder(|| t!("gallery.state.disabled"))
                                .disabled(|| true),
                        ),
                    )
                    .child(
                        view().flex_1().min_w(px(180.0)).child(
                            input(signal("secret-passphrase".to_owned()))
                                .placeholder(|| t!("backup.password"))
                                .masked(),
                        ),
                    ),
            )
            .child(
                field()
                    .label(|| t!("gallery.state.readonly"))
                    .control(input(readonly_value).readonly(|| true)),
            )
            .child(
                field().label(|| t!("gallery.field.name")).control(
                    textarea(signal(String::new()))
                        .placeholder(|| t!("gallery.field.name_placeholder"))
                        .on_change(|_| {}),
                ),
            ),
    )
}
