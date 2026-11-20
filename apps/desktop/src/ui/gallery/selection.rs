//! 画廊分区：选择控件（Select / Switch / Checkbox / Radio / Segmented / Tabs）。

use nsh_base::theme;
use nsh_core::prelude::*;

use super::section;
use crate::ui::{feedback, prefs, select};

pub(super) fn selection_section() -> AnyElement {
    let selected_tab = signal(0_usize);
    let switched = signal(true);
    let checked = signal(true);
    let radio = signal(0_usize);
    let segmented = signal(0_usize);
    let prefs = prefs::prefs();
    let opacity = prefs.opacity;
    let language = prefs.language;
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
    let tab_keys = [
        "gallery.variant.primary",
        "gallery.variant.secondary",
        "gallery.variant.outline",
    ];
    section(
        "gallery.section.selection",
        view()
            .flex_col()
            .gap_4()
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
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
                            let result = prefs::prefs().apply_language(match chosen.as_deref() {
                                Some("zh-CN") => prefs::LanguagePref::ZhCn,
                                Some("en") => prefs::LanguagePref::En,
                                _ => prefs::LanguagePref::System,
                            });
                            if result.is_err() {
                                feedback::show_toast(t!("appearance.save_failed"));
                            }
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
                    .child(
                        text(|| t!("gallery.segmented.label"))
                            .text_size(px(12.0))
                            .text_color(move || theme().muted_foreground),
                    )
                    .child(select::radio_group(
                        || t!("gallery.segmented.label"),
                        select::segmented_group(
                            "gallery-segmented-demo",
                            [
                                ("seg-0", "gallery.variant.primary"),
                                ("seg-1", "gallery.variant.secondary"),
                                ("seg-2", "gallery.variant.outline"),
                            ]
                            .into_iter()
                            .enumerate()
                            .map(|(index, (test_id, key))| {
                                select::segmented_radio(
                                    move || segmented.get() == index,
                                    move || t!(key),
                                    test_id,
                                    move || segmented.set(index),
                                )
                            })
                            .collect(),
                        ),
                    )),
            )
            .child(select::radio_group(
                || t!("gallery.section.selection"),
                view().flex_col().gap_1().max_w(px(360.0)).children(
                    [0_usize, 1, 2].into_iter().map(|index| {
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
                    }),
                ),
            ))
            .child(
                view()
                    .flex_col()
                    .gap_2()
                    .max_w(px(360.0))
                    .child(
                        text(move || t!("gallery.slider.label", percent = opacity.get()))
                            .text_size(px(12.0))
                            .text_color(move || theme().muted_foreground),
                    )
                    .child(crate::ui::slider(
                        move || opacity.get() as f64,
                        prefs::MIN_OPACITY as f64,
                        prefs::MAX_OPACITY as f64,
                        1.0,
                        || false,
                        || t!("appearance.section.opacity"),
                        "gallery-opacity-slider",
                        move |v| prefs.preview_opacity(v.round() as u8),
                        move |v| {
                            let _ = prefs.apply_opacity(v.round() as u8);
                        },
                    )),
            )
            .child(
                view()
                    .flex_col()
                    .gap_2()
                    .child(select::tab_bar(
                        vec![
                            ("tab-0", tab_keys[0]),
                            ("tab-1", tab_keys[1]),
                            ("tab-2", tab_keys[2]),
                        ],
                        selected_tab,
                    ))
                    .child(
                        text(move || {
                            let label = t!(tab_keys[selected_tab.get().min(2)]);
                            t!("gallery.tab.panel", name = label)
                        })
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                    ),
            ),
    )
}
