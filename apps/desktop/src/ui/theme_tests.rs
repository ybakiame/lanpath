use super::*;

/// 每组必须完整定义的语义 token（浅色/深色同名同序）。
const REQUIRED: &[&str] = &[
    "background",
    "foreground",
    "card",
    "card-foreground",
    "popover",
    "popover-foreground",
    "primary",
    "primary-foreground",
    "primary-hover",
    "primary-hover-foreground",
    "primary-active",
    "primary-active-foreground",
    "secondary",
    "secondary-foreground",
    "secondary-hover",
    "secondary-hover-foreground",
    "secondary-active",
    "secondary-active-foreground",
    "muted",
    "muted-foreground",
    "accent",
    "accent-foreground",
    "accent-hover",
    "accent-hover-foreground",
    "accent-active",
    "accent-active-foreground",
    "destructive",
    "destructive-foreground",
    "destructive-hover",
    "destructive-hover-foreground",
    "destructive-active",
    "destructive-active-foreground",
    "border",
    "input",
    "ring",
    "sidebar",
    "sidebar-foreground",
    "sidebar-primary",
    "sidebar-primary-foreground",
    "sidebar-accent",
    "sidebar-accent-foreground",
    "sidebar-border",
    "sidebar-ring",
    "warning",
    "warning-foreground",
    "success",
    "success-foreground",
    "chart-1",
    "chart-2",
    "chart-3",
    "chart-4",
    "chart-5",
    "chart-upload",
    "chart-download",
    "switch-thumb",
    "success-surface",
    "warning-surface",
    "danger-surface",
    "path-line",
    "modal-scrim",
];

const FILLED_PAIRS: &[(&str, &str)] = &[
    ("primary", "primary-foreground"),
    ("secondary", "secondary-foreground"),
    ("accent", "accent-foreground"),
    ("destructive", "destructive-foreground"),
    ("sidebar-primary", "sidebar-primary-foreground"),
    ("sidebar-accent", "sidebar-accent-foreground"),
    ("warning", "warning-foreground"),
    ("success", "success-foreground"),
    ("primary-hover", "primary-hover-foreground"),
    ("primary-active", "primary-active-foreground"),
    ("secondary-hover", "secondary-hover-foreground"),
    ("secondary-active", "secondary-active-foreground"),
    ("accent-hover", "accent-hover-foreground"),
    ("accent-active", "accent-active-foreground"),
    ("destructive-hover", "destructive-hover-foreground"),
    ("destructive-active", "destructive-active-foreground"),
];

fn color_of(id: ThemeId, name: &str, dark: bool) -> Hsla {
    let tokens = theme_tokens(id);
    let color = tokens.color(name).unwrap_or_else(|| panic!("{name}"));
    if dark { color.dark } else { color.light }
}

fn assert_contrast(id: ThemeId, dark: bool, foreground: &str, background: &str, minimum: f32) {
    let ratio = contrast(
        color_of(id, foreground, dark),
        color_of(id, background, dark),
    );
    assert!(
        ratio >= minimum,
        "{} {}: {foreground} / {background} = {ratio:.3}:1，要求至少 {minimum}:1",
        id.id_name(),
        if dark { "dark" } else { "light" },
    );
}

#[test]
fn fourteen_themes_with_complete_pairs() {
    assert_eq!(ThemeId::ALL.len(), 14);
    let mut names: Vec<&str> = ThemeId::ALL.iter().map(|id| id.id_name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 14, "主题 ID 必须唯一");
    let mut required = REQUIRED.to_vec();
    required.sort_unstable();
    for id in ThemeId::ALL {
        let mut light: Vec<&str> = palette(id, false)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let mut dark: Vec<&str> = palette(id, true)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        light.sort_unstable();
        dark.sort_unstable();
        assert_eq!(
            light,
            dark,
            "{} 浅色/深色 palette key 集不一致",
            id.id_name()
        );
        assert_eq!(
            light,
            required,
            "{} palette 必须自行定义全部语义 key，且无额外/重复 key",
            id.id_name()
        );
    }
    // 存储值往返：未知值回退默认主题，不 panic。
    assert_eq!(ThemeId::from_id_name("ocean"), ThemeId::Ocean);
    assert_eq!(ThemeId::from_id_name("mint"), ThemeId::Mint);
    assert_eq!(ThemeId::from_id_name("nope"), ThemeId::Amethyst);
}

#[test]
fn mint_theme_is_high_saturation_and_fresh_in_both_modes() {
    let light_primary = color_of(ThemeId::Mint, "primary", false);
    let dark_primary = color_of(ThemeId::Mint, "primary", true);
    let light_bg = color_of(ThemeId::Mint, "background", false);
    let light_accent = color_of(ThemeId::Mint, "accent", false);
    assert!(
        light_primary.s >= 0.90,
        "mint 浅色主色应保持高饱和（s={}）",
        light_primary.s
    );
    assert!(
        dark_primary.s >= 0.85,
        "mint 深色主色应保持高饱和（s={}）",
        dark_primary.s
    );
    assert!(
        light_bg.l >= 0.95 && light_bg.s >= 0.40,
        "mint 浅色背景应明亮清透（l={}, s={}）",
        light_bg.l,
        light_bg.s
    );
    assert!(
        light_accent.s >= 0.65,
        "mint 浅色强调面应轻快鲜活（s={}）",
        light_accent.s
    );
}

#[test]
fn light_backgrounds_are_bright_never_dirty_gray() {
    for id in ThemeId::ALL {
        let bg = color_of(id, "background", false);
        assert!(bg.l >= 0.95, "{} 浅色背景过暗（l={}）", id.id_name(), bg.l);
        let card = color_of(id, "card", false);
        assert!(
            card.l >= bg.l,
            "{} 浅色卡片必须亮于或等于背景",
            id.id_name()
        );
        // 微色相：除刻意中性的 graphite 外，浅色背景带主题色相。
        if id != ThemeId::Graphite {
            assert!(bg.s >= 0.02, "{} 浅色背景丢失了色相", id.id_name());
        }
    }
}

#[test]
fn surface_text_pairs_meet_wcag_aa() {
    for id in ThemeId::ALL {
        for dark in [false, true] {
            for (surface, body) in [
                ("background", "foreground"),
                ("card", "card-foreground"),
                ("sidebar", "sidebar-foreground"),
                ("popover", "popover-foreground"),
            ] {
                for text in [body, "muted-foreground"] {
                    assert_contrast(id, dark, text, surface, 4.5);
                }
            }
            assert_contrast(id, dark, "secondary-foreground", "background", 4.5);
        }
    }
}

#[test]
fn filled_semantic_pairs_meet_wcag_aa() {
    for id in ThemeId::ALL {
        for dark in [false, true] {
            for (surface, foreground) in FILLED_PAIRS {
                assert_contrast(id, dark, foreground, surface, 4.5);
            }
        }
    }
}

#[test]
fn action_states_have_visible_ordered_feedback() {
    for id in ThemeId::ALL {
        for dark in [false, true] {
            for base in ["primary", "secondary", "accent", "destructive"] {
                let normal = color_of(id, base, dark);
                let hover = color_of(id, &format!("{base}-hover"), dark);
                let active = color_of(id, &format!("{base}-active"), dark);
                let hover_change = (luminance(hover) - luminance(normal)).abs();
                let active_change = (luminance(active) - luminance(normal)).abs();
                assert!(
                    hover_change > 0.0,
                    "{} {base} hover 必须与 normal 可区分",
                    id.id_name()
                );
                assert!(
                    active_change > hover_change,
                    "{} {base} active 必须强于 hover",
                    id.id_name()
                );
            }
        }
    }
}

#[test]
fn focus_rings_and_input_boundaries_meet_non_text_contrast() {
    for id in ThemeId::ALL {
        for dark in [false, true] {
            for surface in ["background", "card", "sidebar", "popover"] {
                assert_contrast(id, dark, "ring", surface, 3.0);
                assert_contrast(id, dark, "input", surface, 3.0);
            }
            assert_contrast(id, dark, "sidebar-ring", "sidebar", 3.0);
        }
    }
}

#[test]
fn contrast_and_luminance_match_known_vectors() {
    let black = hsl(0.0, 0.0, 0.0);
    let white = hsl(0.0, 0.0, 1.0);
    let gray = hsl(0.0, 0.0, 0.5);
    // 单精度 Hsla→sRGB 路径带着约 2e-6 的舍入误差，容差保留该量级。
    let epsilon = 1e-5;
    assert!(luminance(black).abs() <= epsilon);
    assert!((luminance(white) - 1.0).abs() <= epsilon);
    // #808080 的 sRGB 线性通道值，即 WCAG 相对亮度。
    assert!((luminance(gray) - 0.21404114).abs() <= epsilon);
    let black_white = contrast(black, white);
    assert!(
        (black_white - 21.0).abs() <= epsilon,
        "black/white contrast = {black_white}"
    );
    assert!((contrast(gray, gray) - 1.0).abs() <= epsilon);
}

#[test]
fn radius_tokens_follow_design_scale() {
    let tokens = theme_tokens(ThemeId::Amethyst);
    assert_eq!(tokens.number("radius"), Some(6.0));
    assert_eq!(tokens.number("radius-sm"), Some(4.0));
    assert_eq!(tokens.number("radius-lg"), Some(8.0));
}

#[test]
fn degree_hues_convert_to_nsh_normalized_hues() {
    let amethyst = nsh_core::Rgba::from(color_of(ThemeId::Amethyst, "primary", false));
    assert!(amethyst.b > amethyst.r && amethyst.r > amethyst.g);
    let ocean = nsh_core::Rgba::from(color_of(ThemeId::Ocean, "primary", false));
    assert!(ocean.b > ocean.g && ocean.g > ocean.r);
}

#[test]
fn modal_scrim_has_visible_alpha_in_every_appearance() {
    for id in ThemeId::ALL {
        for dark in [false, true] {
            assert!((color_of(id, "modal-scrim", dark).a - 0.45).abs() < 0.001);
        }
    }
}
