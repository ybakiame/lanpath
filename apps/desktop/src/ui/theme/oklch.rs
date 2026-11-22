//! OKLCH 感知均匀色彩空间与 WCAG 对比度辅助。
//!
//! 相比 HSL，OKLCH 的亮度通道 `l`（0..1）与人眼感知一致，跨色相（蓝/青/绿/暖棕/黄）
//! 派生浅深背景、按钮 hover/active 阶梯和聚焦环时不会出现“同 L 值明暗悬殊”或色相偏移。

use nsh_core::Hsla;

/// OKLCH 色彩（`l` 0..=1，`c` >= 0，`h` 0..360 度）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklch {
    pub l: f32,
    pub c: f32,
    pub h: f32,
}

fn srgb_to_linear(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.0031308 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

fn oklch_to_linear_srgb(l: f32, c: f32, h_deg: f32) -> [f32; 3] {
    let l = l as f64;
    let c = c.max(0.0) as f64;
    let h = (h_deg as f64).to_radians();
    let (a, b) = (c * h.cos(), c * h.sin());

    let l_ = (l + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m_ = (l - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s_ = (l - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);

    [
        (4.076_741_662_1 * l_ - 3.307_711_591_3 * m_ + 0.230_969_929_2 * s_) as f32,
        (-1.268_438_004_6 * l_ + 2.609_757_401_1 * m_ - 0.341_319_396_5 * s_) as f32,
        (-0.004_196_086_3 * l_ - 0.703_418_614_7 * m_ + 1.707_614_701 * s_) as f32,
    ]
}

fn in_gamut(rgb: [f32; 3]) -> bool {
    rgb.iter().all(|&v| (-1e-5..=1.0 + 1e-5).contains(&v))
}

fn srgb_to_hsla(r: f32, g: f32, b: f32) -> Hsla {
    nsh_core::Rgba {
        r: r.clamp(0.0, 1.0),
        g: g.clamp(0.0, 1.0),
        b: b.clamp(0.0, 1.0),
        a: 1.0,
    }
    .into()
}

/// 从 OKLCH 构造 sRGB 色域内 `Hsla`：超色域时固定 `l`/`h` 二分压低彩度 `c`，避免色相偏移。
pub fn oklch(l: f32, c: f32, h_deg: f32) -> Hsla {
    let l = l.clamp(0.0, 1.0);
    let c = c.max(0.0);
    let h = h_deg.rem_euclid(360.0);
    let mut linear = oklch_to_linear_srgb(l, c, h);
    if !in_gamut(linear) {
        let (mut lo, mut hi) = (0.0_f32, c);
        for _ in 0..18 {
            let mid = 0.5 * (lo + hi);
            if in_gamut(oklch_to_linear_srgb(l, mid, h)) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        linear = oklch_to_linear_srgb(l, lo, h);
    }
    srgb_to_hsla(
        linear_to_srgb(linear[0]),
        linear_to_srgb(linear[1]),
        linear_to_srgb(linear[2]),
    )
}

/// `Hsla` 转 OKLCH（供自定义色板派生和色相提取使用）。
pub fn hsla_to_oklch(color: Hsla) -> Oklch {
    let rgba = nsh_core::Rgba::from(color);
    let r = srgb_to_linear(rgba.r) as f64;
    let g = srgb_to_linear(rgba.g) as f64;
    let b = srgb_to_linear(rgba.b) as f64;

    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();

    let lightness = (0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s) as f32;
    let a = 1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s;
    let b_ = 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s;
    let chroma = a.hypot(b_) as f32;
    let hue = if chroma < 1e-4 {
        0.0
    } else {
        b_.atan2(a).to_degrees().rem_euclid(360.0) as f32
    };
    Oklch {
        l: lightness.clamp(0.0, 1.0),
        c: chroma.max(0.0),
        h: hue,
    }
}

/// 24-bit hex (`0xRRGGBB`) 转 `Hsla`。
pub fn rgb_hex(value: u32) -> Hsla {
    nsh_core::color::rgb(value).into()
}

/// `Hsla` 格式化为 `#rrggbb`（小写）。
pub fn hsla_to_hex(color: Hsla) -> String {
    let rgba = nsh_core::Rgba::from(color);
    let r = (rgba.r.clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (rgba.g.clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (rgba.b.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// 解析 `#RRGGBB` 或 `RRGGBB` 字符串为 `Hsla`。
pub fn parse_hex(raw: &str) -> Option<Hsla> {
    let trimmed = raw.trim().trim_start_matches('#');
    if trimmed.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(trimmed, 16).ok()?;
    Some(rgb_hex(value))
}

/// WCAG 2.1 相对亮度（0..=1）。
pub fn luminance(color: Hsla) -> f32 {
    let rgba = nsh_core::Rgba::from(color);
    0.2126 * srgb_to_linear(rgba.r)
        + 0.7152 * srgb_to_linear(rgba.g)
        + 0.0722 * srgb_to_linear(rgba.b)
}

/// WCAG 2.1 对比度（1..=21）。
pub fn contrast(a: Hsla, b: Hsla) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// 选择与 `bg` 对比度达标（≥4.5:1）的柔和白/深墨前景色。
pub fn foreground_on(bg: Hsla) -> Hsla {
    let white = oklch(0.995, 0.0, 0.0);
    let dark = oklch(0.18, 0.01, hsla_to_oklch(bg).h);
    let white_cr = contrast(white, bg);
    let dark_cr = contrast(dark, bg);
    if white_cr >= 4.5 && white_cr >= dark_cr {
        return white;
    }
    if dark_cr >= 4.5 && dark_cr >= white_cr {
        return dark;
    }
    let pure_white = rgb_hex(0xffffff);
    let pure_black = rgb_hex(0x000000);
    if contrast(pure_white, bg) >= contrast(pure_black, bg) {
        pure_white
    } else {
        pure_black
    }
}

/// 沿 OKLCH 亮度轴逐步微调，保持色相稳定直到相对全部背景达到最小对比度。
pub fn meet_contrast(color: Hsla, backgrounds: &[Hsla], minimum: f32, lighter: bool) -> Hsla {
    if backgrounds.iter().all(|bg| contrast(color, *bg) >= minimum) {
        return color;
    }
    let mut c = hsla_to_oklch(color);
    let step = if lighter { 0.015 } else { -0.015 };
    for _ in 0..70 {
        c.l = (c.l + step).clamp(0.0, 1.0);
        let candidate = oklch(c.l, c.c, c.h);
        if backgrounds
            .iter()
            .all(|bg| contrast(candidate, *bg) >= minimum)
        {
            return candidate;
        }
    }
    if lighter {
        rgb_hex(0xffffff)
    } else {
        rgb_hex(0x000000)
    }
}

/// 在 OKLCH 空间内生成按钮 hover/active 状态色：沿同一方向单调拉开亮度，
/// 保持 `active` 的亮度变化严格大于 `hover`，且优先沿用 normal 前景。
pub fn action_states(base: Hsla, preferred_fg: Hsla, dark: bool) -> ((Hsla, Hsla), (Hsla, Hsla)) {
    let base_ok = hsla_to_oklch(base);
    // 若颜色本身接近明暗极值，反向拉开确保 hover < active 单调可区分。
    let lighter = if dark {
        base_ok.l < 0.86
    } else {
        base_ok.l < 0.26
    };
    let dir = if lighter { 1.0_f32 } else { -1.0_f32 };
    let make = |delta: f32| {
        let l = (base_ok.l + dir * delta).clamp(0.05, 0.96);
        let bg = oklch(l, base_ok.c, base_ok.h);
        let fg = if contrast(preferred_fg, bg) >= 4.5 {
            preferred_fg
        } else {
            foreground_on(bg)
        };
        (bg, fg)
    };
    (make(0.045), make(0.09))
}
