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
        BadgeVariant::Success => |t| t.color("success").unwrap_or(t.secondary),
        BadgeVariant::Warning => |t| t.color("warning").unwrap_or(t.secondary),
        BadgeVariant::Destructive => |t| t.destructive,
        BadgeVariant::Outline => |_| transparent_black().into(),
    }
}

fn badge_fg(variant: BadgeVariant) -> fn(&nsh_base::Theme) -> AdaptiveColor {
    match variant {
        BadgeVariant::Default => |t| t.secondary_foreground,
        BadgeVariant::Success => |t| {
            t.color("success-foreground")
                .unwrap_or(t.secondary_foreground)
        },
        BadgeVariant::Warning => |t| {
            t.color("warning-foreground")
                .unwrap_or(t.secondary_foreground)
        },
        BadgeVariant::Destructive => |t| {
            t.color("destructive-foreground")
                .unwrap_or(t.secondary_foreground)
        },
        BadgeVariant::Outline => |t| t.foreground,
    }
}

/// 一个徽章（小标签）。
pub fn badge(variant: BadgeVariant, label: impl Fn() -> String + 'static) -> AnyElement {
    let (bg, fg) = (badge_bg(variant), badge_fg(variant));
    let bordered = matches!(variant, BadgeVariant::Outline);
    let mut el = view()
        .h(px(20.0))
        .px(px(8.0))
        .items_center()
        .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
        .bg(move || bg(&theme()))
        .text_color(move || fg(&theme()));
    if bordered {
        el = el.border(px(1.0)).border_color(move || theme().border);
    }
    el.child(
        text(label)
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM),
    )
    .into_element()
}

/// 开口圆环绘制（spinner 的静态形态）。
fn spinner_ring() -> AnyElement {
    view()
        .size(px(16.0))
        .child(canvas(move || {
            Painter::new()
                .begin_path()
                .arc(px(8.0), px(8.0), px(6.0), radians(-120.0), radians(180.0))
                .line_width(px(2.0))
                .line_cap(nsh_core::canvas::LineCap::Round)
                .stroke_color(color(theme().primary))
                .stroke()
        }))
        .into_element()
}

/// 加载 spinner：主题主色的开口圆环，循环旋转；减少动效（系统或本应用
/// 偏好）时渲染为静止弧，立即可读。
pub fn spinner() -> AnyElement {
    let reduced = super::prefs::prefs().reduced;
    view()
        .child(when(
            move || !reduced.get(),
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
                view().child(view().bind_motion(&spin).child(spinner_ring()))
            },
        ))
        .child(when(move || reduced.get(), spinner_ring))
        .into_element()
}

/// 骨架屏占位：静态 `muted` 表面，加载完成后由真实内容替换。
pub fn skeleton() -> AnyElement {
    view()
        .h(px(16.0))
        .w_full()
        .rounded(move || theme().radius)
        .bg(move || theme().muted)
        .into_element()
}

/// 空状态：标题 + 说明，居中、弱化色。
pub fn empty(
    title: impl Fn() -> String + 'static,
    description: impl Fn() -> String + 'static,
) -> AnyElement {
    view()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .p_6()
        .child(
            text(title)
                .text_size(px(15.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || theme().foreground),
        )
        .child(
            text(description)
                .text_size(px(13.0))
                .text_color(move || theme().muted_foreground),
        )
        .into_element()
}

/// 窗口状态栏：底部通栏，左/右内容槽，辅助字号。
pub fn status_bar(left: impl IntoElement, right: impl IntoElement) -> AnyElement {
    view()
        .flex_row()
        .h(px(28.0))
        .px(px(12.0))
        .items_center()
        .justify_between()
        .border_t(px(1.0))
        .border_color(move || theme().border)
        .bg(move || theme().card)
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
                            .px(px(12.0))
                            .py(px(10.0))
                            .bg(move || theme().card)
                            .text_color(move || theme().card_foreground)
                            .border(px(1.0))
                            .border_color(move || theme().border)
                            .child(text(move || item.get().1).text_size(px(13.0)))
                    },
                ))
        },
    )
    .into_element()
}
