//! 画廊分区：反馈（Badge / Spinner / Skeleton / Empty / Toast）与浮层（Dialog / Sheet / Tooltip / Popover / Menu）。

use nsh_base::theme;
use nsh_core::prelude::*;

use super::section;
use crate::ui::icon::Icon;
use crate::ui::{button, feedback, layout, overlay, select, sheet};

pub(super) fn feedback_section() -> AnyElement {
    section(
        "gallery.section.feedback",
        view()
            .flex_col()
            .gap_4()
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .items_center()
                    .child(feedback::badge(feedback::BadgeVariant::Default, || {
                        t!("gallery.badge.default")
                    }))
                    .child(feedback::badge_with_icon(
                        feedback::BadgeVariant::Success,
                        Some(Icon::Check),
                        || t!("gallery.badge.success"),
                    ))
                    .child(feedback::badge_with_icon(
                        feedback::BadgeVariant::Warning,
                        Some(Icon::Warning),
                        || t!("gallery.badge.warning"),
                    ))
                    .child(feedback::badge(feedback::BadgeVariant::Destructive, || {
                        t!("gallery.badge.destructive")
                    }))
                    .child(feedback::badge(feedback::BadgeVariant::Outline, || {
                        t!("gallery.badge.outline")
                    }))
                    .child(feedback::spinner()),
            )
            .child(
                view()
                    .flex_col()
                    .gap_2()
                    .max_w(px(420.0))
                    .child(feedback::skeleton_line(Some(180.0)))
                    .child(feedback::skeleton())
                    .child(feedback::skeleton_line(Some(280.0))),
            )
            .child(layout::surface(layout::SurfaceLevel::Background).child(
                feedback::empty_with_icon(
                    Some(Icon::Subscription),
                    || t!("gallery.empty.title"),
                    || t!("gallery.empty.description"),
                ),
            ))
            .child(
                view().flex_row().child(
                    button::button(button::ButtonVariant::Primary)
                        .label(|| t!("gallery.action.show_toast"))
                        .on_press(|| feedback::show_toast(t!("gallery.toast.message"))),
                ),
            ),
    )
}

pub(super) fn overlays_section() -> AnyElement {
    let dialog_open = signal(false);
    let sheet_open = signal(false);
    let confirmed = signal(false);
    section(
        "gallery.section.overlays",
        view()
            .flex_col()
            .gap_4()
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .items_center()
                    .child(
                        button::button(button::ButtonVariant::Primary)
                            .label(|| t!("gallery.action.open_dialog"))
                            .on_press(move || dialog_open.set(true)),
                    )
                    .child(
                        button::button(button::ButtonVariant::Secondary)
                            .test_id("gallery-open-sheet")
                            .label(|| t!("gallery.action.open_sheet"))
                            .on_press(move || sheet_open.set(true)),
                    )
                    .child(overlay::tooltip(
                        view().child(
                            button::button(button::ButtonVariant::Outline)
                                .label(|| t!("gallery.variant.ghost")),
                        ),
                        || t!("gallery.tooltip.content"),
                    ))
                    .child(overlay::popover(
                        view().child(
                            button::button(button::ButtonVariant::Outline)
                                .label(|| t!("gallery.action.open_popover")),
                        ),
                        view()
                            .w(px(260.0))
                            .flex_col()
                            .gap_2()
                            .child(
                                text(|| t!("gallery.action.open_popover"))
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(move || theme().popover_foreground),
                            )
                            .child(
                                text(|| t!("gallery.popover.content"))
                                    .text_size(px(12.0))
                                    .text_color(move || theme().muted_foreground),
                            ),
                    ))
                    .child(select::menu_demo(
                        button::button(button::ButtonVariant::Outline)
                            .label(|| t!("gallery.action.open_menu")),
                    )),
            )
            .child(when(
                move || confirmed.get(),
                || {
                    text(|| t!("common.confirm"))
                        .text_size(px(12.0))
                        .text_color(move || theme().color("success").unwrap_or(theme().primary))
                },
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
            )
            .child(sheet::sheet(
                420.0,
                || t!("gallery.sheet.title"),
                move || sheet_open.get(),
                move || sheet_open.set(false),
                || {},
                view()
                    .flex_col()
                    .gap_4()
                    .child(
                        text(|| t!("gallery.sheet.description"))
                            .text_size(px(14.0))
                            .text_color(move || theme().muted_foreground),
                    )
                    .child(feedback::badge_with_icon(
                        feedback::BadgeVariant::Success,
                        Some(Icon::Check),
                        || t!("gallery.badge.success"),
                    )),
            )),
    )
}
