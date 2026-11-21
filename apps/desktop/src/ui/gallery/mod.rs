//! Development-only component gallery, opened with --ui-gallery.
//! Inspired by NSH official Gallery and Doona: desktop sidebar navigation,
//! live appearance/theme/motion controls bar, full component matrix and token swatches.

mod controls;
mod data_theme_motion;
mod feedback_overlays;
mod selection;
mod surfaces;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::icon::{Icon, Tone, icon_sized};
use super::{ThemeId, button, feedback, layout, prefs, select};

/// 画廊左侧导航分类；默认 `All` 展示全部验收分区，点选单项可聚焦精查。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GallerySection {
    All,
    Theme,
    Icons,
    Buttons,
    Inputs,
    Selection,
    Surfaces,
    Feedback,
    Overlays,
    Data,
    Motion,
}

impl GallerySection {
    fn show(self, target: GallerySection) -> bool {
        self == GallerySection::All || self == target
    }
}

/// 画廊根组件（`--ui-gallery` 入口）。
#[component]
pub fn Gallery() -> impl IntoElement {
    // 与 App 同一条外观安装链：存储的主题/语言/模式在画廊同样生效。
    prefs::prefs().install();
    gallery_view()
}

/// 画廊内容。
pub fn gallery_view() -> impl IntoElement {
    let active = signal(GallerySection::All);
    view()
        .relative()
        .flex_1()
        .flex_col()
        .bg(prefs::window_bg)
        .text_color(move || theme().foreground)
        .child(super::title_bar::title_bar())
        .child(
            view()
                .flex_1()
                .min_h(px(0.0))
                .flex_row()
                .child(when(
                    move || window().width.0 >= 860.0,
                    move || gallery_sidebar(active),
                ))
                .child(
                    scroll().flex_1().min_w(px(0.0)).child(
                        view()
                            .flex_col()
                            .gap_6()
                            .p_6()
                            .max_w(px(1160.0))
                            .child(gallery_header())
                            .child(when(
                                move || active.get().show(GallerySection::Buttons),
                                controls::buttons_section,
                            ))
                            .child(when(
                                move || active.get().show(GallerySection::Icons),
                                surfaces::icons_section,
                            ))
                            .child(when(
                                move || active.get().show(GallerySection::Inputs),
                                controls::inputs_section,
                            ))
                            .child(when(
                                move || active.get().show(GallerySection::Selection),
                                selection::selection_section,
                            ))
                            .child(when(
                                move || active.get().show(GallerySection::Surfaces),
                                surfaces::surfaces_section,
                            ))
                            .child(when(
                                move || active.get().show(GallerySection::Feedback),
                                feedback_overlays::feedback_section,
                            ))
                            .child(when(
                                move || active.get().show(GallerySection::Overlays),
                                feedback_overlays::overlays_section,
                            ))
                            .child(when(
                                move || active.get().show(GallerySection::Data),
                                data_theme_motion::data_section,
                            ))
                            .child(when(
                                move || active.get().show(GallerySection::Theme),
                                data_theme_motion::theme_section,
                            ))
                            .child(when(
                                move || active.get().show(GallerySection::Motion),
                                data_theme_motion::motion_section,
                            )),
                    ),
                ),
        )
        .child(feedback::toast_host())
}

fn gallery_header() -> AnyElement {
    let prefs = prefs::prefs();
    let mode = prefs.mode;
    let current_theme = prefs.theme;
    let motion = prefs.motion;
    let material = prefs.material;
    view()
        .flex_col()
        .gap_4()
        .child(layout::page_header(
            || t!("gallery.title"),
            || t!("gallery.subtitle"),
        ))
        .child(
            layout::surface(layout::SurfaceLevel::Card)
                .p_3()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap_3()
                        .child(select::segmented_group(
                            "gallery-mode-group",
                            prefs::Mode::ALL
                                .iter()
                                .map(|target| {
                                    let target = *target;
                                    let test_id = match target {
                                        prefs::Mode::System => "gallery-mode-system",
                                        prefs::Mode::Light => "gallery-mode-light",
                                        prefs::Mode::Dark => "gallery-mode-dark",
                                    };
                                    select::segmented_radio(
                                        move || mode.get() == target,
                                        move || t!(target.label_key()),
                                        test_id,
                                        move || {
                                            if prefs.apply_mode(target).is_err() {
                                                feedback::show_toast(t!("appearance.save_failed"));
                                            }
                                        },
                                    )
                                })
                                .collect(),
                        ))
                        .child(select::segmented_group(
                            "gallery-backdrop-group",
                            prefs::WindowMaterial::ALL
                                .iter()
                                .map(|target| {
                                    let target = *target;
                                    let test_id = match target {
                                        prefs::WindowMaterial::Acrylic => {
                                            "gallery-backdrop-acrylic"
                                        }
                                        prefs::WindowMaterial::Mica => "gallery-backdrop-mica",
                                        prefs::WindowMaterial::Solid => "gallery-backdrop-solid",
                                    };
                                    select::segmented_radio(
                                        move || material.get() == target,
                                        move || t!(target.label_key()),
                                        test_id,
                                        move || {
                                            if prefs.apply_material(target).is_err() {
                                                feedback::show_toast(t!("appearance.save_failed"));
                                            }
                                        },
                                    )
                                })
                                .collect(),
                        ))
                        .child(select::segmented_group(
                            "gallery-motion-group",
                            prefs::MotionPref::ALL
                                .iter()
                                .map(|target| {
                                    let target = *target;
                                    let test_id = match target {
                                        prefs::MotionPref::System => "gallery-motion-system",
                                        prefs::MotionPref::Reduce => "gallery-motion-reduce",
                                    };
                                    select::segmented_radio(
                                        move || motion.get() == target,
                                        move || t!(target.label_key()),
                                        test_id,
                                        move || {
                                            if prefs.apply_motion(target).is_err() {
                                                feedback::show_toast(t!("appearance.save_failed"));
                                            }
                                        },
                                    )
                                })
                                .collect(),
                        )),
                )
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(feedback::badge(
                            feedback::BadgeVariant::Outline,
                            move || {
                                t!(
                                    "gallery.state.active_theme",
                                    name = current_theme.get().id_name()
                                )
                            },
                        ))
                        .child(
                            button::button(button::ButtonVariant::Primary)
                                .size(button::ButtonSize::Sm)
                                .test_id("gallery-quick-mint")
                                .prefix(|| icon_sized(Icon::Sparkles, Tone::OnPrimary, 14.0))
                                .label(move || {
                                    if current_theme.get() == ThemeId::Mint {
                                        t!("gallery.controls.theme_ocean")
                                    } else {
                                        t!("gallery.controls.theme_mint")
                                    }
                                })
                                .on_press(move || {
                                    let next = if current_theme.get_untracked() == ThemeId::Mint {
                                        ThemeId::Ocean
                                    } else {
                                        ThemeId::Mint
                                    };
                                    if prefs.apply_theme(next).is_err() {
                                        feedback::show_toast(t!("appearance.save_failed"));
                                    }
                                }),
                        ),
                ),
        )
        .into_element()
}

fn gallery_sidebar(active: Signal<GallerySection>) -> AnyElement {
    let nav_group = |key: &'static str| {
        text(move || t!(key))
            .text_size(px(12.0))
            .text_color(move || theme().muted_foreground)
            .px_3()
            .pt_3()
            .pb_1()
    };
    let nav_row = move |target: GallerySection,
                        icon_kind: Icon,
                        label_key: &'static str,
                        test_id: &'static str| {
        let selected = move || active.get() == target;
        let reduced = prefs::prefs().reduced;
        view().test_id(test_id).child(
            nsh_base::button()
                .selected(selected)
                .w_full()
                .min_h(px(34.0))
                .flex_row()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .rounded(move || theme().radius)
                .cursor(Cursor::Default)
                .transition_colors()
                .duration(move || {
                    super::motion::motion_duration(super::motion::INSTANT_MS, reduced.get())
                })
                .bg(move || {
                    if selected() {
                        theme().sidebar_accent
                    } else {
                        transparent_black().into()
                    }
                })
                .text_color(move || {
                    if selected() {
                        theme().sidebar_accent_foreground
                    } else {
                        theme().sidebar_foreground
                    }
                })
                .hover(move |s| {
                    s.bg(move || {
                        if selected() {
                            theme().sidebar_accent
                        } else {
                            theme().muted
                        }
                    })
                })
                .active(|s| s.bg(move || theme().accent))
                .focus_visible(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
                .on_press(move || active.set(target))
                .child(
                    when(selected, move || icon_sized(icon_kind, Tone::Primary, 15.0))
                        .otherwise(move || icon_sized(icon_kind, Tone::Muted, 15.0)),
                )
                .child(
                    text(move || t!(label_key))
                        .text_size(px(13.0))
                        .style_when(selected, |s| s.font_weight(FontWeight::MEDIUM))
                        .truncate()
                        .flex_1()
                        .min_w(px(0.0)),
                ),
        )
    };

    view()
        .test_id("gallery-sidebar")
        .w(px(220.0))
        .flex_none()
        .min_h(px(0.0))
        .bg(prefs::sidebar_bg)
        .border_r(px(1.0))
        .border_color(move || theme().sidebar_border)
        .child(
            scroll().flex_1().min_h(px(0.0)).child(
                view()
                    .flex_col()
                    .p_3()
                    .gap_1()
                    .child(nav_row(
                        GallerySection::All,
                        Icon::Overview,
                        "gallery.nav.all",
                        "gallery-nav-all",
                    ))
                    .child(nav_group("gallery.nav.group.foundations"))
                    .child(nav_row(
                        GallerySection::Theme,
                        Icon::Appearance,
                        "gallery.section.theme",
                        "gallery-nav-theme",
                    ))
                    .child(nav_row(
                        GallerySection::Icons,
                        Icon::Sparkles,
                        "gallery.section.icons",
                        "gallery-nav-icons",
                    ))
                    .child(nav_row(
                        GallerySection::Motion,
                        Icon::Activity,
                        "gallery.section.motion",
                        "gallery-nav-motion",
                    ))
                    .child(nav_group("gallery.nav.group.controls"))
                    .child(nav_row(
                        GallerySection::Buttons,
                        Icon::Play,
                        "gallery.section.buttons",
                        "gallery-nav-buttons",
                    ))
                    .child(nav_row(
                        GallerySection::Inputs,
                        Icon::Code,
                        "gallery.section.inputs",
                        "gallery-nav-inputs",
                    ))
                    .child(nav_row(
                        GallerySection::Selection,
                        Icon::Sliders,
                        "gallery.section.selection",
                        "gallery-nav-selection",
                    ))
                    .child(nav_group("gallery.nav.group.display"))
                    .child(nav_row(
                        GallerySection::Surfaces,
                        Icon::Layers,
                        "gallery.section.surfaces",
                        "gallery-nav-surfaces",
                    ))
                    .child(nav_row(
                        GallerySection::Feedback,
                        Icon::Shield,
                        "gallery.section.feedback",
                        "gallery-nav-feedback",
                    ))
                    .child(nav_row(
                        GallerySection::Overlays,
                        Icon::File,
                        "gallery.section.overlays",
                        "gallery-nav-overlays",
                    ))
                    .child(nav_row(
                        GallerySection::Data,
                        Icon::Logs,
                        "gallery.section.data",
                        "gallery-nav-data",
                    )),
            ),
        )
        .into_element()
}

/// 分区卡片：标题 + 内容槽。
pub(crate) fn section(title_key: &'static str, content: impl IntoElement) -> AnyElement {
    layout::surface(layout::SurfaceLevel::Card)
        .p_5()
        .flex_col()
        .gap_4()
        .w_full()
        .child(layout::section_title(|| t!(title_key)))
        .child(content)
        .into_element()
}
