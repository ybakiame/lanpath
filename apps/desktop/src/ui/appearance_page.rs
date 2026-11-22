//! Appearance settings: theme previews, mode, window backdrop material
//! (Acrylic frost / Mica / Solid) with custom opacity slider, language, and reduced motion.

use nsh_base::theme;
use nsh_core::element::desktop::main_window;
use nsh_core::prelude::*;
use nsh_core::storage::StorageFull;

use super::{
    button, feedback, layout,
    prefs::{self, MAX_OPACITY, MIN_OPACITY, WindowMaterial},
    select, slider,
    theme::ThemeId,
    theme_preview,
};

/// 外观页。
pub fn appearance_page() -> impl IntoElement {
    let prefs = prefs::prefs();
    let preview = signal(None::<ThemeId>);
    let theme_error = signal(None::<String>);
    cleanup(move || prefs.restore_applied_theme());
    let preview_for_focus = preview;
    main_window().on_focus_change(move |focused| {
        if !focused && preview_for_focus.get_untracked().is_some() {
            preview_for_focus.set(None);
            prefs.restore_applied_theme();
        }
    });
    scroll().flex_1().child(
        view()
            .flex_col()
            .gap_5()
            .max_w(px(960.0))
            .child(layout::page_header(
                || t!("appearance.title"),
                || t!("appearance.subtitle"),
            ))
            .child(
                layout::surface(layout::SurfaceLevel::Card)
                    .p_5()
                    .flex_col()
                    .gap_3()
                    .child(layout::section_title(|| t!("appearance.section.theme")))
                    .child(
                        view()
                            .grid()
                            .w_full()
                            .app_class("justify-items-stretch")
                            .grid_cols(move || {
                                let width =
                                    (window().width.0 - crate::shell::sidebar_width() - 88.0)
                                        .min(920.0);
                                (width / (if font_scale() > 1.3 { 200.0 } else { 152.0 }))
                                    .floor()
                                    .clamp(2.0, 4.0) as u16
                            })
                            .gap_3()
                            .children(ThemeId::ALL.iter().map(|id| {
                                let theme_id = *id;
                                let preview_enter = preview;
                                let preview_leave = preview;
                                let preview_press = preview;
                                let error = theme_error;
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
                                    .test_id(theme_test_id(theme_id))
                                    .on_press(move || {
                                        preview_press.set(None);
                                        let result = prefs.apply_theme(theme_id);
                                        if result.is_err() {
                                            prefs.restore_applied_theme();
                                        }
                                        finish_apply(result, error);
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
                                        super::theme::apply(theme_id);
                                    })
                                    .on_mouse_leave(move |_: &MouseEvent| {
                                        if preview_leave.get_untracked() == Some(theme_id) {
                                            preview_leave.set(None);
                                            prefs.restore_applied_theme();
                                        }
                                    })
                                    .child(card)
                            })),
                    )
                    .child(
                        text(move || {
                            preview.get().map_or_else(
                                || t!("appearance.hover_preview"),
                                |id| t!("appearance.previewing", name = id.id_name()),
                            )
                        })
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                    )
                    .child(when(
                        move || theme_error.get().is_some(),
                        move || {
                            text(move || theme_error.get().unwrap_or_default())
                                .text_size(px(12.0))
                                .text_color(move || theme().destructive)
                        },
                    )),
            )
            .child(super::custom_theme_card::custom_theme_card())
            .child(backdrop_card())
            .child(
                layout::surface(layout::SurfaceLevel::Card)
                    .p_5()
                    .flex_col()
                    .gap_5()
                    .child(mode_section())
                    .child(layout::separator())
                    .child(language_section())
                    .child(layout::separator())
                    .child(motion_section()),
            ),
    )
}

fn backdrop_card() -> AnyElement {
    let prefs = prefs::prefs();
    let material = prefs.material;
    let opacity = prefs.opacity;
    let error = signal(None::<String>);
    layout::surface(layout::SurfaceLevel::Card)
        .p_5()
        .flex_col()
        .gap_4()
        .child(
            view()
                .flex_col()
                .gap_1()
                .child(layout::section_title(|| t!("appearance.section.backdrop")))
                .child(
                    text(|| t!("appearance.backdrop.hint"))
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .child(select::radio_group(
            || t!("appearance.section.backdrop"),
            select::segmented_group(
                "backdrop-group",
                WindowMaterial::ALL
                    .iter()
                    .map(|target| {
                        let target = *target;
                        let test_id = match target {
                            WindowMaterial::Acrylic => "backdrop-acrylic",
                            WindowMaterial::Mica => "backdrop-mica",
                            WindowMaterial::Solid => "backdrop-solid",
                        };
                        select::segmented_radio(
                            move || material.get() == target,
                            move || t!(target.label_key()),
                            test_id,
                            move || finish_apply(prefs.apply_material(target), error),
                        )
                    })
                    .collect(),
            ),
        ))
        .child(
            view()
                .flex_col()
                .gap_2()
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(
                            text(|| t!("appearance.section.opacity"))
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(move || theme().foreground),
                        )
                        .child(
                            text(move || {
                                let percent = if material.get() == WindowMaterial::Solid {
                                    100
                                } else {
                                    opacity.get()
                                };
                                t!("appearance.opacity.value", percent = percent)
                            })
                            .test_id("backdrop-opacity-value")
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(move || theme().primary),
                        ),
                )
                .child(view().max_w(px(420.0)).child(slider::slider(
                    move || {
                        if material.get() == WindowMaterial::Solid {
                            MAX_OPACITY as f64
                        } else {
                            opacity.get() as f64
                        }
                    },
                    MIN_OPACITY as f64,
                    MAX_OPACITY as f64,
                    1.0,
                    move || material.get() == WindowMaterial::Solid,
                    || t!("appearance.section.opacity"),
                    "backdrop-opacity-slider",
                    move |v| prefs.preview_opacity(v.round() as u8),
                    move |v| finish_apply(prefs.apply_opacity(v.round() as u8), error),
                ))),
        )
        .child(when(
            move || error.get().is_some(),
            move || {
                text(move || error.get().unwrap_or_default())
                    .text_size(px(12.0))
                    .text_color(move || theme().destructive)
            },
        ))
        .into_element()
}

fn theme_test_id(id: ThemeId) -> &'static str {
    match id {
        ThemeId::Ocean => "theme-ocean",
        ThemeId::Teal => "theme-teal",
        ThemeId::Lanpath => "theme-lanpath",
        ThemeId::Custom => "theme-custom",
    }
}

fn finish_apply(result: Result<(), StorageFull>, error: Signal<Option<String>>) {
    match result {
        Ok(()) => error.set(None),
        Err(_) => {
            let message = t!("appearance.save_failed");
            error.set(Some(message.clone()));
            feedback::show_toast(message);
        }
    }
}

fn mode_section() -> AnyElement {
    let prefs = prefs::prefs();
    let mode = prefs.mode;
    let error = signal(None::<String>);
    view()
        .flex_col()
        .gap_2()
        .child(layout::section_title(|| t!("appearance.section.mode")))
        .child(select::radio_group(
            || t!("appearance.section.mode"),
            select::segmented_group(
                "mode-group",
                prefs::Mode::ALL
                    .iter()
                    .map(|target| {
                        let target = *target;
                        let test_id = match target {
                            prefs::Mode::Light => "appearance-light",
                            prefs::Mode::Dark => "appearance-dark",
                            prefs::Mode::System => "appearance-system",
                        };
                        select::segmented_radio(
                            move || mode.get() == target,
                            move || t!(target.label_key()),
                            test_id,
                            move || finish_apply(prefs.apply_mode(target), error),
                        )
                    })
                    .collect(),
            ),
        ))
        .child(when(
            move || error.get().is_some(),
            move || {
                text(move || error.get().unwrap_or_default())
                    .text_size(px(12.0))
                    .text_color(move || theme().destructive)
            },
        ))
        .into_element()
}

fn language_section() -> AnyElement {
    let prefs = prefs::prefs();
    let language = prefs.language;
    let error = signal(None::<String>);
    view()
        .flex_col()
        .gap_2()
        .child(layout::section_title(|| t!("appearance.section.language")))
        .child(select::radio_group(
            || t!("appearance.section.language"),
            select::segmented_group(
                "language-group",
                prefs::LanguagePref::ALL
                    .iter()
                    .map(|target| {
                        let target = *target;
                        let test_id = match target {
                            prefs::LanguagePref::System => "language-system",
                            prefs::LanguagePref::ZhCn => "language-zh-cn",
                            prefs::LanguagePref::En => "language-en",
                        };
                        select::segmented_radio(
                            move || language.get() == target,
                            move || t!(target.label_key()),
                            test_id,
                            move || finish_apply(prefs.apply_language(target), error),
                        )
                    })
                    .collect(),
            ),
        ))
        .child(when(
            move || error.get().is_some(),
            move || {
                text(move || error.get().unwrap_or_default())
                    .text_size(px(12.0))
                    .text_color(move || theme().destructive)
            },
        ))
        .into_element()
}

fn motion_section() -> AnyElement {
    let prefs = prefs::prefs();
    let motion = prefs.motion;
    let error = signal(None::<String>);
    view()
        .flex_col()
        .gap_2()
        .child(layout::section_title(|| t!("appearance.section.motion")))
        .child(select::radio_group(
            || t!("appearance.section.motion"),
            select::segmented_group(
                "motion-group",
                prefs::MotionPref::ALL
                    .iter()
                    .map(|target| {
                        let target = *target;
                        let test_id = match target {
                            prefs::MotionPref::System => "motion-system",
                            prefs::MotionPref::Reduce => "motion-reduce",
                        };
                        select::segmented_radio(
                            move || motion.get() == target,
                            move || t!(target.label_key()),
                            test_id,
                            move || finish_apply(prefs.apply_motion(target), error),
                        )
                    })
                    .collect(),
            ),
        ))
        .child(when(
            move || error.get().is_some(),
            move || {
                text(move || error.get().unwrap_or_default())
                    .text_size(px(12.0))
                    .text_color(move || theme().destructive)
            },
        ))
        .into_element()
}
