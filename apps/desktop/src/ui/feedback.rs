//! 反馈组件：Badge、Spinner、Skeleton、Empty、StatusBar 与 Toast。
//! 全部只按语义 token 取色；动效遵守动效 token 与减少动效偏好。

use std::cell::RefCell;
use std::time::Duration;

use nsh_base::{
    Anchor, ToastManager, ToastMotion, ToastOptions, ToastStackState, ToastTransitionStatus,
};
use nsh_base::{theme, toast, toast_stack};
use nsh_core::prelude::*;
use nsh_core::time::now;

/// Badge 语义变体。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeVariant {
    Default,
    Success,
    Warning,
    Destructive,
    Outline,
}

fn badge_bg(variant: BadgeVariant) -> fn(&nsh_base::Theme) -> AdaptiveColor {
    match variant {
        BadgeVariant::Default => |t| t.secondary,
        BadgeVariant::Success => |t| t.color("success-surface").unwrap_or(t.secondary),
        BadgeVariant::Warning => |t| t.color("warning-surface").unwrap_or(t.secondary),
        BadgeVariant::Destructive => |t| t.destructive,
        BadgeVariant::Outline => |_| transparent_black().into(),
    }
}

fn badge_fg(variant: BadgeVariant) -> fn(&nsh_base::Theme) -> AdaptiveColor {
    match variant {
        BadgeVariant::Default => |t| t.secondary_foreground,
        BadgeVariant::Success => |t| t.color("success").unwrap_or(t.secondary_foreground),
        BadgeVariant::Warning => |t| t.color("warning").unwrap_or(t.secondary_foreground),
        BadgeVariant::Destructive => |t| {
            t.color("destructive-foreground")
                .unwrap_or(t.secondary_foreground)
        },
        BadgeVariant::Outline => |t| t.foreground,
    }
}

/// 一个徽章（小标签；自适应最小高度以兼容系统文字放大）。
pub fn badge(variant: BadgeVariant, label: impl Fn() -> String + 'static) -> AnyElement {
    badge_with_icon(variant, None, label)
}

/// 带可选语义图标的徽章（不只靠颜色传递状态）。
pub fn badge_with_icon(
    variant: BadgeVariant,
    leading: Option<super::icon::Icon>,
    label: impl Fn() -> String + 'static,
) -> AnyElement {
    let (bg, fg) = (badge_bg(variant), badge_fg(variant));
    let bordered = matches!(variant, BadgeVariant::Outline);
    let tone = match variant {
        BadgeVariant::Success => super::icon::Tone::Success,
        BadgeVariant::Warning => super::icon::Tone::Warning,
        BadgeVariant::Destructive => super::icon::Tone::OnPrimary,
        BadgeVariant::Default | BadgeVariant::Outline => super::icon::Tone::Muted,
    };
    let mut el = view()
        .flex_row()
        .min_h(px(22.0))
        .py(px(2.0))
        .px(px(8.0))
        .gap_1()
        .items_center()
        .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
        .bg(move || bg(&theme()))
        .text_color(move || fg(&theme()));
    if bordered {
        el = el.border(px(1.0)).border_color(move || theme().border);
    }
    if let Some(kind) = leading {
        el = el.child(super::icon::icon_sized(kind, tone, 12.0));
    }
    el.child(
        text(label)
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM),
    )
    .into_element()
}

/// 开口圆环绘制（spinner 的静态形态）。
fn spinner_ring(variant: Option<super::ButtonVariant>) -> AnyElement {
    view()
        .size(px(16.0))
        .child(canvas(move || {
            Painter::new()
                .begin_path()
                .arc(px(8.0), px(8.0), px(6.0), radians(-120.0), radians(180.0))
                .line_width(px(2.0))
                .line_cap(nsh_core::canvas::LineCap::Round)
                .stroke_color(color(match variant {
                    Some(super::ButtonVariant::Primary) => theme().primary_foreground,
                    Some(super::ButtonVariant::Destructive) => theme()
                        .color("destructive-foreground")
                        .unwrap_or(theme().foreground),
                    _ => theme().primary,
                }))
                .stroke()
        }))
        .into_element()
}

/// 加载 spinner：主题主色的开口圆环，循环旋转；减少动效（系统或本应用
/// 偏好）时渲染为静止弧，立即可读。
pub fn spinner() -> AnyElement {
    spinner_variant(None)
}

pub(super) fn button_spinner(variant: super::ButtonVariant) -> AnyElement {
    spinner_variant(Some(variant))
}

fn spinner_variant(variant: Option<super::ButtonVariant>) -> AnyElement {
    let reduced = super::prefs::prefs().reduced;
    view()
        .child(when(
            move || !reduced.get() && super::motion::window_active(),
            move || {
                let spin = motion();
                let spec = MotionSpec::new(
                    KeyframeTiming::new(900.0)
                        .easing(Easing::Linear)
                        .iterations(None),
                )
                .track(
                    MotionChannel::Rotate,
                    [Keyframe::new(0.0), Keyframe::new(360.0)],
                );
                let spin = spin.entering(spec);
                view().child(view().bind_motion(&spin).child(spinner_ring(variant)))
            },
        ))
        .child(when(
            move || reduced.get() || !super::motion::window_active(),
            move || spinner_ring(variant),
        ))
        .into_element()
}

/// 骨架屏占位：静态 `muted` 表面，加载完成后由真实内容替换。
pub fn skeleton() -> AnyElement {
    skeleton_line(None)
}

/// 指定宽度的骨架屏条（`None` 占满父容器）。
pub fn skeleton_line(width: Option<f32>) -> AnyElement {
    let base = view()
        .h(px(14.0))
        .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
        .bg(move || theme().muted);
    match width {
        Some(w) => base.w(px(w)).into_element(),
        None => base.w_full().into_element(),
    }
}

/// 空状态：标题 + 说明，居中、弱化色。
pub fn empty(
    title: impl Fn() -> String + 'static,
    description: impl Fn() -> String + 'static,
) -> AnyElement {
    empty_with_icon(None, title, description)
}

/// 带图标的空状态：温和提示对象与下一步。
pub fn empty_with_icon(
    leading: Option<super::icon::Icon>,
    title: impl Fn() -> String + 'static,
    description: impl Fn() -> String + 'static,
) -> AnyElement {
    let mut el = view()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .p_6();
    if let Some(kind) = leading {
        el = el.child(
            view()
                .size(px(40.0))
                .rounded(move || theme().radius)
                .bg(move || theme().muted)
                .items_center()
                .justify_center()
                .child(super::icon::icon_sized(
                    kind,
                    super::icon::Tone::Muted,
                    20.0,
                )),
        );
    }
    el.child(
        text(title)
            .text_size(px(15.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(move || theme().foreground),
    )
    .child(
        text(description)
            .text_size(px(13.0))
            .text_color(move || theme().muted_foreground),
    )
    .into_element()
}

/// 窗口状态栏：底部通栏，左/右内容槽，辅助字号，适配磨砂透明度。
pub fn status_bar(left: impl IntoElement, right: impl IntoElement) -> AnyElement {
    view()
        .flex_row()
        .min_h(px(30.0))
        .gap_4()
        .flex_none()
        .text_size(px(12.0))
        .px(px(14.0))
        .items_center()
        .justify_between()
        .border_t(px(1.0))
        .border_color(move || theme().border)
        .bg(super::prefs::chrome_bg)
        .child(left)
        .child(right)
        .into_element()
}

// --- Toast -------------------------------------------------------------------

struct ToastBus {
    manager: Option<ToastManager<u64, SharedString>>,
    next_id: std::cell::Cell<u64>,
}

impl Default for ToastBus {
    fn default() -> Self {
        Self {
            manager: None,
            next_id: std::cell::Cell::new(1),
        }
    }
}

thread_local! {
    static BUS: RefCell<ToastBus> = RefCell::new(ToastBus::default());
}

fn with_bus<R>(f: impl FnOnce(&mut ToastBus) -> R) -> R {
    BUS.with(|bus| f(&mut bus.borrow_mut()))
}

/// 显示一条 toast（5 秒超时）。随处可调用；由 [`toast_host`] 挂载的
/// manager 消费。当前仅画廊使用，业务页引入时同一入口。
pub fn show_toast(message: impl Into<SharedString>) {
    with_bus(|bus| {
        let manager = bus
            .manager
            .get_or_insert_with(|| ToastManager::new(ToastMotion::sonner()));
        let id = bus.next_id.get();
        bus.next_id.set(id + 1);
        manager.push(
            id,
            message.into(),
            ToastOptions::timeout(Some(Duration::from_secs(5))),
            now(),
        );
    });
}

/// Toast 宿主：驱动 manager 生命周期（常驻定时器），仅在确有可见
/// toast 时挂载 `toast_stack`——空栈不挂载浮层，不拦截指针命中。
pub fn toast_host() -> AnyElement {
    with_bus(|bus| bus.manager = Some(ToastManager::new(ToastMotion::sonner())));
    let shown = signal(Vec::<(u64, SharedString, ToastTransitionStatus)>::new());
    let state = ToastStackState::new();
    let sync = move || {
        shown.set(with_bus(|bus| {
            bus.manager.as_ref().map_or_else(Vec::new, |m| {
                m.visible(3)
                    .map(|(id, text, status)| (*id, text.clone(), status))
                    .collect()
            })
        }));
    };
    set_interval(Duration::from_millis(50), move || {
        let changed = with_bus(|bus| match bus.manager.as_mut() {
            Some(manager) => manager.advance(now(), state.is_expanded()).changed,
            None => false,
        });
        if changed {
            sync();
        }
    });
    when(
        move || !shown.get().is_empty(),
        move || {
            toast_stack()
                .placement(Anchor::BottomRight)
                .state(state)
                .w(px(320.0))
                .child(each(
                    shown,
                    |(id, _, _)| *id,
                    move |item, _| {
                        let id = move || item.get().0;
                        toast()
                            .transition_status(move || item.get().2)
                            .accessibility_label(move || Some(item.get().1))
                            .on_dismiss_request(move || {
                                with_bus(|bus| {
                                    if let Some(manager) = bus.manager.as_mut() {
                                        manager.dismiss(&id(), now());
                                    }
                                });
                            })
                            .rounded(move || theme().radius)
                            .px(px(16.0))
                            .py(px(12.0))
                            .bg(move || theme().popover)
                            .text_color(move || theme().popover_foreground)
                            .border(px(1.0))
                            .border_color(move || theme().border)
                            .shadow_lg()
                            .child(text(move || item.get().1).text_size(px(13.0)))
                    },
                ))
        },
    )
    .into_element()
}
