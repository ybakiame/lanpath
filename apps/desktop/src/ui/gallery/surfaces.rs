//! 画廊分区：循径线性图标家族、表面层级、选择卡片与工具栏。

use nsh_base::theme;
use nsh_core::prelude::*;

use super::section;
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::{button, cards, feedback, layout, select};

pub(super) fn icons_section() -> AnyElement {
    let tones = [
        (Tone::Muted, "Muted"),
        (Tone::Foreground, "Foreground"),
        (Tone::Primary, "Primary"),
        (Tone::Success, "Success"),
        (Tone::Warning, "Warning"),
        (Tone::Danger, "Danger"),
    ];
    section(
        "gallery.section.icons",
        view()
            .flex_col()
            .gap_4()
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .children(tones.into_iter().map(|(tone, name)| {
                        view()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_1()
                            .rounded(move || theme().radius)
                            .bg(move || theme().muted)
                            .child(icon_sized(Icon::Shield, tone, 16.0))
                            .child(
                                text(name)
                                    .text_size(px(12.0))
                                    .text_color(move || theme().foreground),
                            )
                            .into_element()
                    })),
            )
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .children(Icon::ALL.iter().map(|(kind, name)| {
                        view()
                            .w(px(96.0))
                            .p_2()
                            .flex_col()
                            .items_center()
                            .gap_1()
                            .rounded(move || theme().radius)
                            .border(px(1.0))
                            .border_color(move || theme().border)
                            .bg(move || theme().background)
                            .child(icon_sized(*kind, Tone::Primary, 18.0))
                            .child(
                                text(*name)
                                    .text_size(px(12.0))
                                    .text_color(move || theme().muted_foreground)
                                    .truncate(),
                            )
                            .into_element()
                    })),
            ),
    )
}

pub(super) fn surfaces_section() -> AnyElement {
    let card_enabled = signal(true);
    let card_selected = signal(true);
    let surface_tile = |level: layout::SurfaceLevel, title: &'static str| {
        layout::surface(level)
            .flex_1()
            .min_w(px(160.0))
            .p_3()
            .flex_col()
            .gap_1()
            .child(
                text(title)
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(move || theme().foreground),
            )
            .child(
                text("radius-lg 8pt · 1pt border")
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground),
            )
            .into_element()
    };
    section(
        "gallery.section.surfaces",
        view()
            .flex_col()
            .gap_4()
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap_3()
                    .child(surface_tile(layout::SurfaceLevel::Background, "Background"))
                    .child(surface_tile(layout::SurfaceLevel::Card, "Card"))
                    .child(surface_tile(layout::SurfaceLevel::Popover, "Popover")),
            )
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap_3()
                    .items_start()
                    .child(
                        view().flex_1().min_w(px(260.0)).child(
                            cards::selection_card(
                                move || card_enabled.get(),
                                move || card_selected.get(),
                            )
                            .child(
                                view()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(
                                        text(|| t!("gallery.card.enabled"))
                                            .text_size(px(14.0))
                                            .font_weight(FontWeight::MEDIUM),
                                    )
                                    .child(select::switch(card_enabled, move |v| {
                                        card_enabled.set(v);
                                    })),
                            )
                            .child(
                                text(|| t!("gallery.card.hint"))
                                    .text_size(px(12.0))
                                    .text_color(move || theme().muted_foreground),
                            )
                            .child(
                                view()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        button::button(button::ButtonVariant::Outline)
                                            .size(button::ButtonSize::Sm)
                                            .label(|| t!("gallery.card.selected"))
                                            .on_press(move || {
                                                card_selected.set(!card_selected.get_untracked());
                                            }),
                                    )
                                    .child(when(
                                        move || card_selected.get(),
                                        || {
                                            feedback::badge_with_icon(
                                                feedback::BadgeVariant::Success,
                                                Some(Icon::Check),
                                                || t!("subscription.active_mark"),
                                            )
                                        },
                                    )),
                            ),
                        ),
                    )
                    .child(
                        layout::surface(layout::SurfaceLevel::Background)
                            .p_3()
                            .flex_col()
                            .gap_2()
                            .child(
                                text("Toolbar")
                                    .text_size(px(12.0))
                                    .text_color(move || theme().muted_foreground),
                            )
                            .child(layout::toolbar(vec![
                                vec![
                                    button::icon_button()
                                        .tool_size()
                                        .accessibility_label(|| SharedString::from("Play"))
                                        .child(icon_sized(Icon::Play, Tone::Primary, 14.0))
                                        .on_press(|| {})
                                        .into_element(),
                                    button::icon_button()
                                        .tool_size()
                                        .accessibility_label(|| SharedString::from("Refresh"))
                                        .child(icon_sized(Icon::Refresh, Tone::Foreground, 14.0))
                                        .on_press(|| {})
                                        .into_element(),
                                ],
                                vec![
                                    button::icon_button()
                                        .tool_size()
                                        .accessibility_label(|| SharedString::from("Copy"))
                                        .child(icon_sized(Icon::Copy, Tone::Muted, 14.0))
                                        .on_press(|| {})
                                        .into_element(),
                                    button::icon_button()
                                        .tool_size()
                                        .accessibility_label(|| SharedString::from("Delete"))
                                        .child(icon_sized(Icon::Trash, Tone::Danger, 14.0))
                                        .on_press(|| {})
                                        .into_element(),
                                ],
                            ])),
                    ),
            ),
    )
}
