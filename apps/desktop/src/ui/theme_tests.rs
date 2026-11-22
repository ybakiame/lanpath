use super::oklch::{contrast, hsla_to_oklch, luminance, oklch as from_oklch, rgb_hex};
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
fn four_themes_with_complete_pairs_and_ocean_default() {
    assert_eq!(ThemeId::PRESETS.len(), 3);
    assert_eq!(ThemeId::ALL.len(), 4);
    let mut names: Vec<&str> = ThemeId::ALL.iter().map(|id| id.id_name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 4, "主题 ID 必须唯一");
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
    // 存储值往返：未知/历史已删除主题回退默认 Lanpath。
    assert_eq!(ThemeId::ALL[0], ThemeId::Lanpath);
    assert_eq!(ThemeId::from_id_name("lanpath"), ThemeId::Lanpath);
    assert_eq!(ThemeId::from_id_name("ocean"), ThemeId::Ocean);
    assert_eq!(ThemeId::from_id_name("teal"), ThemeId::Teal);
    assert_eq!(ThemeId::from_id_name("custom"), ThemeId::Custom);
    assert_eq!(ThemeId::from_id_name("amethyst"), ThemeId::Lanpath);
    assert_eq!(ThemeId::from_id_name("nope"), ThemeId::Lanpath);
}

#[test]
fn teal_primary_button_uses_crisp_white_text_in_both_modes() {
    for dark in [false, true] {
        let fg = color_of(ThemeId::Teal, "primary-foreground", dark);
        let bg = color_of(ThemeId::Teal, "primary", dark);
        assert!(
            luminance(fg) > 0.95,
            "teal {} 主按钮文字应为纯白（lum={}）",
            if dark { "dark" } else { "light" },
            luminance(fg)
        );
        assert!(
            contrast(fg, bg) >= 4.5,
            "teal {} 白字/主色对比度不足：{}",
            if dark { "dark" } else { "light" },
            contrast(fg, bg)
        );
    }
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
        assert!(bg.s >= 0.02, "{} 浅色背景丢失了微色相", id.id_name());
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
fn custom_oklch_derivation_enforces_wcag_even_on_extreme_inputs() {
    let extreme_seeds = [
        // 极端亮荧光黄 + 极浅文字 + 暗底输入
        CustomPalette {
            primary: rgb_hex(0xfef08a),
            accent: rgb_hex(0xfffbeb),
            background: rgb_hex(0x111111),
            foreground: rgb_hex(0xfafafa),
        },
        // 极端深黑 + 高饱和洋红
        CustomPalette {
            primary: rgb_hex(0x050505),
            accent: rgb_hex(0xff00aa),
            background: rgb_hex(0xffffff),
            foreground: rgb_hex(0xcccccc),
        },
        preset_seed(ThemeId::Teal),
        preset_seed(ThemeId::Lanpath),
    ];

    for seed in extreme_seeds {
        let serialized = seed.serialize();
        let parsed = CustomPalette::deserialize(&serialized);
        assert_eq!(parsed.serialize(), serialized);

        let tokens = custom_tokens(seed);
        for dark in [false, true] {
            let get = |name: &str| {
                let c = tokens.color(name).unwrap_or_else(|| panic!("{name}"));
                if dark { c.dark } else { c.light }
            };
            for (surface, fg) in FILLED_PAIRS {
                assert!(
                    contrast(get(fg), get(surface)) >= 4.5,
                    "custom {serialized} dark={dark}: {fg}/{surface} < 4.5"
                );
            }
            for surface in ["background", "card", "sidebar", "popover"] {
                assert!(contrast(get("foreground"), get(surface)) >= 4.5);
                assert!(contrast(get("muted-foreground"), get(surface)) >= 4.5);
                assert!(contrast(get("ring"), get(surface)) >= 3.0);
                assert!(contrast(get("input"), get(surface)) >= 3.0);
            }
        }
    }
}

#[test]
fn oklch_roundtrip_and_contrast_vectors_are_accurate() {
    let black = rgb_hex(0x000000);
    let white = rgb_hex(0xffffff);
    let gray = rgb_hex(0x808080);
    assert!(luminance(black).abs() <= 1e-5);
    assert!((luminance(white) - 1.0).abs() <= 1e-5);
    assert!((contrast(black, white) - 21.0).abs() <= 1e-4);
    assert!((contrast(gray, gray) - 1.0).abs() <= 1e-5);

    let sample = rgb_hex(0x245db5);
    let ok = hsla_to_oklch(sample);
    let back = from_oklch(ok.l, ok.c, ok.h);
    let a = nsh_core::Rgba::from(sample);
    let b = nsh_core::Rgba::from(back);
    assert!((a.r - b.r).abs() < 0.01 && (a.g - b.g).abs() < 0.01 && (a.b - b.b).abs() < 0.01);
}

#[test]
fn radius_tokens_and_modal_scrim_follow_design_scale() {
    let tokens = theme_tokens(ThemeId::Ocean);
    assert_eq!(tokens.number("radius"), Some(6.0));
    assert_eq!(tokens.number("radius-sm"), Some(4.0));
    assert_eq!(tokens.number("radius-lg"), Some(8.0));
    for id in ThemeId::ALL {
        for dark in [false, true] {
            assert!((color_of(id, "modal-scrim", dark).a - 0.45).abs() < 0.001);
        }
    }
}
