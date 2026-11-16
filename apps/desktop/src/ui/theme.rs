//! 主题注册：13 组命名主题（amethyst 默认），每组完整浅色/深色配对，
//! 集中产出语义 token（`Tokens`）并经 `nsh_core::element::set_tokens`
//! 运行时切换——`theme()` 读取处随新 token 重建，无需重建组件树。
//! 语义状态色（错误/警告/成功）各主题固定，不被主题色取代；
//! 圆角为 number token（`radius` 6pt、`radius-sm` 4pt、`radius-lg` 8pt）。
//! 浅色背景保持高亮度微色相分层，避免低饱和脏灰；正文/主色对比度
//! 由构建器自动校验（≥4.5:1），单测逐项断言。

use std::sync::LazyLock;

use nsh_core::element::{AdaptiveColor, Tokens};
use nsh_core::{Hsla, element::set_tokens};

/// 命名主题 ID（DESIGN.md 4.3；ID 本身不翻译，界面直接显示 ID）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThemeId {
    Amethyst,
    Indigo,
    Ocean,
    Forest,
    Sand,
    Rose,
    Graphite,
    Cyan,
    Orange,
    Teal,
    Crimson,
    Gold,
    Slate,
}

impl ThemeId {
    /// 全部主题，网格显示顺序。
    pub const ALL: [ThemeId; 13] = [
        ThemeId::Amethyst,
        ThemeId::Indigo,
        ThemeId::Ocean,
        ThemeId::Forest,
        ThemeId::Sand,
        ThemeId::Rose,
        ThemeId::Graphite,
        ThemeId::Cyan,
        ThemeId::Orange,
        ThemeId::Teal,
        ThemeId::Crimson,
        ThemeId::Gold,
        ThemeId::Slate,
    ];

    /// 稳定存储/配置用 ID（小写连字符；不随语言变化）。
    pub fn id_name(self) -> &'static str {
        match self {
            ThemeId::Amethyst => "amethyst",
            ThemeId::Indigo => "indigo",
            ThemeId::Ocean => "ocean",
            ThemeId::Forest => "forest",
            ThemeId::Sand => "sand",
            ThemeId::Rose => "rose",
            ThemeId::Graphite => "graphite",
            ThemeId::Cyan => "cyan",
            ThemeId::Orange => "orange",
            ThemeId::Teal => "teal",
            ThemeId::Crimson => "crimson",
            ThemeId::Gold => "gold",
            ThemeId::Slate => "slate",
        }
    }

    /// 存储值对应的主题；未知/缺省回退默认 amethyst。
    pub fn from_id_name(name: &str) -> Self {
        ThemeId::ALL
            .into_iter()
            .find(|id| id.id_name() == name)
            .unwrap_or(ThemeId::Amethyst)
    }
}

/// 一组主题的色相参数：由色相派生完整配对，保证跨主题一致的亮度
/// 与对比度纪律。
struct Params {
    /// 基础色相（度）。
    hue: f32,
    /// 基础饱和度（0..1）。
    saturation: f32,
}

const fn params(id: ThemeId) -> Params {
    match id {
        ThemeId::Amethyst => Params {
            hue: 258.0,
            saturation: 0.46,
        },
        ThemeId::Indigo => Params {
            hue: 232.0,
            saturation: 0.56,
        },
        ThemeId::Ocean => Params {
            hue: 205.0,
            saturation: 0.62,
        },
        ThemeId::Forest => Params {
            hue: 150.0,
            saturation: 0.42,
        },
        ThemeId::Sand => Params {
            hue: 40.0,
            saturation: 0.38,
        },
        ThemeId::Rose => Params {
            hue: 345.0,
            saturation: 0.56,
        },
        ThemeId::Graphite => Params {
            hue: 250.0,
            saturation: 0.05,
        },
        ThemeId::Cyan => Params {
            hue: 190.0,
            saturation: 0.62,
        },
        ThemeId::Orange => Params {
            hue: 28.0,
            saturation: 0.78,
        },
        ThemeId::Teal => Params {
            hue: 175.0,
            saturation: 0.52,
        },
        ThemeId::Crimson => Params {
            hue: 355.0,
            saturation: 0.70,
        },
        ThemeId::Gold => Params {
            hue: 46.0,
            saturation: 0.72,
        },
        ThemeId::Slate => Params {
            hue: 215.0,
            saturation: 0.22,
        },
    }
}

/// HSL 快捷构造（输入 h 度；NSH 内部 h 为 0..1，s/l 为 0..1）。
fn hsl(h: f32, s: f32, l: f32) -> Hsla {
    Hsla {
        h: h.rem_euclid(360.0) / 360.0,
        s,
        l,
        a: 1.0,
    }
}

/// HSL → 线性 RGB（对比度计算用）。
fn linear_rgb(c: Hsla) -> (f32, f32, f32) {
    let h = c.h.rem_euclid(1.0) * 6.0;
    let (s, l) = (c.s.clamp(0.0, 1.0), c.l.clamp(0.0, 1.0));
    let chroma = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = chroma * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u8 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = l - chroma / 2.0;
    let channel = |v: f32| {
        let v = v + m;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055_f32).powf(2.4)
        }
    };
    (channel(r), channel(g), channel(b))
}

/// WCAG 相对亮度。
fn luminance(c: Hsla) -> f32 {
    let (r, g, b) = linear_rgb(c);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// WCAG 对比度（1..21）。
fn contrast(a: Hsla, b: Hsla) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// `bg` 上对比度达标的前景：在深/浅两个候选中取更优者；都不达标时
/// 向对应极值逐步压亮度直到 ≥4.5（极小步数内收敛）。
fn foreground_on(bg: Hsla) -> Hsla {
    let mut light = hsl(0.0, 0.0, 0.99);
    let mut dark = hsl(0.0, 0.0, 0.10);
    for _ in 0..24 {
        let cl = contrast(light, bg);
        let cd = contrast(dark, bg);
        if cl >= 4.5 || cd >= 4.5 {
            return if cl >= cd { light } else { dark };
        }
        light.l -= 0.04;
        dark.l += 0.04;
    }
    if contrast(light, bg) >= contrast(dark, bg) {
        light
    } else {
        dark
    }
}

/// 语义状态色（错误/警告/成功）：各主题固定，不随主题色相变化。
struct Status {
    destructive: Hsla,
    warning: Hsla,
    success: Hsla,
}

fn status(dark: bool) -> Status {
    if dark {
        Status {
            destructive: hsl(0.0, 0.62, 0.68),
            warning: hsl(38.0, 0.86, 0.68),
            success: hsl(145.0, 0.52, 0.62),
        }
    } else {
        Status {
            destructive: hsl(0.0, 0.68, 0.44),
            warning: hsl(38.0, 0.88, 0.40),
            success: hsl(145.0, 0.58, 0.34),
        }
    }
}

/// 朝 `toward_darker` 方向逐步调 `color` 的亮度，直到与 `bg` 的对比度
/// ≥4.5（正文级）或步数耗尽——保证每组主题、两个外观的辅助文字可读。
fn meet_aa(mut color: Hsla, bg: Hsla, darker: bool) -> Hsla {
    for _ in 0..24 {
        if contrast(color, bg) >= 4.5 {
            break;
        }
        color.l += if darker { 0.02 } else { -0.02 };
    }
    color
}

/// 一组语义色（单外观）：名字 → 颜色。派生规则集中在此，组件层只按
/// 名字读取，不出现颜色字面量。
fn palette(id: ThemeId, dark: bool) -> Vec<(&'static str, Hsla)> {
    let Params {
        hue: h,
        saturation: s,
    } = params(id);
    let status = status(dark);
    let (
        bg,
        card,
        popover,
        fg,
        muted_fg,
        border,
        input,
        primary,
        secondary,
        secondary_fg,
        muted,
        accent,
        accent_fg,
    ) = if dark {
        let bg = hsl(h, s * 0.16, 0.135);
        let primary = hsl(h, s * 0.85, 0.70);
        (
            bg,
            hsl(h, s * 0.20, 0.175),
            hsl(h, s * 0.20, 0.19),
            hsl(h, s * 0.18, 0.93),
            hsl(h, s * 0.10, 0.64),
            hsl(h, s * 0.15, 0.26),
            hsl(h, s * 0.15, 0.30),
            primary,
            hsl(h, s * 0.22, 0.22),
            hsl(h, s * 0.20, 0.90),
            hsl(h, s * 0.18, 0.20),
            hsl(h, s * 0.25, 0.24),
            hsl(h, s * 0.30, 0.92),
        )
    } else {
        let bg = hsl(h, s * 0.22, 0.965);
        let primary = hsl(h, s, 0.46);
        (
            bg,
            hsl(h, s * 0.28, 0.995),
            hsl(h, s * 0.28, 0.995),
            hsl(h, s * 0.30, 0.17),
            hsl(h, s * 0.16, 0.44),
            hsl(h, s * 0.20, 0.865),
            hsl(h, s * 0.20, 0.83),
            primary,
            hsl(h, s * 0.30, 0.93),
            hsl(h, s * 0.35, 0.25),
            hsl(h, s * 0.25, 0.94),
            hsl(h, s * 0.35, 0.905),
            hsl(h, s * 0.45, 0.28),
        )
    };
    let primary_fg = foreground_on(primary);
    let muted_fg = meet_aa(muted_fg, bg, dark);
    let switch_thumb = if dark {
        hsl(h, s * 0.20, 0.96)
    } else {
        hsl(h, s * 0.25, 0.99)
    };
    let sidebar = if dark {
        hsl(h, s * 0.14, 0.115)
    } else {
        hsl(h, s * 0.22, 0.955)
    };
    let mut colors = vec![
        ("background", bg),
        ("foreground", fg),
        ("card", card),
        ("card-foreground", fg),
        ("popover", popover),
        ("popover-foreground", fg),
        ("primary", primary),
        ("primary-foreground", primary_fg),
        ("secondary", secondary),
        ("secondary-foreground", secondary_fg),
        ("muted", muted),
        ("muted-foreground", muted_fg),
        ("accent", accent),
        ("accent-foreground", accent_fg),
        ("destructive", status.destructive),
        ("destructive-foreground", foreground_on(status.destructive)),
        ("border", border),
        ("input", input),
        ("ring", primary),
        ("sidebar", sidebar),
        ("sidebar-foreground", fg),
        ("sidebar-primary", primary),
        ("sidebar-primary-foreground", primary_fg),
        ("sidebar-accent", accent),
        ("sidebar-accent-foreground", accent_fg),
        ("sidebar-border", border),
        ("sidebar-ring", primary),
        ("warning", status.warning),
        ("warning-foreground", foreground_on(status.warning)),
        ("success", status.success),
        ("success-foreground", foreground_on(status.success)),
        // 开关滑块的中性表面色（DESIGN：组件不得出现颜色字面量，
        // 颜色只能在主题定义中）。
        ("switch-thumb", switch_thumb),
    ];
    // 图表序列：主色相的固定轮转，两个外观各自保持可读亮度。
    let (chart_l, chart_s) = if dark { (0.68, s * 0.80) } else { (0.46, s) };
    let series: Vec<Hsla> = [0.0_f32, 32.0, 64.0, -32.0, 152.0]
        .iter()
        .map(|delta| hsl(h + delta, chart_s, chart_l))
        .collect();
    for (index, name) in ["chart-1", "chart-2", "chart-3", "chart-4", "chart-5"]
        .iter()
        .enumerate()
    {
        colors.push((name, series[index]));
    }
    // 流量图例的上传/下载语义别名（DESIGN 3.3）：取图表序列前两个
    // 对比色，全部主题统一注册。
    colors.push(("chart-upload", series[1]));
    colors.push(("chart-download", series[0]));
    colors
}

/// 主题的完整 token：shadcn 默认 token 之上覆盖本组配对，并写入
/// 圆角层级（radius 6pt / radius-sm 4pt / radius-lg 8pt）。
fn build_tokens(id: ThemeId) -> Tokens {
    let mut tokens = nsh_base::Theme::default().into_tokens();
    let light: std::collections::HashMap<&str, Hsla> = palette(id, false).into_iter().collect();
    for (name, dark_color) in palette(id, true) {
        let light_color = light
            .get(name)
            .unwrap_or_else(|| panic!("浅色配对缺 {name}"));
        tokens.set_color(
            name,
            AdaptiveColor {
                light: *light_color,
                dark: dark_color,
            },
        );
    }
    tokens.set_number("radius", 6.0);
    tokens.set_number("radius-sm", 4.0);
    tokens.set_number("radius-lg", 8.0);
    tokens
}

static TOKENS: LazyLock<Vec<Tokens>> =
    LazyLock::new(|| ThemeId::ALL.iter().map(|id| build_tokens(*id)).collect());

/// 主题的 token（构建期缓存；每次克隆一份新的 `Tokens`，`set_tokens`
/// 以 `Arc` 身份传播，读取方按指针变化重建）。
pub fn theme_tokens(id: ThemeId) -> Tokens {
    TOKENS[id as usize].clone()
}

/// 运行时切换主题：所有 `theme()`/`tokens()` 读取方收到新 token，
/// 组件树不重挂、业务运行状态不重建。
pub fn apply(id: ThemeId) {
    set_tokens(theme_tokens(id));
}

#[cfg(test)]
#[path = "theme_tests.rs"]
mod tests;
