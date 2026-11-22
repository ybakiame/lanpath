//! 用户自定义 4 色种子（主色 / 强调色 / 背景 / 文字）在 OKLCH 空间内派生全部 60 个语义 token。
//!
//! 无论用户输入任何极端颜色，派生器都保持：
//! - 浅色背景明亮清爽、卡片亮于或等于背景；
//! - 深色模式继承同色相的克制深色表面；
//! - 正文/次文/按钮/强调态均 ≥4.5:1，聚焦环与输入边界 ≥3:1。

use nsh_core::Hsla;

use super::ThemeId;
use super::assemble::{PaletteSpec, assemble_palette};
use super::oklch::{
    foreground_on, hsla_to_hex, hsla_to_oklch, meet_contrast, oklch, parse_hex, rgb_hex,
};
use super::presets::preset_seed;

/// 4 个核心可调种子色（对应外观页自定义配色组件）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CustomPalette {
    pub primary: Hsla,
    pub accent: Hsla,
    pub background: Hsla,
    pub foreground: Hsla,
}

impl Default for CustomPalette {
    fn default() -> Self {
        preset_seed(ThemeId::Lanpath)
    }
}

impl CustomPalette {
    /// 序列化为紧凑可读的 4 色十六进制串（写入 `app.toml` `[ui].custom_theme`）。
    pub fn serialize(self) -> String {
        format!(
            "{},{},{},{}",
            hsla_to_hex(self.primary),
            hsla_to_hex(self.accent),
            hsla_to_hex(self.background),
            hsla_to_hex(self.foreground),
        )
    }

    /// 解析存储串；空或非法字段自动回退 Ocean 种子，永不 panic。
    pub fn deserialize(raw: &str) -> Self {
        let fallback = Self::default();
        let parts: Vec<&str> = raw.split(',').map(str::trim).collect();
        if parts.len() != 4 {
            return fallback;
        }
        Self {
            primary: parse_hex(parts[0]).unwrap_or(fallback.primary),
            accent: parse_hex(parts[1]).unwrap_or(fallback.accent),
            background: parse_hex(parts[2]).unwrap_or(fallback.background),
            foreground: parse_hex(parts[3]).unwrap_or(fallback.foreground),
        }
    }
}

/// 从 4 色种子在 OKLCH 空间派生完整的浅/深色 60 token 调色板。
pub(super) fn derive_custom_palette(seed: CustomPalette, dark: bool) -> Vec<(&'static str, Hsla)> {
    let p = hsla_to_oklch(seed.primary);
    let a = hsla_to_oklch(seed.accent);
    let bg_ok = hsla_to_oklch(seed.background);
    let fg_ok = hsla_to_oklch(seed.foreground);

    // 若用户背景几乎无彩度，回退主色相给表面保留轻微呼吸感。
    let surface_h = if bg_ok.c > 0.008 { bg_ok.h } else { p.h };
    let surface_c = if bg_ok.c > 0.008 {
        bg_ok.c.clamp(0.006, 0.024)
    } else {
        0.008
    };

    let spec = if dark {
        let background = oklch(0.20, surface_c * 0.85, surface_h);
        let sidebar = oklch(0.165, surface_c * 0.80, surface_h);
        let card = oklch(0.245, surface_c * 0.95, surface_h);
        let popover = oklch(0.285, surface_c * 1.0, surface_h);
        let muted = oklch(0.265, surface_c * 0.9, surface_h);
        let border = oklch(0.36, surface_c * 1.1, surface_h);
        let input = oklch(0.58, (surface_c * 1.4).max(0.02), surface_h);

        let fg_h = if fg_ok.c > 0.01 { fg_ok.h } else { surface_h };
        let foreground = oklch(0.94, fg_ok.c.min(0.02), fg_h);
        let muted_foreground = oklch(0.75, fg_ok.c.min(0.025), fg_h);

        let primary = if p.l < 0.56 {
            oklch(0.74, p.c.clamp(0.04, 0.16), p.h)
        } else {
            oklch(p.l.clamp(0.56, 0.84), p.c.clamp(0.04, 0.18), p.h)
        };
        let secondary = oklch(0.29, (p.c * 0.25).clamp(0.015, 0.045), p.h);
        let secondary_foreground = oklch(0.92, (p.c * 0.15).min(0.03), p.h);

        let accent = oklch(0.31, (a.c * 0.40).clamp(0.02, 0.065), a.h);
        let accent_foreground = oklch(0.90, (a.c * 0.45).clamp(0.02, 0.09), a.h);

        let chart_1 = oklch(0.76, p.c.clamp(0.08, 0.17), p.h);
        let chart_2 = oklch(0.74, a.c.clamp(0.08, 0.16), a.h);
        let chart_3 = oklch(0.78, 0.12, p.h + 85.0);
        let chart_4 = oklch(0.74, 0.13, p.h + 165.0);
        let chart_5 = oklch(0.74, 0.13, p.h + 245.0);

        PaletteSpec {
            background,
            sidebar,
            card,
            popover,
            foreground,
            muted,
            muted_foreground,
            border,
            input,
            primary,
            primary_foreground: None,
            secondary,
            secondary_foreground,
            accent,
            accent_foreground,
            switch_thumb: oklch(0.97, 0.008, surface_h),
            charts: [chart_1, chart_2, chart_3, chart_4, chart_5],
            path_line: oklch(0.56, (p.c * 0.5).clamp(0.04, 0.10), p.h),
        }
    } else {
        // 浅色背景确保明度清爽（OKLCH L >= 0.97 对应 HSL l >= 0.95），保留用户的色相与暖/冷调。
        let background = oklch(bg_ok.l.clamp(0.97, 0.985), surface_c, surface_h);
        let sidebar = oklch(0.945, (surface_c * 1.15).min(0.028), surface_h);
        let card = rgb_hex(0xffffff);
        let popover = rgb_hex(0xffffff);
        let muted = oklch(0.94, (surface_c * 1.1).min(0.025), surface_h);
        let border = oklch(0.875, (surface_c * 1.2).min(0.032), surface_h);
        let input = oklch(0.60, (surface_c * 1.5).max(0.025), surface_h);

        let surfaces = [background, card, popover, sidebar];
        let foreground = if fg_ok.l > 0.55 {
            // 用户若把文字色拖到浅色，按同色相折算为可读深墨色。
            oklch(0.25, fg_ok.c.min(0.06), fg_ok.h)
        } else {
            seed.foreground
        };
        let foreground = meet_contrast(foreground, &surfaces, 4.5, false);
        let muted_foreground = meet_contrast(
            oklch(0.47, fg_ok.c.min(0.045), fg_ok.h),
            &surfaces,
            4.5,
            false,
        );

        let primary = oklch(p.l.clamp(0.24, 0.85), p.c.min(0.22), p.h);
        let secondary = oklch(0.945, (p.c * 0.16).clamp(0.012, 0.032), p.h);
        let secondary_foreground = meet_contrast(
            oklch(0.32, (p.c * 0.45).min(0.09), p.h),
            &[secondary, background],
            4.5,
            false,
        );

        let accent = if a.l < 0.86 {
            oklch(0.93, (a.c * 0.35).clamp(0.018, 0.05), a.h)
        } else {
            oklch(a.l.clamp(0.88, 0.95), a.c.clamp(0.018, 0.055), a.h)
        };
        let accent_foreground = meet_contrast(
            oklch(0.34, (a.c * 0.70).clamp(0.04, 0.14), a.h),
            &[accent],
            4.5,
            false,
        );

        let chart_1 = oklch(p.l.clamp(0.45, 0.62), p.c.clamp(0.08, 0.18), p.h);
        let chart_2 = oklch(a.l.clamp(0.48, 0.65), a.c.clamp(0.08, 0.17), a.h);
        let chart_3 = oklch(0.58, 0.14, p.h + 85.0);
        let chart_4 = oklch(0.54, 0.14, p.h + 165.0);
        let chart_5 = oklch(0.52, 0.14, p.h + 245.0);

        PaletteSpec {
            background,
            sidebar,
            card,
            popover,
            foreground,
            muted,
            muted_foreground,
            border,
            input,
            primary,
            primary_foreground: Some(foreground_on(primary)),
            secondary,
            secondary_foreground,
            accent,
            accent_foreground,
            switch_thumb: rgb_hex(0xffffff),
            charts: [chart_1, chart_2, chart_3, chart_4, chart_5],
            path_line: oklch(0.64, (p.c * 0.55).clamp(0.04, 0.11), p.h),
        }
    };
    assemble_palette(spec, dark)
}
