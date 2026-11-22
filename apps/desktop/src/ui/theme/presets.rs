//! 3 套精选预设主题（全 OKLCH 感知均匀派生）：
//!
//! - `ocean`（默认）：清澈海蓝
//! - `teal`：深海青绿（主按钮在浅/深模式均保持高对比度纯白文字）
//! - `lanpath`：基于应用图标品牌色 `#292D63` 与深海暮蓝/雾紫配色（`#232946 / #292D63 / #b8c1ec / #fffffe`）

use super::ThemeId;
use super::assemble::{PaletteSpec, charts_from};
use super::custom::CustomPalette;
use super::oklch::rgb_hex;

/// 每个预设对应的 4 色种子，用于自定义配色器“从预设导入微调”。
pub fn preset_seed(id: ThemeId) -> CustomPalette {
    match id {
        ThemeId::Lanpath | ThemeId::Custom => CustomPalette {
            primary: rgb_hex(0x292d63),
            accent: rgb_hex(0xb8c1ec),
            background: rgb_hex(0xf4f5fb),
            foreground: rgb_hex(0x232946),
        },
        ThemeId::Ocean => CustomPalette {
            primary: rgb_hex(0x245db5),
            accent: rgb_hex(0xe3edfc),
            background: rgb_hex(0xf4f6f8),
            foreground: rgb_hex(0x202936),
        },
        ThemeId::Teal => CustomPalette {
            primary: rgb_hex(0x0f766e),
            accent: rgb_hex(0xd6f0ec),
            background: rgb_hex(0xf3f8f8),
            foreground: rgb_hex(0x162b2b),
        },
    }
}

pub(super) fn preset_spec(id: ThemeId, dark: bool) -> PaletteSpec {
    match id {
        ThemeId::Lanpath | ThemeId::Custom => lanpath_spec(dark),
        ThemeId::Ocean => ocean_spec(dark),
        ThemeId::Teal => teal_spec(dark),
    }
}

fn ocean_spec(dark: bool) -> PaletteSpec {
    if dark {
        PaletteSpec {
            background: rgb_hex(0x15191e),
            sidebar: rgb_hex(0x111519),
            card: rgb_hex(0x1c2229),
            popover: rgb_hex(0x242c35),
            foreground: rgb_hex(0xe7edf5),
            muted: rgb_hex(0x222a33),
            muted_foreground: rgb_hex(0xa0adbc),
            border: rgb_hex(0x35414e),
            input: rgb_hex(0x718197),
            primary: rgb_hex(0xa9ccff),
            primary_foreground: Some(rgb_hex(0x132742)),
            secondary: rgb_hex(0x24303e),
            secondary_foreground: rgb_hex(0xdce8f8),
            accent: rgb_hex(0x2b3d54),
            accent_foreground: rgb_hex(0xc3dcff),
            switch_thumb: rgb_hex(0xf4f8ff),
            charts: charts_from(
                rgb_hex(0x7cb3ff),
                rgb_hex(0x52d1b2),
                rgb_hex(0xf0b86e),
                true,
            ),
            path_line: rgb_hex(0x4e6480),
        }
    } else {
        PaletteSpec {
            background: rgb_hex(0xf4f6f8),
            sidebar: rgb_hex(0xebeff3),
            card: rgb_hex(0xffffff),
            popover: rgb_hex(0xffffff),
            foreground: rgb_hex(0x202936),
            muted: rgb_hex(0xe9eef4),
            muted_foreground: rgb_hex(0x566579),
            border: rgb_hex(0xd2dae3),
            input: rgb_hex(0x7b899b),
            primary: rgb_hex(0x245db5),
            primary_foreground: Some(rgb_hex(0xffffff)),
            secondary: rgb_hex(0xe7eef8),
            secondary_foreground: rgb_hex(0x203a60),
            accent: rgb_hex(0xe3edfc),
            accent_foreground: rgb_hex(0x204f91),
            switch_thumb: rgb_hex(0xffffff),
            charts: charts_from(
                rgb_hex(0x245db5),
                rgb_hex(0x148f77),
                rgb_hex(0xc97b18),
                false,
            ),
            path_line: rgb_hex(0x8399b6),
        }
    }
}

/// Teal：浅/深色主按钮均采用饱和深青绿配纯白文字（contrast >= 5.0:1），告别黑字违和感。
fn teal_spec(dark: bool) -> PaletteSpec {
    if dark {
        PaletteSpec {
            background: rgb_hex(0x10191a),
            sidebar: rgb_hex(0x0c1314),
            card: rgb_hex(0x162324),
            popover: rgb_hex(0x1d2e30),
            foreground: rgb_hex(0xe6f2f2),
            muted: rgb_hex(0x1b2c2d),
            muted_foreground: rgb_hex(0x94b3b3),
            border: rgb_hex(0x2a4244),
            input: rgb_hex(0x5a8e8d),
            primary: rgb_hex(0x0f766e),
            primary_foreground: Some(rgb_hex(0xffffff)),
            secondary: rgb_hex(0x193234),
            secondary_foreground: rgb_hex(0xd2f0ed),
            accent: rgb_hex(0x173b3a),
            accent_foreground: rgb_hex(0x99ebe1),
            switch_thumb: rgb_hex(0xf2fffd),
            charts: charts_from(
                rgb_hex(0x2dd4bf),
                rgb_hex(0x58a6ff),
                rgb_hex(0xf0b86e),
                true,
            ),
            path_line: rgb_hex(0x2f7572),
        }
    } else {
        PaletteSpec {
            background: rgb_hex(0xf3f8f8),
            sidebar: rgb_hex(0xe7f1f0),
            card: rgb_hex(0xffffff),
            popover: rgb_hex(0xffffff),
            foreground: rgb_hex(0x162b2b),
            muted: rgb_hex(0xe5f0ef),
            muted_foreground: rgb_hex(0x456161),
            border: rgb_hex(0xc7dedc),
            input: rgb_hex(0x5a8582),
            primary: rgb_hex(0x0f766e),
            primary_foreground: Some(rgb_hex(0xffffff)),
            secondary: rgb_hex(0xe1f2f0),
            secondary_foreground: rgb_hex(0x124743),
            accent: rgb_hex(0xd6f0ec),
            accent_foreground: rgb_hex(0x0b524c),
            switch_thumb: rgb_hex(0xffffff),
            charts: charts_from(
                rgb_hex(0x0f766e),
                rgb_hex(0x2563eb),
                rgb_hex(0xd97706),
                false,
            ),
            path_line: rgb_hex(0x639994),
        }
    }
}

/// Lanpath 图标品牌主题：融合应用图标靛蓝 `#292D63` 与暮蓝/雾紫配色
/// （`#232946 / #292D63 / #a8b4f4 / #b8c1ec / #fffffe`），整体克制统一。
fn lanpath_spec(dark: bool) -> PaletteSpec {
    if dark {
        PaletteSpec {
            background: rgb_hex(0x232946),
            sidebar: rgb_hex(0x191e36),
            card: rgb_hex(0x2a3154),
            popover: rgb_hex(0x323a63),
            foreground: rgb_hex(0xfffffe),
            muted: rgb_hex(0x2f3659),
            muted_foreground: rgb_hex(0xb8c1ec),
            border: rgb_hex(0x3f4875),
            input: rgb_hex(0x8b95c9),
            primary: rgb_hex(0xa8b4f4),
            primary_foreground: Some(rgb_hex(0x171c38)),
            secondary: rgb_hex(0x313961),
            secondary_foreground: rgb_hex(0xe4e8fc),
            accent: rgb_hex(0x364070),
            accent_foreground: rgb_hex(0xdce2ff),
            switch_thumb: rgb_hex(0xfffffe),
            charts: charts_from(
                rgb_hex(0xa8b4f4),
                rgb_hex(0x76d1c6),
                rgb_hex(0xe6c27a),
                true,
            ),
            path_line: rgb_hex(0x7b85bd),
        }
    } else {
        PaletteSpec {
            background: rgb_hex(0xf4f5fb),
            sidebar: rgb_hex(0xeaedf8),
            card: rgb_hex(0xfffffe),
            popover: rgb_hex(0xfffffe),
            foreground: rgb_hex(0x232946),
            muted: rgb_hex(0xe9ecf7),
            muted_foreground: rgb_hex(0x4a5275),
            border: rgb_hex(0xd0d5ea),
            input: rgb_hex(0x6a73a0),
            primary: rgb_hex(0x292d63),
            primary_foreground: Some(rgb_hex(0xfffffe)),
            secondary: rgb_hex(0xe6e9f8),
            secondary_foreground: rgb_hex(0x232946),
            accent: rgb_hex(0xdfe4fa),
            accent_foreground: rgb_hex(0x292d63),
            switch_thumb: rgb_hex(0xfffffe),
            charts: charts_from(
                rgb_hex(0x292d63),
                rgb_hex(0x5568d4),
                rgb_hex(0x23968a),
                false,
            ),
            path_line: rgb_hex(0x7d87ba),
        }
    }
}
