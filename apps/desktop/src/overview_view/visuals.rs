//! 概览页视觉增强组件：原生 WGSL `Shader` 动态氛围背景与原生 `Canvas` 代理/直连比例环形图。
//!
//! - `Shader` 严格受 `shaders_available()` 与减少动效/后台窗口暂停控制，headless 与
//!   软件渲染环境自动安全降级；
//! - `Canvas` 环形图完全使用主题语义色，高 DPI 下矢量清晰。

use std::sync::OnceLock;

use nsh_base::theme;
use nsh_core::canvas::LineCap;
use nsh_core::prelude::*;
use nsh_core::shader::Shader;

use crate::app_state::TrafficSnapshot;
use crate::ui;

/// 概览主卡微光流动氛围着色器（48 字节 16 字节对齐 Uniforms）。
const HERO_AURA_WGSL: &str = r#"
struct Uniforms {
    primary: vec4f,
    accent: vec4f,
    params: vec4f,
}

fn shade(uv: vec2f) -> vec4f {
    let running = uniforms.params.x;
    let t = nsh.time * (0.22 + running * 0.42);
    let p1 = vec2f(0.16 + 0.08 * sin(t * 0.85), 0.22 + 0.10 * cos(t * 0.65));
    let p2 = vec2f(0.84 + 0.07 * cos(t * 0.55), 0.74 + 0.09 * sin(t * 0.75));
    let d1 = length((uv - p1) * vec2f(1.35, 1.0));
    let d2 = length((uv - p2) * vec2f(1.25, 1.0));
    let g1 = exp(-d1 * d1 * 4.0);
    let g2 = exp(-d2 * d2 * 3.6);
    let strength = 0.07 + running * 0.10;
    let col = uniforms.primary.rgb * g1 + uniforms.accent.rgb * g2;
    let alpha = clamp((g1 * 0.65 + g2 * 0.45) * strength, 0.0, 0.26);
    return vec4f(col, alpha);
}
"#;

static HERO_AURA_SHADER: OnceLock<Option<Shader>> = OnceLock::new();

fn compiled_hero_aura() -> Option<Shader> {
    HERO_AURA_SHADER
        .get_or_init(|| match Shader::compile(HERO_AURA_WGSL) {
            Ok(compiled) => Some(compiled),
            Err(error) => {
                tracing::warn!(
                    target: "lanpath::overview_visuals",
                    %error,
                    "概览氛围着色器编译失败，已降级为静态卡片表面"
                );
                None
            }
        })
        .clone()
}

/// 挂载在概览 Hero 卡片底层的 WGSL 动态氛围光效；不可用时退化为空层。
pub(super) fn hero_aura_shader(running: impl Fn() -> bool + Copy + 'static) -> AnyElement {
    let Some(program) = compiled_hero_aura() else {
        return view().into_element();
    };
    if !shaders_available() {
        return view().into_element();
    }
    let reduced = ui::prefs::prefs().reduced;
    shader()
        .test_id("overview-hero-shader")
        .source(program)
        .uniform("primary", move || color(theme().primary))
        .uniform("accent", move || {
            color(
                theme()
                    .color("success")
                    .unwrap_or(theme().accent_foreground),
            )
        })
        .uniform("params", move || {
            [if running() { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0]
        })
        .paused(move || reduced.get() || !ui::motion::window_active())
        .absolute()
        .inset(px(0.0))
        .pointer_events(PointerEvents::None)
        .accessibility_elements_hidden(true)
        .into_element()
}

/// 原生 `Canvas` 代理 vs 直连比例环形仪表，居中显示代理占比。
pub(super) fn proxy_ratio_canvas(traffic: Signal<TrafficSnapshot>) -> AnyElement {
    const SIZE: f32 = 132.0;
    const CENTER: f32 = SIZE / 2.0;
    const RADIUS: f32 = 48.0;
    const STROKE: f32 = 12.0;

    view()
        .test_id("overview-ratio-canvas")
        .relative()
        .size(px(SIZE))
        .flex_none()
        .child(
            canvas(move || {
                let today = traffic.get().today;
                let total = today
                    .upload_bytes
                    .saturating_add(today.download_bytes)
                    .max(0) as f64;
                let proxied = today
                    .proxied_upload_bytes
                    .saturating_add(today.proxied_download_bytes)
                    .max(0) as f64;
                let ratio = if total > 0.0 {
                    (proxied / total).clamp(0.0, 1.0) as f32
                } else {
                    0.0
                };

                let t = theme();
                let track_color = color(t.muted);
                let proxy_color = color(t.primary);
                let direct_color = color(t.color("success").unwrap_or(t.secondary_foreground));

                let mut p = Painter::new()
                    .begin_path()
                    .arc(px(CENTER), px(CENTER), px(RADIUS), deg(0.0), deg(360.0))
                    .line_width(px(STROKE))
                    .stroke_color(track_color)
                    .stroke();

                if total > 0.0 {
                    let start_deg = -90.0_f32;
                    let proxy_sweep = ratio * 360.0;
                    let direct_sweep = (1.0 - ratio) * 360.0;
                    let gap = if proxy_sweep > 10.0 && direct_sweep > 10.0 {
                        10.0
                    } else {
                        0.0
                    };

                    if direct_sweep > 1.0 {
                        let d_start = start_deg + proxy_sweep + gap * 0.5;
                        let d_end = start_deg + 360.0 - gap * 0.5;
                        if d_end > d_start {
                            p = p
                                .begin_path()
                                .arc(px(CENTER), px(CENTER), px(RADIUS), deg(d_start), deg(d_end))
                                .line_width(px(STROKE))
                                .line_cap(LineCap::Round)
                                .stroke_color(direct_color)
                                .stroke();
                        }
                    }

                    if proxy_sweep > 1.0 {
                        let p_start = start_deg + gap * 0.5;
                        let p_end = start_deg + proxy_sweep - gap * 0.5;
                        if p_end > p_start {
                            p = p
                                .begin_path()
                                .arc(px(CENTER), px(CENTER), px(RADIUS), deg(p_start), deg(p_end))
                                .line_width(px(STROKE))
                                .line_cap(LineCap::Round)
                                .stroke_color(proxy_color)
                                .stroke();
                        }
                    }
                }
                p
            })
            .size(px(SIZE)),
        )
        .child(
            view()
                .absolute()
                .inset(px(0.0))
                .flex_col()
                .items_center()
                .justify_center()
                .child(
                    text(move || {
                        let today = traffic.get().today;
                        let total = today
                            .upload_bytes
                            .saturating_add(today.download_bytes)
                            .max(0) as u64;
                        let proxied = today
                            .proxied_upload_bytes
                            .saturating_add(today.proxied_download_bytes)
                            .max(0) as u64;
                        let pct = proxied.saturating_mul(100).checked_div(total).unwrap_or(0);
                        format!("{pct}%")
                    })
                    .text_size(px(20.0))
                    .font_weight(FontWeight::BOLD),
                )
                .child(
                    text(|| t!("overview.analytics.proxied_short"))
                        .text_size(px(11.0))
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .into_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hero_aura_wgsl_compiles_and_validates() {
        let compiled = Shader::compile(HERO_AURA_WGSL).expect("WGSL hero aura shader must compile");
        assert!(compiled.animated());
    }
}
