//! Development-only component gallery, opened with --ui-gallery.
//! Covers shared variants, states, themes, languages and reduced motion.
//! System text scaling is reflected without an application-level override.
//! Sections live in sections.rs; Windows UIA and screenshots verify the result.

mod sections;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::{ThemeId, button, feedback, layout, prefs, theme_preview};
use sections::{
    buttons_section, data_section, feedback_section, inputs_section, overlays_section,
    selection_section,
};

/// 画廊根组件（`--ui-gallery` 入口）。
#[component]
pub fn Gallery() -> impl IntoElement {
    // 与 App 同一条外观安装链：存储的主题/语言/模式在画廊同样生效。
    prefs::prefs().install();
    gallery_view()
}

/// 画廊内容。
pub fn gallery_view() -> impl IntoElement {
    view()
        .flex_1()
        .flex_col()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(super::title_bar::title_bar())
        .child(
            scroll().flex_1().child(
                view()
                    .flex_col()
                    .gap_6()
                    .p_6()
                    .child(layout::page_header(
                        || t!("gallery.title"),
                        || t!("gallery.subtitle"),
                    ))
                    .child(buttons_section())
                    .child(inputs_section())
                    .child(selection_section())
                    .child(feedback_section())
                    .child(overlays_section())
                    .child(data_section())
                    .child(theme_section())
                    .child(motion_section()),
            ),
        )
        .child(feedback::toast_host())
}

/// 分区卡片：标题 + 内容槽。
pub(crate) fn section(title_key: &'static str, content: impl IntoElement) -> AnyElement {
    layout::surface(layout::SurfaceLevel::Card)
        .p_4()
        .flex_col()
        .gap_3()
        .w_full()
        .child(layout::section_title(|| t!(title_key)))
        .child(content)
        .into_element()
}

fn theme_section() -> AnyElement {
    section(
        "gallery.section.theme",
        view()
            .grid()
            .w_full()
            .app_class("justify-items-stretch")
            .grid_cols(move || if window().width.0 < 1080.0 { 2_u16 } else { 4 })
            .gap_3()
            .children(ThemeId::ALL.iter().map(|id| {
                let prefs = prefs::prefs();
                let prefs_for_press = prefs;
                let theme_id = *id;
                let test_id: &'static str = match theme_id {
                    ThemeId::Amethyst => "gallery-theme-amethyst",
                    ThemeId::Indigo => "gallery-theme-indigo",
                    ThemeId::Ocean => "gallery-theme-ocean",
                    ThemeId::Forest => "gallery-theme-forest",
                    ThemeId::Sand => "gallery-theme-sand",
                    ThemeId::Rose => "gallery-theme-rose",
                    ThemeId::Graphite => "gallery-theme-graphite",
                    ThemeId::Cyan => "gallery-theme-cyan",
                    ThemeId::Orange => "gallery-theme-orange",
                    ThemeId::Teal => "gallery-theme-teal",
                    ThemeId::Crimson => "gallery-theme-crimson",
                    ThemeId::Gold => "gallery-theme-gold",
                    ThemeId::Slate => "gallery-theme-slate",
                };
                button::button(button::ButtonVariant::Ghost)
                    .size(button::ButtonSize::Content)
                    .full_width()
                    .selected(move || prefs.theme.get() == theme_id)
                    .accessible_name(move || {
                        SharedString::from(t!("appearance.theme.a11y", name = theme_id.id_name()))
                    })
                    .test_id(test_id)
                    .on_press(move || {
                        if prefs_for_press.apply_theme(theme_id).is_err() {
                            feedback::show_toast(t!("appearance.save_failed"));
                        }
                    })
                    .prefix(move || {
                        theme_preview::theme_preview(theme_id, move || {
                            prefs.theme.get() == theme_id
                        })
                    })
            })),
    )
}

fn motion_section() -> AnyElement {
    let reduced = prefs::prefs().reduced;
    section(
        "gallery.section.motion",
        view()
            .flex_col()
            .gap_3()
            .child(
                view()
                    .flex_row()
                    .gap_4()
                    .flex_wrap()
                    .text_size(px(13.0))
                    .children(
                        [
                            "gallery.motion.instant",
                            "gallery.motion.fast",
                            "gallery.motion.base",
                            "gallery.motion.slow",
                            "gallery.motion.slower",
                            "gallery.motion.distance",
                            "gallery.motion.press",
                        ]
                        .map(|key| text(move || t!(key)).into_element()),
                    ),
            )
            .child(when(
                move || reduced.get(),
                || text(|| t!("gallery.motion.reduced_on")).text_size(px(12.0)),
            ))
            .child(motion_demo()),
    )
}

/// 动效演示：进入（slower 错峰淡入）、按下（fast pulse）、循环（spin）。
/// 全部由动效 token 驱动；reduced 时 `Motion::play` 立即落位。
fn motion_demo() -> AnyElement {
    let boxed = motion();
    let spin = motion();
    let spinning = signal(false);
    let play_press = boxed;
    let play_spin = spin;
    view()
        .flex_row()
        .gap_3()
        .items_center()
        .child(
            view()
                .bind_motion(&boxed)
                .size(px(56.0))
                .rounded(move || theme().radius)
                .bg(move || theme().primary)
                .items_center()
                .justify_center()
                .child(
                    text("Aa")
                        .text_size(px(18.0))
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
            button::button(button::ButtonVariant::Outline)
                .label(|| t!("gallery.motion.base"))
                .on_press(move || {
                    let _ = play_press.play(MotionPreset::Pulse);
                }),
        )
        .child(
            button::button(button::ButtonVariant::Outline)
                .label(|| t!("gallery.motion.demo"))
                .on_press(move || {
                    if prefs::prefs().reduced.get() {
                        // 应用内“减少动效”与系统设置同等生效：循环演示
                        // 不启动，也不留下一帧持续动画。
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
                        // 引擎级 reduced 时 `play` 落到末帧且不持续旋转。
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
