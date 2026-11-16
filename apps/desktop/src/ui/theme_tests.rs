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
    "secondary",
    "secondary-foreground",
    "muted",
    "muted-foreground",
    "accent",
    "accent-foreground",
    "destructive",
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
];

fn color_of(id: ThemeId, name: &str, dark: bool) -> Hsla {
    let tokens = theme_tokens(id);
    let color = tokens.color(name).unwrap_or_else(|| panic!("{name}"));
    if dark { color.dark } else { color.light }
}

#[test]
fn thirteen_themes_with_complete_pairs() {
    assert_eq!(ThemeId::ALL.len(), 13);
    let mut names: Vec<&str> = ThemeId::ALL.iter().map(|id| id.id_name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 13, "主题 ID 必须唯一");
    for id in ThemeId::ALL {
        for name in REQUIRED {
            assert!(
                theme_tokens(id).color(name).is_some(),
                "{} 缺 {name}",
                id.id_name()
            );
        }
    }
    // 存储值往返：未知值回退默认主题，不 panic。
    assert_eq!(ThemeId::from_id_name("ocean"), ThemeId::Ocean);
    assert_eq!(ThemeId::from_id_name("nope"), ThemeId::Amethyst);
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
fn text_and_primary_contrast_meets_wcag_aa() {
    for id in ThemeId::ALL {
        for dark in [false, true] {
            let bg = color_of(id, "background", dark);
            for text in ["foreground", "muted-foreground", "secondary-foreground"] {
                assert!(
                    contrast(color_of(id, text, dark), bg) >= 4.5,
                    "{} {}（{}）正文对比度不足",
                    id.id_name(),
                    text,
                    if dark { "dark" } else { "light" }
                );
            }
            let primary = color_of(id, "primary", dark);
            let primary_fg = color_of(id, "primary-foreground", dark);
            assert!(
                contrast(primary_fg, primary) >= 4.5,
                "{} primary-foreground 对比度不足",
                id.id_name()
            );
        }
    }
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
