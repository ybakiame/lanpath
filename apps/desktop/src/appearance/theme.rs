use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Hsla, WindowAppearance, px, rgb};

use super::{AccentPreference, AppearancePreferences};

pub(super) fn apply(
    preferences: AppearancePreferences,
    appearance: WindowAppearance,
    cx: &mut App,
) {
    let dark = preferences.is_dark(appearance);
    // Always reload the base palette so leaving pure black restores every surface.
    Theme::change(
        if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
        None,
        cx,
    );
    Theme::update(cx, |theme| {
        // Theme base font is Root's rem reference, so controls and spacing zoom too.
        let scale = f32::from(preferences.scale_percent()) / 100.;
        theme.font_size = px(16. * scale);
        theme.mono_font_size = px(13. * scale);
        apply_surfaces(theme, dark);
        let pure_black = preferences.effective_pure_black(appearance);
        if pure_black {
            apply_pure_black(theme);
        }
        apply_accent(theme, preferences.accent(), dark);
        theme.overlay = theme.background.opacity(0.72);
        if preferences.uses_acrylic(appearance) {
            apply_window_material(theme, preferences.acrylic_opacity_percent());
        }
    });
}

fn apply_window_material(theme: &mut Theme, opacity_percent: u16) {
    // Root paints the backdrop once. Editors, menus and content surfaces stay
    // opaque for stable text contrast.
    let opacity = f32::from(opacity_percent) / 100.;
    theme.background = theme.background.opacity(opacity);
    theme.title_bar = theme.title_bar.opacity(opacity);
    theme.sidebar = theme.sidebar.opacity(opacity);
}

fn apply_surfaces(theme: &mut Theme, dark: bool) {
    let color = |light, dark_color| -> Hsla { rgb(if dark { dark_color } else { light }).into() };
    let background = color(0xf0f1f8, 0x2c2c3e);
    let surface = color(0xfafaff, 0x242434);
    let secondary = color(0xe7e9f3, 0x373748);
    let hover = color(0xdde0ee, 0x404052);
    let border = color(0xd3d6e5, 0x4c4c60);
    let foreground = color(0x29283a, 0xeeeef5);
    let muted_foreground = color(0x65647a, 0xaaaabd);
    theme.background = background;
    theme.foreground = foreground;
    theme.group_box = surface;
    theme.group_box_foreground = foreground;
    theme.popover = surface;
    theme.popover_foreground = foreground;
    theme.sidebar = background;
    theme.sidebar_foreground = foreground;
    theme.title_bar = background;
    theme.status_bar = background;
    theme.colors.list = surface;
    theme.list_even = surface;
    theme.list_head = secondary;
    theme.list_hover = secondary;
    theme.table = surface;
    theme.table_even = surface;
    theme.table_head = secondary;
    theme.table_foot = secondary;
    theme.table_hover = secondary;
    theme.tab = background;
    theme.tab_bar = background;
    theme.tab_bar_segmented = secondary;
    theme.tab_active = surface;
    theme.accordion = surface;
    theme.secondary = secondary;
    theme.secondary_foreground = foreground;
    theme.secondary_hover = hover;
    theme.secondary_active = hover;
    theme.button = surface;
    theme.button_foreground = foreground;
    theme.button_hover = secondary;
    theme.button_active = hover;
    theme.button_secondary = secondary;
    theme.button_secondary_foreground = foreground;
    theme.button_secondary_hover = hover;
    theme.button_secondary_active = hover;
    theme.muted = secondary;
    theme.muted_foreground = muted_foreground;
    theme.description_list_label = secondary;
    theme.description_list_label_foreground = muted_foreground;
    theme.skeleton = secondary;
    theme.switch = border;
    theme.border = border;
    theme.input = color(0x85859a, 0x8a8a9e);
    theme.sidebar_border = border;
    theme.title_bar_border = border;
    theme.status_bar_border = border;
    theme.table_row_border = border;
    theme.window_border = border;
}

fn apply_accent(theme: &mut Theme, accent: AccentPreference, dark: bool) {
    // Product palette from packages/ui/src/appearance.css; accents are a separate
    // semantic axis, never replacements for danger/warning/success/info colors.
    let color = accent_color(accent, dark);
    let foreground: Hsla = rgb(if dark { 0x181823 } else { 0xffffff }).into();
    let hover_mix: Hsla = rgb(if dark { 0xffffff } else { 0x000000 }).into();
    let hover = color.blend(hover_mix.opacity(0.1));
    let active = color.blend(hover_mix.opacity(0.16));
    let soft = theme
        .group_box
        .blend(color.opacity(if dark { 0.14 } else { 0.10 }));
    theme.primary = color;
    theme.primary_foreground = foreground;
    theme.primary_hover = hover;
    theme.primary_active = active;
    theme.button_primary = color;
    theme.button_primary_foreground = foreground;
    theme.button_primary_hover = hover;
    theme.button_primary_active = active;
    theme.ring = color;
    theme.caret = color;
    theme.selection = color.opacity(if dark { 0.30 } else { 0.24 });
    theme.accent = soft;
    theme.accent_foreground = color;
    theme.list_active = soft;
    theme.list_active_border = color;
    theme.table_active = soft;
    theme.table_active_border = color;
    theme.sidebar_primary = color;
    theme.sidebar_primary_foreground = foreground;
    theme.sidebar_accent = theme
        .sidebar
        .blend(color.opacity(if dark { 0.14 } else { 0.10 }));
    theme.sidebar_accent_foreground = color;
    theme.link = color;
    theme.link_hover = hover;
    theme.link_active = active;
    theme.progress_bar = color;
    theme.slider_bar = color;
    theme.slider_thumb = color;
    theme.tab_active_foreground = color;
    theme.drag_border = color;
    theme.drop_target = soft;
}

pub(crate) fn accent_color(accent: AccentPreference, dark: bool) -> Hsla {
    rgb(match (accent, dark) {
        (AccentPreference::Iris, false) => 0x6550c9,
        (AccentPreference::Teal, false) => 0x087e80,
        (AccentPreference::Rose, false) => 0xb63d72,
        (AccentPreference::Blue, false) => 0x2463c4,
        (AccentPreference::Iris, true) => 0xb5a4ff,
        (AccentPreference::Teal, true) => 0x79d9cf,
        (AccentPreference::Rose, true) => 0xf4a0c0,
        (AccentPreference::Blue, true) => 0x97beff,
    })
    .into()
}

pub(crate) fn preview_colors(dark: bool) -> (Hsla, Hsla, Hsla) {
    let mut theme = Theme::default();
    apply_surfaces(&mut theme, dark);
    (theme.background, theme.group_box, theme.border)
}

fn apply_pure_black(theme: &mut Theme) {
    // Matches the product's black-contrast surface/border roles. Text and status
    // palettes retain the framework's dark-theme contrast treatment.
    let black: Hsla = rgb(0x000000).into();
    let secondary: Hsla = rgb(0x141414).into();
    let tertiary: Hsla = rgb(0x1c1c1c).into();
    let hover: Hsla = rgb(0x151515).into();
    let border: Hsla = rgb(0x363636).into();
    theme.background = black;
    theme.group_box = black;
    theme.popover = black;
    theme.sidebar = black;
    theme.title_bar = black;
    theme.status_bar = black;
    theme.colors.list = black;
    theme.table = black;
    theme.tab = black;
    theme.tab_bar = black;
    theme.accordion = black;
    theme.description_list_label = secondary;
    theme.secondary = secondary;
    theme.secondary_hover = tertiary;
    theme.secondary_active = tertiary;
    theme.button = secondary;
    theme.button_hover = tertiary;
    theme.button_active = tertiary;
    theme.button_secondary = secondary;
    theme.button_secondary_hover = tertiary;
    theme.button_secondary_active = tertiary;
    theme.muted = secondary;
    theme.skeleton = tertiary;
    theme.switch = tertiary;
    theme.tab_active = secondary;
    theme.tab_bar_segmented = secondary;
    theme.list_even = secondary;
    theme.list_head = secondary;
    theme.list_hover = hover;
    theme.table_even = secondary;
    theme.table_head = secondary;
    theme.table_foot = secondary;
    theme.table_hover = hover;
    theme.border = border;
    theme.input = border;
    theme.sidebar_border = border;
    theme.title_bar_border = border;
    theme.status_bar_border = border;
    theme.table_row_border = border;
    theme.window_border = border;
}

#[cfg(test)]
mod tests {
    use gpui_kit::Rgba;

    use super::*;

    fn luminance(color: Hsla) -> f32 {
        let color = Rgba::from(color);
        let linear = |channel: f32| {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
    }

    #[test]
    fn accent_roles_keep_primary_contrast_and_do_not_recolor_statuses() {
        for dark in [false, true] {
            for accent in [
                AccentPreference::Iris,
                AccentPreference::Teal,
                AccentPreference::Rose,
                AccentPreference::Blue,
            ] {
                let mut theme = Theme::default();
                let status = (theme.danger, theme.warning, theme.success, theme.info);
                apply_accent(&mut theme, accent, dark);
                assert_eq!(
                    (theme.danger, theme.warning, theme.success, theme.info),
                    status
                );
                let background = luminance(theme.primary);
                let foreground = luminance(theme.primary_foreground);
                let contrast =
                    (background.max(foreground) + 0.05) / (background.min(foreground) + 0.05);
                assert!(
                    contrast >= 4.5,
                    "accent {accent:?}, dark={dark}: {contrast}"
                );
            }
        }
    }

    #[test]
    fn readable_surfaces_survive_dark_light_and_pure_black_palettes() {
        for dark in [false, true] {
            for black in [false, true] {
                let mut theme = Theme::default();
                apply_surfaces(&mut theme, dark);
                if dark && black {
                    apply_pure_black(&mut theme);
                }
                for surface in [theme.group_box, theme.background, theme.popover] {
                    for text in [theme.foreground, theme.muted_foreground] {
                        let a = luminance(surface);
                        let b = luminance(text);
                        assert!(
                            (a.max(b) + 0.05) / (a.min(b) + 0.05) >= 4.5,
                            "dark={dark}, black={black}, surface={surface:?}, text={text:?}"
                        );
                    }
                }
            }
        }
    }
}
