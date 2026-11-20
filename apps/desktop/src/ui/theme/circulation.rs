//! 循径共用表面、Ocean 与高饱和清新主题 Mint 配对；仅主题定义允许具体颜色。
use super::{ThemeId, foreground_on};
use nsh_core::Hsla;

fn rgb(value: u32) -> Hsla {
    nsh_core::color::rgb(value).into()
}

fn apply_overrides(colors: &mut [(&'static str, Hsla)], overrides: Vec<(&'static str, Hsla)>) {
    for (name, value) in overrides {
        if let Some(entry) = colors.iter_mut().find(|(key, _)| *key == name) {
            entry.1 = value;
        }
    }
}

fn ocean_overrides(dark: bool) -> Vec<(&'static str, Hsla)> {
    let palette = if dark {
        [0xa9ccff, 0x132742, 0xbfdaff, 0x8fb8f2, 0x2b3d54, 0xc3dcff]
    } else {
        [0x245db5, 0xffffff, 0x1d509f, 0x194586, 0xe3edfc, 0x204f91]
    };
    let mut overrides = Vec::new();
    for key in ["primary", "sidebar-primary", "ring", "sidebar-ring"] {
        overrides.push((key, rgb(palette[0])));
    }
    for key in [
        "primary-foreground",
        "primary-hover-foreground",
        "primary-active-foreground",
        "sidebar-primary-foreground",
    ] {
        overrides.push((key, rgb(palette[1])));
    }
    overrides.extend([
        ("primary-hover", rgb(palette[2])),
        ("primary-active", rgb(palette[3])),
    ]);
    for key in ["accent", "sidebar-accent"] {
        overrides.push((key, rgb(palette[4])));
    }
    for key in ["accent-foreground", "sidebar-accent-foreground"] {
        overrides.push((key, rgb(palette[5])));
    }
    overrides
}

/// 高饱和活跃清新主题（Mint）：浅色使用高明度清透薄荷底色 + 纯白卡片 + 100% 饱和翠绿青主色；
/// 深色使用清澈深海青底色 + 荧光薄荷主色；全部正文/填充/边界均满足 WCAG AA 与 3:1 非文本对比度。
fn mint_overrides(dark: bool) -> Vec<(&'static str, Hsla)> {
    if dark {
        vec![
            ("background", rgb(0x09191b)),
            ("sidebar", rgb(0x061315)),
            ("card", rgb(0x0f2428)),
            ("popover", rgb(0x142e33)),
            ("foreground", rgb(0xe6fbf6)),
            ("card-foreground", rgb(0xe6fbf6)),
            ("popover-foreground", rgb(0xe6fbf6)),
            ("sidebar-foreground", rgb(0xe6fbf6)),
            ("muted", rgb(0x153237)),
            ("muted-foreground", rgb(0x82bfb5)),
            ("border", rgb(0x1e464d)),
            ("sidebar-border", rgb(0x1e464d)),
            ("input", rgb(0x39a896)),
            ("primary", rgb(0x18f0be)),
            ("primary-foreground", rgb(0x04241c)),
            ("primary-hover", rgb(0x42f5cb)),
            ("primary-hover-foreground", rgb(0x04241c)),
            ("primary-active", rgb(0x0ad8a8)),
            ("primary-active-foreground", rgb(0x04241c)),
            ("sidebar-primary", rgb(0x18f0be)),
            ("sidebar-primary-foreground", rgb(0x04241c)),
            ("ring", rgb(0x18f0be)),
            ("sidebar-ring", rgb(0x18f0be)),
            ("secondary", rgb(0x14383c)),
            ("secondary-foreground", rgb(0xd2faf0)),
            ("secondary-hover", rgb(0x1b464b)),
            ("secondary-hover-foreground", rgb(0xd2faf0)),
            ("secondary-active", rgb(0x22545a)),
            ("secondary-active-foreground", rgb(0xd2faf0)),
            ("accent", rgb(0x11423d)),
            ("accent-foreground", rgb(0x78fce0)),
            ("accent-hover", rgb(0x17524c)),
            ("accent-hover-foreground", rgb(0x78fce0)),
            ("accent-active", rgb(0x1e635c)),
            ("accent-active-foreground", rgb(0x78fce0)),
            ("sidebar-accent", rgb(0x11423d)),
            ("sidebar-accent-foreground", rgb(0x78fce0)),
            ("switch-thumb", rgb(0xf2fffc)),
            ("chart-1", rgb(0x18f0be)),
            ("chart-2", rgb(0x38bdf8)),
            ("chart-3", rgb(0xfbbf24)),
            ("chart-4", rgb(0xfb7185)),
            ("chart-5", rgb(0xa78bfa)),
            ("chart-download", rgb(0x18f0be)),
            ("chart-upload", rgb(0x38bdf8)),
        ]
    } else {
        vec![
            ("background", rgb(0xeefbf7)),
            ("sidebar", rgb(0xdff7ef)),
            ("card", rgb(0xffffff)),
            ("popover", rgb(0xffffff)),
            ("foreground", rgb(0x092924)),
            ("card-foreground", rgb(0x092924)),
            ("popover-foreground", rgb(0x092924)),
            ("sidebar-foreground", rgb(0x092924)),
            ("muted", rgb(0xd6f4ea)),
            ("muted-foreground", rgb(0x236156)),
            ("border", rgb(0xa5e4d3)),
            ("sidebar-border", rgb(0xa5e4d3)),
            ("input", rgb(0x1d8773)),
            ("primary", rgb(0x007c67)),
            ("primary-foreground", rgb(0xffffff)),
            ("primary-hover", rgb(0x006b59)),
            ("primary-hover-foreground", rgb(0xffffff)),
            ("primary-active", rgb(0x00594a)),
            ("primary-active-foreground", rgb(0xffffff)),
            ("sidebar-primary", rgb(0x007c67)),
            ("sidebar-primary-foreground", rgb(0xffffff)),
            ("ring", rgb(0x008a73)),
            ("sidebar-ring", rgb(0x008a73)),
            ("secondary", rgb(0xd4f6ec)),
            ("secondary-foreground", rgb(0x074a3e)),
            ("secondary-hover", rgb(0xbfefdf)),
            ("secondary-hover-foreground", rgb(0x074a3e)),
            ("secondary-active", rgb(0xa8e7d3)),
            ("secondary-active-foreground", rgb(0x074a3e)),
            ("accent", rgb(0xc2f5e5)),
            ("accent-foreground", rgb(0x05473b)),
            ("accent-hover", rgb(0xaaeed9)),
            ("accent-hover-foreground", rgb(0x05473b)),
            ("accent-active", rgb(0x90e6cc)),
            ("accent-active-foreground", rgb(0x05473b)),
            ("sidebar-accent", rgb(0xc2f5e5)),
            ("sidebar-accent-foreground", rgb(0x05473b)),
            ("switch-thumb", rgb(0xffffff)),
            ("chart-1", rgb(0x00897b)),
            ("chart-2", rgb(0x0077ff)),
            ("chart-3", rgb(0xff8800)),
            ("chart-4", rgb(0xe11d68)),
            ("chart-5", rgb(0x6d28d9)),
            ("chart-download", rgb(0x00897b)),
            ("chart-upload", rgb(0x0077ff)),
        ]
    }
}

pub(super) fn refine(colors: &mut Vec<(&'static str, Hsla)>, id: ThemeId, dark: bool) {
    if id == ThemeId::Mint {
        apply_overrides(colors, mint_overrides(dark));
    } else {
        let neutral = if dark {
            [
                0x15191e, 0x111519, 0x1c2229, 0x242c35, 0xe7edf5, 0xa0adbc, 0x35414e, 0x718197,
            ]
        } else {
            [
                0xf4f6f8, 0xebeff3, 0xffffff, 0xffffff, 0x202936, 0x566579, 0xd2dae3, 0x7b899b,
            ]
        };
        let mut overrides: Vec<(&'static str, Hsla)> = [
            "background",
            "sidebar",
            "card",
            "popover",
            "foreground",
            "muted-foreground",
            "border",
            "input",
        ]
        .into_iter()
        .zip(neutral.map(rgb))
        .collect();
        let fg = rgb(neutral[4]);
        for key in [
            "card-foreground",
            "popover-foreground",
            "sidebar-foreground",
        ] {
            overrides.push((key, fg));
        }
        overrides.push(("sidebar-border", rgb(neutral[6])));
        if id == ThemeId::Ocean {
            overrides.extend(ocean_overrides(dark));
        }
        apply_overrides(colors, overrides);
    }
    let status = if dark {
        [
            0x83d5b5, 0x203d34, 0xf0c27d, 0x403222, 0xffaba5, 0x432b2d, 0x4e6480,
        ]
    } else {
        [
            0x17694d, 0xe4f4ec, 0x87500a, 0xfff2dc, 0xac3036, 0xfceaed, 0x8399b6,
        ]
    };
    let path_line = if id == ThemeId::Mint {
        rgb(if dark { 0x2ce0b8 } else { 0x14b89a })
    } else {
        rgb(status[6])
    };
    for (name, value) in [("success", status[0]), ("warning", status[2])] {
        let c = rgb(value);
        if let Some(entry) = colors.iter_mut().find(|(key, _)| *key == name) {
            entry.1 = c;
        }
        let foreground = if name == "success" {
            "success-foreground"
        } else {
            "warning-foreground"
        };
        if let Some(entry) = colors.iter_mut().find(|(key, _)| *key == foreground) {
            entry.1 = foreground_on(c);
        }
    }
    colors.extend([
        ("modal-scrim", rgb(0).alpha(0.45)),
        ("success-surface", rgb(status[1])),
        ("warning-surface", rgb(status[3])),
        ("danger-surface", rgb(status[5])),
        ("path-line", path_line),
    ]);
    let surfaces = ["background", "card", "popover", "sidebar"].map(|name| {
        colors
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, c)| *c)
            .unwrap_or_default()
    });
    for name in ["ring", "sidebar-ring", "input"] {
        if let Some((_, c)) = colors.iter_mut().find(|(key, _)| *key == name) {
            *c = super::meet_contrast(*c, &surfaces, 3.0, dark);
        }
    }
}
