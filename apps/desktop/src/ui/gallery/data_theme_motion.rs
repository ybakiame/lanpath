//! 画廊分区：数据展示、主题与语义色板、动效令牌与交互演示。

use nsh_base::{DataTableColumn, DataTableRows, theme};
use nsh_core::prelude::*;

use super::section;
use crate::ui::icon::{Icon, Tone, icon_sized, status_icon};
use crate::ui::{ThemeId, button, feedback, layout, prefs, table, theme_preview};

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
    let row_icons = [Icon::Subscription, Icon::Core, Icon::Rules];
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
                        let icon_kind = row_icons[index];
                        layout::list_item(move || t!("gallery.list.item_title", index = index + 1))
                            .leading(icon_sized(icon_kind, Tone::Primary, 16.0))
                            .subtitle(|| t!("gallery.list.item_subtitle"))
                            .trailing(feedback::badge(
                                if index == 1 {
                                    feedback::BadgeVariant::Success
                                } else {
                                    feedback::BadgeVariant::Outline
                                },
                                move || {
                                    if index == 1 {
                                        t!("gallery.badge.success")
                                    } else {
                                        t!("gallery.state.normal")
                                    }
                                },
                            ))
                            .selected(move || selected.get() == index)
                            .test_id(match index {
                                0 => "list-item-0",
                                1 => "list-item-1",
                                _ => "list-item-2",
                            })
                            .on_press(move || selected.set(index))
                    })),
            )
            .child(table(columns, rows))
            .child(
                view()
                    .overflow_hidden()
                    .rounded(move || theme().radius)
                    .border(px(1.0))
                    .border_color(move || theme().border)
                    .child(feedback::status_bar(
                        text(|| t!("runtime.status.stopped"))
                            .text_color(move || theme().foreground),
                        text(|| t!("gallery.badge.success"))
                            .text_color(move || theme().muted_foreground),
                    )),
            ),
    )
}

pub(super) fn theme_section() -> AnyElement {
    let prefs = prefs::prefs();
    let preview = signal(None::<ThemeId>);
    cleanup(move || crate::ui::theme::apply(prefs.theme.get_untracked()));
    type TokenSwatch = (&'static str, fn(&nsh_base::Theme) -> AdaptiveColor);
    let swatches: &[TokenSwatch] = &[
        ("primary", |t| t.primary),
        ("secondary", |t| t.secondary),
        ("accent", |t| t.accent),
        ("muted", |t| t.muted),
        ("border", |t| t.border),
        ("ring", |t| t.ring),
        ("success", |t| t.color("success").unwrap_or(t.primary)),
        ("warning", |t| t.color("warning").unwrap_or(t.primary)),
        ("destructive", |t| t.destructive),
        ("path-line", |t| t.color("path-line").unwrap_or(t.border)),
    ];
    section(
        "gallery.section.theme",
        view()
            .flex_col()
            .gap_4()
            .child(
                view()
                    .grid()
                    .w_full()
                    .app_class("justify-items-stretch")
                    .grid_cols(move || if window().width.0 < 980.0 { 2_u16 } else { 4 })
                    .gap_3()
                    .children(ThemeId::ALL.iter().map(|id| {
                        let prefs_for_press = prefs;
                        let theme_id = *id;
                        let preview_enter = preview;
                        let preview_leave = preview;
                        let preview_press = preview;
                        let card = button::button(button::ButtonVariant::Preview)
                            .size(button::ButtonSize::Content)
                            .full_width()
                            .selected(move || prefs.theme.get() == theme_id)
                            .accessible_name(move || {
                                SharedString::from(t!(
                                    "appearance.theme.a11y",
                                    name = theme_id.id_name()
                                ))
                            })
                            .test_id(gallery_theme_test_id(theme_id))
                            .on_press(move || {
                                preview_press.set(None);
                                if prefs_for_press.apply_theme(theme_id).is_err() {
                                    crate::ui::theme::apply(prefs_for_press.theme.get_untracked());
                                    feedback::show_toast(t!("appearance.save_failed"));
                                }
                            })
                            .prefix(move || {
                                theme_preview::theme_preview(theme_id, move || {
                                    prefs.theme.get() == theme_id
                                })
                            });
                        view()
                            .w_full()
                            .on_mouse_enter(move |_: &MouseEvent| {
                                preview_enter.set(Some(theme_id));
                                crate::ui::theme::apply(theme_id);
                            })
                            .on_mouse_leave(move |_: &MouseEvent| {
                                if preview_leave.get_untracked() == Some(theme_id) {
                                    preview_leave.set(None);
                                    crate::ui::theme::apply(prefs.theme.get_untracked());
                                }
                            })
                            .child(card)
                    })),
            )
            .child(
                text(move || {
                    preview.get().map_or_else(
                        || t!("gallery.theme.pick"),
                        |id| t!("appearance.previewing", name = id.id_name()),
                    )
                })
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
            )
            .child(
                view()
                    .flex_col()
                    .gap_2()
                    .child(
                        text(|| t!("gallery.tokens.title"))
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(move || theme().muted_foreground),
                    )
                    .child(
                        view()
                            .flex_row()
                            .flex_wrap()
                            .gap_2()
                            .children(swatches.iter().map(|(name, getter)| {
                                let getter = *getter;
                                view()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .px_2()
                                    .py_1()
                                    .rounded(move || {
                                        theme().number("radius-sm").map(px).unwrap_or(px(4.0))
                                    })
                                    .border(px(1.0))
                                    .border_color(move || theme().border)
                                    .bg(move || theme().background)
                                    .child(
                                        view()
                                            .size(px(14.0))
                                            .rounded_full()
                                            .border(px(1.0))
                                            .border_color(move || theme().border)
                                            .bg(move || getter(&theme())),
                                    )
                                    .child(
                                        text(*name)
                                            .text_size(px(12.0))
                                            .text_color(move || theme().foreground),
                                    )
                                    .into_element()
                            })),
                    ),
            ),
    )
}

fn gallery_theme_test_id(id: ThemeId) -> &'static str {
    match id {
        ThemeId::Ocean => "gallery-theme-ocean",
        ThemeId::Teal => "gallery-theme-teal",
        ThemeId::Lanpath => "gallery-theme-lanpath",
        ThemeId::Custom => "gallery-theme-custom",
    }
}

pub(super) fn motion_section() -> AnyElement {
    let reduced = prefs::prefs().reduced;
    section(
        "gallery.section.motion",
        view()
            .flex_col()
            .gap_3()
            .child(
                view().flex_row().gap_2().flex_wrap().children(
                    [
                        "gallery.motion.instant",
                        "gallery.motion.fast",
                        "gallery.motion.base",
                        "gallery.motion.slow",
                        "gallery.motion.slower",
                        "gallery.motion.distance",
                        "gallery.motion.press",
                    ]
                    .map(|key| feedback::badge(feedback::BadgeVariant::Default, move || t!(key))),
                ),
            )
            .child(when(
                move || reduced.get(),
                || {
                    text(|| t!("gallery.motion.reduced_on"))
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground)
                },
            ))
            .child(motion_demo()),
    )
}

/// 动效演示：按下（pulse）、循环（spin）与状态图形交换（status_icon）。
/// 全部由动效 token 驱动；reduced 时立即落位且不启动循环动画。
fn motion_demo() -> AnyElement {
    let boxed = motion();
    let spin = motion();
    let spinning = signal(false);
    let glyph_step = signal(0_u8);
    let play_press = boxed;
    let play_spin = spin;
    view()
        .flex_row()
        .flex_wrap()
        .gap_3()
        .items_center()
        .child(
            view()
                .bind_motion(&boxed)
                .size(px(52.0))
                .rounded(move || theme().radius)
                .bg(move || theme().primary)
                .items_center()
                .justify_center()
                .child(
                    text("Aa")
                        .text_size(px(16.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || theme().primary_foreground),
                ),
        )
        .child(
            view()
                .bind_motion(&spin)
                .size(px(28.0))
                .rounded_full()
                .border(px(2.0))
                .border_color(move || theme().primary),
        )
        .child(
            view()
                .size(px(36.0))
                .rounded(move || theme().radius)
                .bg(move || theme().muted)
                .items_center()
                .justify_center()
                .child(when(
                    move || glyph_step.get() == 0,
                    || status_icon(Icon::Play, Tone::Primary),
                ))
                .child(when(
                    move || glyph_step.get() == 1,
                    || status_icon(Icon::Shield, Tone::Success),
                ))
                .child(when(
                    move || glyph_step.get() == 2,
                    || status_icon(Icon::Warning, Tone::Warning),
                )),
        )
        .child(
            button::button(button::ButtonVariant::Outline)
                .label(|| t!("gallery.motion.base"))
                .on_press(move || {
                    let _ = play_press.play(MotionPreset::Pulse);
                }),
        )
        .child(
            button::button(button::ButtonVariant::Outline)
                .label(|| t!("gallery.action.swap_glyph"))
                .on_press(move || {
                    glyph_step.update(|step| *step = (*step + 1) % 3);
                }),
        )
        .child(
            button::button(button::ButtonVariant::Outline)
                .label(|| t!("gallery.motion.demo"))
                .on_press(move || {
                    if prefs::prefs().reduced.get() {
                        play_spin.stop();
                        spinning.set(false);
                        return;
                    }
                    if spinning.get() {
                        play_spin.stop();
                        spinning.set(false);
                    } else {
                        let mut spec = MotionPreset::Spin.spec();
                        spec.timing.duration_ms = 1400.0;
                        let _ = play_spin.play(spec);
                        spinning.set(true);
                    }
                }),
        )
        .child(when(
            move || spinning.get(),
            || text(|| t!("gallery.motion.spinning")).text_size(px(12.0)),
        ))
        .into_element()
}
