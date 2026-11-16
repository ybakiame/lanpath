//! 外观设置页（阶段 0 重构）：13 主题网格（4 列，点击应用并保存）、
//! 外观模式、界面语言、动效偏好。界面缩放为诚实的不支持项——NSH
//! 0.5.0 的 `font_scale` 仅平台可读，此处只展示当前系统文本缩放。

use nsh_base::theme;
use nsh_core::prelude::*;

use super::{button, feedback, layout, prefs, select, theme::ThemeId, theme_preview};

/// 外观页。
pub fn appearance_page() -> impl IntoElement {
    let prefs = prefs::prefs();
    scroll().flex_1().child(
        view()
            .flex_col()
            .gap_4()
            .child(layout::page_header(
                || t!("appearance.title"),
                || t!("appearance.subtitle"),
            ))
            .child(
                view()
                    .flex_col()
                    .gap_2()
                    .child(layout::section_title(|| t!("appearance.section.theme")))
                    .child(
                        view()
                            .grid()
                            .grid_cols(move || if window().width.0 < 1080.0 { 2_u16 } else { 4 })
                            .gap_3()
                            .children(ThemeId::ALL.iter().map(|id| {
                                let prefs = prefs.clone();
                                let prefs_for_press = prefs.clone();
                                let prefs_for_card = prefs.clone();
                                let theme_id = *id;
                                let card = button::button(button::ButtonVariant::Ghost)
                                    .size(button::ButtonSize::Content)
                                    .test_id(match theme_id {
                                        ThemeId::Amethyst => "theme-amethyst",
                                        ThemeId::Indigo => "theme-indigo",
                                        ThemeId::Ocean => "theme-ocean",
                                        ThemeId::Forest => "theme-forest",
                                        ThemeId::Sand => "theme-sand",
                                        ThemeId::Rose => "theme-rose",
                                        ThemeId::Graphite => "theme-graphite",
                                        ThemeId::Cyan => "theme-cyan",
                                        ThemeId::Orange => "theme-orange",
                                        ThemeId::Teal => "theme-teal",
                                        ThemeId::Crimson => "theme-crimson",
                                        ThemeId::Gold => "theme-gold",
                                        ThemeId::Slate => "theme-slate",
                                    })
                                    .on_press(move || prefs_for_press.apply_theme(theme_id))
                                    .prefix(theme_preview::theme_preview(theme_id, move || {
                                        prefs_for_card.theme.get() == theme_id
                                    }));
                                // 指向即时预览（只换 token、不落盘），离开回到
                                // 已保存主题（DESIGN.md 4.3）。
                                view()
                                    .on_mouse_enter(move |_: &MouseEvent| {
                                        super::theme::apply(theme_id);
                                    })
                                    .on_mouse_leave(move |_: &MouseEvent| {
                                        super::theme::apply(prefs.theme.get());
                                    })
                                    .child(card)
                            })),
                    )
                    .child(
                        text(move || t!("appearance.hover_preview"))
                            .text_size(px(12.0))
                            .text_color(move || theme().muted_foreground),
                    ),
            )
            .child(mode_section())
            .child(language_section())
            .child(motion_section())
            .child(
                text(move || {
                    t!(
                        "appearance.scale_note",
                        percent = format!("{:.0}", font_scale() * 100.0)
                    )
                })
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
            )
            .child(feedback::status_bar(
                text(move || t!("common.selected", name = current_label(prefs.theme.get()))),
                text(move || {
                    let name = if dark() {
                        t!("appearance.mode.dark")
                    } else {
                        t!("appearance.mode.light")
                    };
                    t!("common.current", name = name)
                }),
            )),
    )
}

fn current_label(id: ThemeId) -> String {
    id.id_name().to_owned()
}

fn mode_section() -> AnyElement {
    let prefs = prefs::prefs();
    let mode = prefs.mode;
    view()
        .flex_col()
        .gap_2()
        .child(layout::section_title(|| t!("appearance.section.mode")))
        .child(view().test_id("mode-group").flex_col().gap_2().children(
            prefs::Mode::ALL.iter().map(|target| {
                let prefs = prefs.clone();
                let target = *target;
                let test_id = match target {
                    prefs::Mode::Light => "appearance-light",
                    prefs::Mode::Dark => "appearance-dark",
                    prefs::Mode::System => "appearance-system",
                };
                select::radio_row(
                    move || mode.get() == target,
                    move || t!(target.label_key()),
                    test_id,
                    move || prefs.apply_mode(target),
                )
            }),
        ))
        .into_element()
}

fn language_section() -> AnyElement {
    let prefs = prefs::prefs();
    let language = prefs.language;
    view()
        .flex_col()
        .gap_2()
        .child(layout::section_title(|| t!("appearance.section.language")))
        .child(
            view()
                .test_id("language-group")
                .flex_col()
                .gap_2()
                .children(prefs::LanguagePref::ALL.iter().map(|target| {
                    let prefs = prefs.clone();
                    let target = *target;
                    let test_id = match target {
                        prefs::LanguagePref::System => "language-system",
                        prefs::LanguagePref::ZhCn => "language-zh-cn",
                        prefs::LanguagePref::En => "language-en",
                    };
                    select::radio_row(
                        move || language.get() == target,
                        move || t!(target.label_key()),
                        test_id,
                        move || prefs.apply_language(target),
                    )
                })),
        )
        .into_element()
}

fn motion_section() -> AnyElement {
    let prefs = prefs::prefs();
    let motion = prefs.motion;
    view()
        .flex_col()
        .gap_2()
        .child(layout::section_title(|| t!("appearance.section.motion")))
        .child(view().test_id("motion-group").flex_col().gap_2().children(
            prefs::MotionPref::ALL.iter().map(|target| {
                let prefs = prefs.clone();
                let target = *target;
                let test_id = match target {
                    prefs::MotionPref::System => "motion-system",
                    prefs::MotionPref::Reduce => "motion-reduce",
                };
                select::radio_row(
                    move || motion.get() == target,
                    move || t!(target.label_key()),
                    test_id,
                    move || prefs.apply_motion(target),
                )
            }),
        ))
        .into_element()
}
