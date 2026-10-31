use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Hsla, WindowAppearance, px, rgb};

use super::{AccentPreference, AppearancePreferences};

const WINDOW_SURFACE_OPACITY: f32 = 0.86;

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
        let pure_black = preferences.effective_pure_black(appearance);
        if pure_black {
            apply_pure_black(theme);
        }
        apply_accent(theme, preferences.accent(), dark);
        theme.overlay = theme.background.opacity(0.72);
        if cfg!(windows) && !pure_black {
            apply_window_material(theme);
        }
    });
}

fn apply_window_material(theme: &mut Theme) {
    theme.background = theme.background.opacity(WINDOW_SURFACE_OPACITY);
    theme.title_bar = theme.title_bar.opacity(WINDOW_SURFACE_OPACITY);
    theme.sidebar = theme.sidebar.opacity(WINDOW_SURFACE_OPACITY);
}

fn apply_accent(theme: &mut Theme, accent: AccentPreference, dark: bool) {
    // Product palette from packages/ui/src/appearance.css; accents are a separate
    // semantic axis, never replacements for danger/warning/success/info colors.
    let color: Hsla = rgb(match (accent, dark) {
        (AccentPreference::Iris, false) => 0x6550c9,
        (AccentPreference::Teal, false) => 0x087e80,
        (AccentPreference::Rose, false) => 0xb63d72,
        (AccentPreference::Blue, false) => 0x2463c4,
        (AccentPreference::Iris, true) => 0xb5a4ff,
        (AccentPreference::Teal, true) => 0x79d9cf,
        (AccentPreference::Rose, true) => 0xf4a0c0,
        (AccentPreference::Blue, true) => 0x97beff,
    })
    .into();
    let foreground: Hsla = rgb(if dark { 0x181823 } else { 0xffffff }).into();
    let hover_mix: Hsla = rgb(if dark { 0xffffff } else { 0x000000 }).into();
    let hover = color.blend(hover_mix.opacity(0.1));
    let active = color.blend(hover_mix.opacity(0.16));
    let soft = theme
        .background
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
}
