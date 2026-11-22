//! 语义色板装配器与跨主题固定状态色（OKLCH 对比度自检）。

use nsh_core::Hsla;

use super::oklch::{
    action_states, contrast, foreground_on, hsla_to_oklch, meet_contrast, oklch, rgb_hex,
};

/// 一套完整语义色板所需的核心表面与品牌输入（由 `assemble_palette` 补全状态与校验对比度）。
pub(super) struct PaletteSpec {
    pub background: Hsla,
    pub sidebar: Hsla,
    pub card: Hsla,
    pub popover: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub muted_foreground: Hsla,
    pub border: Hsla,
    pub input: Hsla,
    pub primary: Hsla,
    pub primary_foreground: Option<Hsla>,
    pub secondary: Hsla,
    pub secondary_foreground: Hsla,
    pub accent: Hsla,
    pub accent_foreground: Hsla,
    pub switch_thumb: Hsla,
    pub charts: [Hsla; 5],
    pub path_line: Hsla,
}

/// 跨主题固定的语义状态色（成功/警告/危险，基于 OKLCH 统一明度）。
struct StatusColors {
    destructive: Hsla,
    warning: Hsla,
    success: Hsla,
    success_surface: Hsla,
    warning_surface: Hsla,
    danger_surface: Hsla,
}

fn status_colors(dark: bool) -> StatusColors {
    if dark {
        StatusColors {
            destructive: oklch(0.74, 0.16, 22.0),
            warning: oklch(0.82, 0.12, 78.0),
            success: oklch(0.81, 0.11, 160.0),
            success_surface: oklch(0.32, 0.05, 160.0),
            warning_surface: oklch(0.33, 0.05, 75.0),
            danger_surface: oklch(0.31, 0.06, 20.0),
        }
    } else {
        StatusColors {
            destructive: oklch(0.51, 0.18, 22.0),
            warning: oklch(0.49, 0.13, 68.0),
            success: oklch(0.46, 0.11, 160.0),
            success_surface: oklch(0.95, 0.03, 160.0),
            warning_surface: oklch(0.96, 0.04, 82.0),
            danger_surface: oklch(0.95, 0.03, 20.0),
        }
    }
}

pub(super) fn charts_from(primary: Hsla, second: Hsla, third: Hsla, dark: bool) -> [Hsla; 5] {
    let p = hsla_to_oklch(primary);
    let l = if dark { 0.74 } else { 0.56 };
    let c = if dark { 0.13 } else { 0.14 };
    [
        primary,
        second,
        third,
        oklch(l, c, p.h + 125.0),
        oklch(l, c, p.h + 230.0),
    ]
}

/// 将 `PaletteSpec` 装配为全部 60 个标准语义 token，并执行对比度自检修正。
pub(super) fn assemble_palette(spec: PaletteSpec, dark: bool) -> Vec<(&'static str, Hsla)> {
    let surfaces = [spec.background, spec.card, spec.popover, spec.sidebar];
    let fg = meet_contrast(spec.foreground, &surfaces, 4.5, dark);
    let muted_fg = meet_contrast(spec.muted_foreground, &surfaces, 4.5, dark);
    let input = meet_contrast(spec.input, &surfaces, 3.0, dark);
    let ring = meet_contrast(spec.primary, &surfaces, 3.0, dark);

    let primary_fg = spec
        .primary_foreground
        .filter(|&c| contrast(c, spec.primary) >= 4.5)
        .unwrap_or_else(|| foreground_on(spec.primary));
    let secondary_fg = if contrast(spec.secondary_foreground, spec.secondary) >= 4.5
        && contrast(spec.secondary_foreground, spec.background) >= 4.5
    {
        spec.secondary_foreground
    } else {
        meet_contrast(
            foreground_on(spec.secondary),
            &[spec.secondary, spec.background],
            4.5,
            dark,
        )
    };
    let accent_fg = if contrast(spec.accent_foreground, spec.accent) >= 4.5 {
        spec.accent_foreground
    } else {
        foreground_on(spec.accent)
    };

    let status = status_colors(dark);
    let destructive_fg = foreground_on(status.destructive);

    let ((primary_hover, primary_hover_fg), (primary_active, primary_active_fg)) =
        action_states(spec.primary, primary_fg, dark);
    let ((secondary_hover, secondary_hover_fg), (secondary_active, secondary_active_fg)) =
        action_states(spec.secondary, secondary_fg, dark);
    let ((accent_hover, accent_hover_fg), (accent_active, accent_active_fg)) =
        action_states(spec.accent, accent_fg, dark);
    let ((destructive_hover, destructive_hover_fg), (destructive_active, destructive_active_fg)) =
        action_states(status.destructive, destructive_fg, dark);

    vec![
        ("background", spec.background),
        ("foreground", fg),
        ("card", spec.card),
        ("card-foreground", fg),
        ("popover", spec.popover),
        ("popover-foreground", fg),
        ("primary", spec.primary),
        ("primary-foreground", primary_fg),
        ("primary-hover", primary_hover),
        ("primary-hover-foreground", primary_hover_fg),
        ("primary-active", primary_active),
        ("primary-active-foreground", primary_active_fg),
        ("secondary", spec.secondary),
        ("secondary-foreground", secondary_fg),
        ("secondary-hover", secondary_hover),
        ("secondary-hover-foreground", secondary_hover_fg),
        ("secondary-active", secondary_active),
        ("secondary-active-foreground", secondary_active_fg),
        ("muted", spec.muted),
        ("muted-foreground", muted_fg),
        ("accent", spec.accent),
        ("accent-foreground", accent_fg),
        ("accent-hover", accent_hover),
        ("accent-hover-foreground", accent_hover_fg),
        ("accent-active", accent_active),
        ("accent-active-foreground", accent_active_fg),
        ("destructive", status.destructive),
        ("destructive-foreground", destructive_fg),
        ("destructive-hover", destructive_hover),
        ("destructive-hover-foreground", destructive_hover_fg),
        ("destructive-active", destructive_active),
        ("destructive-active-foreground", destructive_active_fg),
        ("border", spec.border),
        ("input", input),
        ("ring", ring),
        ("sidebar", spec.sidebar),
        ("sidebar-foreground", fg),
        ("sidebar-primary", spec.primary),
        ("sidebar-primary-foreground", primary_fg),
        ("sidebar-accent", spec.accent),
        ("sidebar-accent-foreground", accent_fg),
        ("sidebar-border", spec.border),
        ("sidebar-ring", ring),
        ("warning", status.warning),
        ("warning-foreground", foreground_on(status.warning)),
        ("success", status.success),
        ("success-foreground", foreground_on(status.success)),
        ("switch-thumb", spec.switch_thumb),
        ("chart-1", spec.charts[0]),
        ("chart-2", spec.charts[1]),
        ("chart-3", spec.charts[2]),
        ("chart-4", spec.charts[3]),
        ("chart-5", spec.charts[4]),
        ("chart-upload", spec.charts[1]),
        ("chart-download", spec.charts[0]),
        ("modal-scrim", rgb_hex(0x000000).alpha(0.45)),
        ("success-surface", status.success_surface),
        ("warning-surface", status.warning_surface),
        ("danger-surface", status.danger_surface),
        ("path-line", spec.path_line),
    ]
}
