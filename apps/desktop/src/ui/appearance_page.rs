//! 外观设置页（阶段 0 重构）：13 主题网格（4 列，点击应用并保存）、
//! 外观模式、界面语言、动效偏好。界面缩放为诚实的不支持项——NSH
//! 0.5.1 的 `font_scale` 仅平台可读，此处只展示当前系统文本缩放。

use nsh_base::theme;
use nsh_core::element::desktop::main_window;
use nsh_core::prelude::*;
use nsh_core::storage::StorageFull;

use super::{button, feedback, layout, prefs, select, theme::ThemeId, theme_preview};

/// 外观页。
pub fn appearance_page() -> impl IntoElement {
    let prefs = prefs::prefs();
    let preview = signal(None::<ThemeId>);
    let theme_error = signal(None::<String>);
    // 指针悬停预览只能短暂改变全窗 token；页面销毁或窗口失焦必须回到
    // 最后确认并已持久化的主题，避免预览泄漏。
    cleanup(move || super::theme::apply(prefs.theme.get_untracked()));
    let preview_for_focus = preview;
    main_window().on_focus_change(move |focused| {
        if !focused && preview_for_focus.get_untracked().is_some() {
            preview_for_focus.set(None);
            super::theme::apply(prefs.theme.get_untracked());
        }
    });
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
                            .w_full()
                            .app_class("justify-items-stretch")
                            .grid_cols(move || if window().width.0 < 1080.0 { 2_u16 } else { 4 })
                            .gap_3()
                            .children(ThemeId::ALL.iter().map(|id| {
                                let theme_id = *id;
                                let preview_enter = preview;
                                let preview_leave = preview;
                                let preview_press = preview;
                                let error = theme_error;
                                let card = button::button(button::ButtonVariant::Ghost)
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
                                            super::theme::apply(prefs.theme.get_untracked());
                                        }
                                        finish_apply(result, error);
                                    })
                                    .prefix(move || {
                                        theme_preview::theme_preview(theme_id, move || {
                                            prefs.theme.get() == theme_id
                                        })
                                    });
                                // 指向即时预览（只换 token、不落盘），离开回到
                                // 已保存主题；后进入的卡片优先，旧卡片离场不覆盖。
                                view()
                                    .w_full()
                                    .on_mouse_enter(move |_: &MouseEvent| {
                                        preview_enter.set(Some(theme_id));
                                        super::theme::apply(theme_id);
                                    })
                                    .on_mouse_leave(move |_: &MouseEvent| {
                                        if preview_leave.get_untracked() == Some(theme_id) {
                                            preview_leave.set(None);
                                            super::theme::apply(prefs.theme.get_untracked());
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
                    .child(
                        text(move || theme_error.get().unwrap_or_default())
                            .text_size(px(12.0))
                            .text_color(move || theme().destructive),
                    ),
            )
            .child(mode_section())
            .child(language_section())
            .child(motion_section())
            .child(material_note_section())
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

fn theme_test_id(id: ThemeId) -> &'static str {
    match id {
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
    }
}

/// 同步保存结果的就近反馈：成功清除本组错误，失败保持上一有效值并
/// 显示可恢复的说明；toast 与内联文字使用同一消息。
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
        .child(select::radio_group(view().test_id("mode-group").children(
            prefs::Mode::ALL.iter().map(|target| {
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
                    move || finish_apply(prefs.apply_mode(target), error),
                )
            }),
        )))
        .child(
            text(move || error.get().unwrap_or_default())
                .text_size(px(12.0))
                .text_color(move || theme().destructive),
        )
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
            view()
                .test_id("language-group")
                .children(prefs::LanguagePref::ALL.iter().map(|target| {
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
                        move || finish_apply(prefs.apply_language(target), error),
                    )
                })),
        ))
        .child(
            text(move || error.get().unwrap_or_default())
                .text_size(px(12.0))
                .text_color(move || theme().destructive),
        )
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
            view()
                .test_id("motion-group")
                .children(prefs::MotionPref::ALL.iter().map(|target| {
                    let target = *target;
                    let test_id = match target {
                        prefs::MotionPref::System => "motion-system",
                        prefs::MotionPref::Reduce => "motion-reduce",
                    };
                    select::radio_row(
                        move || motion.get() == target,
                        move || t!(target.label_key()),
                        test_id,
                        move || finish_apply(prefs.apply_motion(target), error),
                    )
                })),
        ))
        .child(
            text(move || error.get().unwrap_or_default())
                .text_size(px(12.0))
                .text_color(move || theme().destructive),
        )
        .into_element()
}

/// 窗口材质/不透明度：NSH 0.5.1 的诚实边界（调查结论）。主窗口 backdrop
/// 仅是 nsh.json 启动配置，应用内没有运行时切换窗口材质或窗口不透明度的
/// API；与界面缩放一样只说明、不提供存而不生效的控件。
fn material_note_section() -> impl IntoElement {
    view()
        .flex_col()
        .gap_2()
        .child(layout::section_title(|| t!("appearance.section.material")))
        .child(
            text(move || t!("appearance.material_note"))
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .into_element()
}
