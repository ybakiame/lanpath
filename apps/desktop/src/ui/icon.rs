//! 循径线性图标。SVG 是静态几何，颜色由原生 tint token 响应式提供。
use nsh_base::theme;
use nsh_core::element::ImageSrc;
use nsh_core::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Overview,
    Sidebar,
    Subscription,
    Core,
    Rules,
    Code,
    Sliders,
    File,
    Activity,
    Logs,
    Backup,
    Appearance,
    Play,
    Stop,
    Check,
    Shield,
    Refresh,
    Port,
    More,
    Arrow,
    Plus,
    Warning,
    Clock,
    Chevron,
    Sun,
    Moon,
    Sparkles,
    Search,
    Copy,
    Trash,
    Layers,
    Download,
    Close,
}

impl Icon {
    /// 全部循径线性图标，供组件画廊验收展示。
    pub const ALL: &'static [(Icon, &'static str)] = &[
        (Icon::Overview, "Overview"),
        (Icon::Sidebar, "Sidebar"),
        (Icon::Subscription, "Subscription"),
        (Icon::Core, "Core"),
        (Icon::Rules, "Rules"),
        (Icon::Code, "Code"),
        (Icon::Sliders, "Sliders"),
        (Icon::File, "File"),
        (Icon::Activity, "Activity"),
        (Icon::Logs, "Logs"),
        (Icon::Backup, "Backup"),
        (Icon::Appearance, "Appearance"),
        (Icon::Play, "Play"),
        (Icon::Stop, "Stop"),
        (Icon::Check, "Check"),
        (Icon::Shield, "Shield"),
        (Icon::Refresh, "Refresh"),
        (Icon::Port, "Port"),
        (Icon::More, "More"),
        (Icon::Arrow, "Arrow"),
        (Icon::Plus, "Plus"),
        (Icon::Warning, "Warning"),
        (Icon::Clock, "Clock"),
        (Icon::Chevron, "Chevron"),
        (Icon::Sun, "Sun"),
        (Icon::Moon, "Moon"),
        (Icon::Sparkles, "Sparkles"),
        (Icon::Search, "Search"),
        (Icon::Copy, "Copy"),
        (Icon::Trash, "Trash"),
        (Icon::Layers, "Layers"),
        (Icon::Download, "Download"),
        (Icon::Close, "Close"),
    ];
}

#[derive(Clone, Copy, Default)]
pub enum Tone {
    #[default]
    Muted,
    Foreground,
    Primary,
    OnPrimary,
    Success,
    Warning,
    Danger,
}

fn geometry(icon: Icon) -> &'static str {
    match icon {
        Icon::Sidebar => "<rect x='3' y='3' width='18' height='18' rx='2'/><path d='M9 3v18'/>",
        Icon::Overview => {
            "<rect x='3' y='3' width='18' height='18' rx='3'/><path d='M3 10h18M10 10v11'/>"
        }
        Icon::Subscription => {
            "<rect x='6' y='3' width='14' height='15' rx='2'/><path d='M3 7v14h13M10 8h6M10 12h6'/>"
        }
        Icon::Core => {
            "<rect x='6' y='6' width='12' height='12' rx='2'/><path d='M9 2v4m6-4v4M9 18v4m6-4v4M2 9h4m-4 6h4m12-6h4m-4 6h4'/><rect x='9' y='9' width='6' height='6' rx='1'/>"
        }
        Icon::Rules => {
            "<path d='M6 5v11a4 4 0 0 0 4 4h8M6 9h8a4 4 0 0 0 4-4'/><circle cx='6' cy='3' r='2'/><circle cx='18' cy='3' r='2'/><circle cx='20' cy='20' r='2'/>"
        }
        Icon::Code => "<path d='m8 6-6 6 6 6m8-12 6 6-6 6m-3-14-2 16'/>",
        Icon::Sliders => {
            "<path d='M3 6h6m4 0h8M3 18h12m4 0h2M3 12h2m4 0h12'/><circle cx='11' cy='6' r='2'/><circle cx='7' cy='12' r='2'/><circle cx='17' cy='18' r='2'/>"
        }
        Icon::File => "<path d='M14 2H5v20h14V7ZM14 2v5h5M8 12h8m-8 4h8'/>",
        Icon::Activity => "<path d='M2 12h4l3-7 5 14 3-7h5'/>",
        Icon::Logs => "<path d='M5 4h14M5 9h10M5 14h14M5 19h9'/>",
        Icon::Backup => {
            "<rect x='3' y='3' width='18' height='5' rx='1'/><path d='M5 8v13h14V8M10 12h4'/>"
        }
        Icon::Appearance => {
            "<circle cx='12' cy='12' r='9'/><path d='M12 3v18a9 9 0 0 0 0-18Z' fill='currentColor' stroke='none'/>"
        }
        Icon::Play => "<path d='m8 4 12 8-12 8Z'/>",
        Icon::Stop => "<rect x='5' y='5' width='14' height='14' rx='2'/>",
        Icon::Check => "<path d='m5 12 4 4L19 6'/>",
        Icon::Shield => {
            "<path d='m12 2 8 4v6c0 5-8 10-8 10S4 17 4 12V6Z'/><path d='m8 12 3 3 5-6'/>"
        }
        Icon::Refresh => {
            "<path d='M20 8a8 8 0 0 0-14-3L3 8m0-5v5h5M4 16a8 8 0 0 0 14 3l3-3m0 5v-5h-5'/>"
        }
        Icon::Port => {
            "<rect x='3' y='5' width='18' height='14' rx='3'/><path d='M7 10h10m-8 4h6M7 3v2m10-2v2'/>"
        }
        Icon::More => {
            "<circle cx='5' cy='12' r='1'/><circle cx='12' cy='12' r='1'/><circle cx='19' cy='12' r='1'/>"
        }
        Icon::Arrow => "<path d='M5 12h14m-6-6 6 6-6 6'/>",
        Icon::Plus => "<path d='M12 4v16M4 12h16'/>",
        Icon::Warning => "<path d='m12 3 10 18H2Z'/><path d='M12 9v5m0 3v.1'/>",
        Icon::Clock => "<circle cx='12' cy='12' r='9'/><path d='M12 7v5l3 2'/>",
        Icon::Chevron => "<path d='m6 9 6 6 6-6'/>",
        Icon::Sun => {
            "<circle cx='12' cy='12' r='4'/><path d='M12 2v2m0 16v2M4.93 4.93l1.41 1.41m11.32 11.32 1.41 1.41M2 12h2m16 0h2M4.93 19.07l1.41-1.41m11.32-11.32 1.41-1.41'/>"
        }
        Icon::Moon => "<path d='M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8Z'/>",
        Icon::Sparkles => {
            "<path d='m12 3 1.9 5.1L19 10l-5.1 1.9L12 17l-1.9-5.1L5 10l5.1-1.9ZM19 3v4m2-2h-4M5 17v4m2-2H3'/>"
        }
        Icon::Search => "<circle cx='11' cy='11' r='7'/><path d='m20 20-3.5-3.5'/>",
        Icon::Copy => {
            "<rect x='9' y='9' width='12' height='12' rx='2'/><path d='M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1'/>"
        }
        Icon::Trash => "<path d='M3 6h18M8 6V4h8v2m-9 0 1 14h8l1-14M10 11v5m4-5v5'/>",
        Icon::Layers => "<path d='m12 2 9 5-9 5-9-5 9-5ZM3 12l9 5 9-5M3 17l9 5 9-5'/>",
        Icon::Download => {
            "<path d='M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3'/>"
        }
        Icon::Close => "<path d='M18 6 6 18M6 6l12 12'/>",
    }
}

pub fn icon(kind: Icon, tone: Tone) -> AnyElement {
    icon_sized(kind, tone, 18.0)
}

/// 按组件档位（sm 14 / md 16 / lg 18 pt）绘制线性图标。
pub fn icon_sized(kind: Icon, tone: Tone, size: f32) -> AnyElement {
    let source = ImageSrc::svg_markup(&format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='1.75' stroke-linecap='round' stroke-linejoin='round'>{}</svg>",
        geometry(kind)
    ));
    svg(source)
        .size(px(size))
        .flex_none()
        .accessibility_elements_hidden(true)
        .tint_color(move || {
            let t = theme();
            match tone {
                Tone::Muted => t.muted_foreground,
                Tone::Foreground => t.foreground,
                Tone::Primary => t.primary,
                Tone::OnPrimary => t.primary_foreground,
                Tone::Success => t.color("success").unwrap_or(t.primary),
                Tone::Warning => t.color("warning").unwrap_or(t.foreground),
                Tone::Danger => t.destructive,
            }
        })
        .into_element()
}

/// 状态含义改变时的一次性图形交换；不是持续装饰动画。
pub fn status_icon(kind: Icon, tone: Tone) -> AnyElement {
    let reduced = super::prefs::prefs().reduced;
    when(
        move || !reduced.get(),
        move || {
            let reveal = motion().entering(
                MotionSpec::new(
                    KeyframeTiming::new(super::motion::BASE_MS).easing(Easing::EaseOut),
                )
                .track(
                    MotionChannel::Opacity,
                    [Keyframe::new(0.0), Keyframe::new(1.0)],
                )
                .track(
                    MotionChannel::ScaleX,
                    [Keyframe::new(0.9), Keyframe::new(1.0)],
                )
                .track(
                    MotionChannel::ScaleY,
                    [Keyframe::new(0.9), Keyframe::new(1.0)],
                ),
            );
            view().bind_motion(&reveal).child(icon(kind, tone))
        },
    )
    .otherwise(move || icon(kind, tone))
    .into_element()
}
