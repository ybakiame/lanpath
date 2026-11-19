//! 语义按钮：`nsh_base::button`（行为原语）的薄封装，外观完全由语义
//! token 驱动——功能页只传 variant/size/label/状态与事件，不传颜色、
//! 圆角或字体。状态覆盖 normal/hover/pressed/focus-visible/disabled/
//! loading；正常动效下 press 缩放 0.97（图标按钮 0.95），减少动效时
//! 只做即时颜色反馈、不缩放，时长 fast token。

use std::rc::Rc;

use nsh_base::{button as base_button, theme};
use nsh_core::prelude::*;

use super::motion::{self, PRESS_SCALE, PRESS_SCALE_ICON};

/// 按钮语义变体。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    /// 主提交。
    Primary,
    /// 次要动作。
    Secondary,
    /// 描边动作。
    Outline,
    /// 低强调动作（工具栏、行内）。
    Ghost,
    /// 危险动作（需配合确认）。
    Destructive,
}

/// Button sizes; Content fits compound content without a fixed height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonSize {
    Sm,
    Md,
    Lg,
    Content,
}

impl ButtonSize {
    fn height(self) -> Option<f32> {
        match self {
            ButtonSize::Sm => Some(28.0),
            ButtonSize::Md => Some(32.0),
            ButtonSize::Lg => Some(40.0),
            ButtonSize::Content => None,
        }
    }

    fn text(self) -> f32 {
        match self {
            ButtonSize::Sm => 12.0,
            ButtonSize::Md | ButtonSize::Lg | ButtonSize::Content => 14.0,
        }
    }

    fn pad_x(self) -> f32 {
        match self {
            ButtonSize::Sm => 12.0,
            ButtonSize::Md => 16.0,
            ButtonSize::Lg => 20.0,
            ButtonSize::Content => 4.0,
        }
    }
}

/// 变体的交互表面（normal / hover / active 的底色与文字，是否描边）。
/// 状态色从主题的成对 token 读取；Outline/Ghost 的透明静息面叠加 accent
/// 的成对状态色，避免用半透明色冲淡 Primary/Destructive 的文字对比度。
#[derive(Clone, Copy)]
struct Surface {
    bg: fn(&nsh_base::Theme) -> AdaptiveColor,
    fg: fn(&nsh_base::Theme) -> AdaptiveColor,
    hover_bg: fn(&nsh_base::Theme) -> AdaptiveColor,
    hover_fg: fn(&nsh_base::Theme) -> AdaptiveColor,
    active_bg: fn(&nsh_base::Theme) -> AdaptiveColor,
    active_fg: fn(&nsh_base::Theme) -> AdaptiveColor,
    bordered: bool,
}

/// 主题自定义状态 token，缺项时使用 normal 语义色（主题注册单测保证
/// 这些 token 成对存在）。
fn state_color(theme: &nsh_base::Theme, name: &str, fallback: AdaptiveColor) -> AdaptiveColor {
    theme.color(name).unwrap_or(fallback)
}

fn surface(variant: ButtonVariant) -> Surface {
    match variant {
        ButtonVariant::Primary => Surface {
            bg: |t| t.primary,
            fg: |t| t.primary_foreground,
            hover_bg: |t| state_color(t, "primary-hover", t.primary),
            hover_fg: |t| state_color(t, "primary-hover-foreground", t.primary_foreground),
            active_bg: |t| state_color(t, "primary-active", t.primary),
            active_fg: |t| state_color(t, "primary-active-foreground", t.primary_foreground),
            bordered: false,
        },
        ButtonVariant::Secondary => Surface {
            bg: |t| t.secondary,
            fg: |t| t.secondary_foreground,
            hover_bg: |t| state_color(t, "secondary-hover", t.secondary),
            hover_fg: |t| state_color(t, "secondary-hover-foreground", t.secondary_foreground),
            active_bg: |t| state_color(t, "secondary-active", t.secondary),
            active_fg: |t| state_color(t, "secondary-active-foreground", t.secondary_foreground),
            bordered: false,
        },
        ButtonVariant::Outline => Surface {
            bg: |_| transparent_black().into(),
            fg: |t| t.foreground,
            hover_bg: |t| t.accent,
            hover_fg: |t| t.accent_foreground,
            active_bg: |t| state_color(t, "accent-active", t.accent),
            active_fg: |t| state_color(t, "accent-active-foreground", t.accent_foreground),
            bordered: true,
        },
        ButtonVariant::Ghost => Surface {
            bg: |_| transparent_black().into(),
            fg: |t| t.foreground,
            hover_bg: |t| t.accent,
            hover_fg: |t| t.accent_foreground,
            active_bg: |t| state_color(t, "accent-active", t.accent),
            active_fg: |t| state_color(t, "accent-active-foreground", t.accent_foreground),
            bordered: false,
        },
        ButtonVariant::Destructive => Surface {
            bg: |t| t.destructive,
            fg: |t| t.color("destructive-foreground").unwrap_or(t.foreground),
            hover_bg: |t| state_color(t, "destructive-hover", t.destructive),
            hover_fg: |t| {
                state_color(
                    t,
                    "destructive-hover-foreground",
                    t.color("destructive-foreground").unwrap_or(t.foreground),
                )
            },
            active_bg: |t| state_color(t, "destructive-active", t.destructive),
            active_fg: |t| {
                state_color(
                    t,
                    "destructive-active-foreground",
                    t.color("destructive-foreground").unwrap_or(t.foreground),
                )
            },
            bordered: false,
        },
    }
}

/// 语义按钮构建器。
pub struct Button {
    variant: ButtonVariant,
    size: ButtonSize,
    label: Option<Box<dyn Fn() -> String>>,
    disabled: Option<Box<dyn Fn() -> bool>>,
    loading: Option<Box<dyn Fn() -> bool>>,
    prefix: Option<Rc<dyn Fn() -> AnyElement>>,
    on_press: Option<Box<dyn Fn()>>,
    test_id: Option<&'static str>,
    accessibility_label: Option<SharedString>,
    accessible_name: Option<Box<dyn Fn() -> SharedString>>,
    selected: Option<Box<dyn Fn() -> bool>>,
    full_width: bool,
}

pub fn button(variant: ButtonVariant) -> Button {
    Button {
        variant,
        size: ButtonSize::Md,
        label: None,
        disabled: None,
        loading: None,
        prefix: None,
        on_press: None,
        test_id: None,
        accessibility_label: None,
        accessible_name: None,
        selected: None,
        full_width: false,
    }
}

impl Button {
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// 文本槽（响应式闭包，随语言切换更新）。
    pub fn label(mut self, label: impl Fn() -> String + 'static) -> Self {
        self.label = Some(Box::new(label));
        self
    }

    pub fn disabled(mut self, disabled: impl Fn() -> bool + 'static) -> Self {
        self.disabled = Some(Box::new(disabled));
        self
    }

    /// 加载中：前缀换成 spinner 并禁止按下。
    pub fn loading(mut self, loading: impl Fn() -> bool + 'static) -> Self {
        self.loading = Some(Box::new(loading));
        self
    }

    /// 前缀内容槽（图标）；用工厂而非单个元素，loading 切回后仍能重建。
    pub fn prefix(mut self, prefix: impl Fn() -> AnyElement + 'static) -> Self {
        self.prefix = Some(Rc::new(prefix));
        self
    }

    pub fn on_press(mut self, on_press: impl Fn() + 'static) -> Self {
        self.on_press = Some(Box::new(on_press));
        self
    }

    pub fn test_id(mut self, id: &'static str) -> Self {
        self.test_id = Some(id);
        self
    }

    /// 无障碍名称（纯图标按钮必须提供）。
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }

    /// 响应式无障碍名称：语言切换后立即重读（主题卡等名称含用户词）。
    pub fn accessible_name(mut self, label: impl Fn() -> SharedString + 'static) -> Self {
        self.accessible_name = Some(Box::new(label));
        self
    }

    /// 选中语义（当前主题、当前筛选等）；外观由调用方的卡片/列表表达。
    pub fn selected(mut self, selected: impl Fn() -> bool + 'static) -> Self {
        self.selected = Some(Box::new(selected));
        self
    }

    /// 让按钮和内容占满父级宽度；用于网格卡片等需要对齐扫描轴的复合按钮。
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }
}

impl IntoElement for Button {
    fn into_element(self) -> AnyElement {
        let surface = surface(self.variant);
        let size = self.size;
        let full_width = self.full_width;
        let disabled = Rc::new(self.disabled);
        let loading = Rc::new(self.loading);
        let blocked_disabled = disabled.clone();
        let blocked_loading = loading.clone();
        let blocked: Rc<dyn Fn() -> bool> = Rc::new(move || {
            blocked_disabled.as_ref().as_ref().is_some_and(|f| f())
                || blocked_loading.as_ref().as_ref().is_some_and(|f| f())
        });
        let blocked_for_disabled = blocked.clone();
        let blocked_for_press = blocked.clone();
        let pressed = signal(false);
        let press_down = pressed;
        let press_up = pressed;
        let scaled = pressed;
        let reduced = super::prefs::prefs().reduced;

        let mut el = base_button()
            .disabled(move || blocked_for_disabled())
            .on_press_down(move || press_down.set(true))
            .on_press_up(move || press_up.set(false));
        if full_width {
            el = el.w_full();
        }
        if let Some(selected) = self.selected {
            let selected = Rc::new(selected);
            el = el.selected(move || selected());
        }
        if let Some(height) = size.height() {
            el = el.h(px(height));
        }
        let mut el = el
            .px(px(size.pad_x()))
            .justify_center()
            .items_center()
            .gap_2()
            .rounded(move || theme().radius)
            .cursor_pointer()
            .transition_colors()
            .duration(move || motion::motion_duration(motion::FAST_MS, reduced.get()))
            .bg(move || (surface.bg)(&theme()))
            .text_color(move || (surface.fg)(&theme()))
            .hover(|s| {
                s.bg(move || (surface.hover_bg)(&theme()))
                    .text_color(move || (surface.hover_fg)(&theme()))
            })
            .active(|s| {
                s.bg(move || (surface.active_bg)(&theme()))
                    .text_color(move || (surface.active_fg)(&theme()))
            })
            .disabled_style(|s| s.opacity(0.5))
            .focus_visible(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
            .scale(move || {
                if scaled.get() && !reduced.get() {
                    PRESS_SCALE
                } else {
                    1.0
                }
            })
            .transition_transform()
            .duration(move || motion::motion_duration(motion::FAST_MS, reduced.get()));
        if surface.bordered {
            el = el.border(px(1.0)).border_color(move || theme().border);
        }
        if let Some(on_press) = self.on_press {
            el = el.on_press(move || {
                if !blocked_for_press() {
                    on_press();
                }
            });
        }
        if let Some(accessibility_label) = self.accessibility_label {
            el = el.accessibility_label(accessibility_label);
        }
        if let Some(accessible_name) = self.accessible_name {
            el = el.accessibility_label(move || Some(accessible_name()));
        }

        let loading_for_content = loading.clone();
        let prefix = self.prefix;
        let has_leading = prefix.is_some() || loading.is_some();
        let leading = when(
            move || loading_for_content.as_ref().as_ref().is_some_and(|f| f()),
            || {
                view()
                    .test_id("button-loading-indicator")
                    .child(super::feedback::spinner())
                    .into_element()
            },
        );
        let leading = match prefix {
            Some(build) => leading.otherwise(move || build()).into_element(),
            None => leading.into_element(),
        };
        let mut children: Vec<AnyElement> = Vec::new();
        if has_leading {
            children.push(leading);
        }
        if let Some(label) = self.label {
            children.push(
                text(label)
                    .text_size(px(size.text()))
                    .font_weight(FontWeight::MEDIUM)
                    .into_element(),
            );
        }
        // Keep the content anchor inside the button so test presses reach its handler.
        let content: AnyElement = view()
            .flex_row()
            .items_center()
            .gap_2()
            .when(!full_width, |content| content.justify_center())
            .when(full_width, |content| content.w_full())
            .children(children)
            .into_element();
        let content = match self.test_id {
            Some(id) => view().test_id(id).child(content).into_element(),
            None => content,
        };
        el.child(content).into_element()
    }
}

/// 图标按钮：正方形幽灵按钮，可访问名称必填（纯图标对屏幕阅读器
/// 不可见），press 缩放 0.95。
pub struct IconButton {
    size: f32,
    content: Option<AnyElement>,
    disabled: Option<Box<dyn Fn() -> bool>>,
    on_press: Option<Box<dyn Fn()>>,
    test_id: Option<&'static str>,
    accessibility_label: Option<Box<dyn Fn() -> SharedString>>,
}

pub fn icon_button() -> IconButton {
    IconButton {
        size: 32.0,
        content: None,
        disabled: None,
        on_press: None,
        test_id: None,
        accessibility_label: None,
    }
}

impl IconButton {
    /// 工具尺寸 28（默认 32）。
    pub fn tool_size(mut self) -> Self {
        self.size = 28.0;
        self
    }

    pub fn child(mut self, content: impl IntoElement) -> Self {
        self.content = Some(content.into_element());
        self
    }

    pub fn disabled(mut self, disabled: impl Fn() -> bool + 'static) -> Self {
        self.disabled = Some(Box::new(disabled));
        self
    }

    pub fn on_press(mut self, on_press: impl Fn() + 'static) -> Self {
        self.on_press = Some(Box::new(on_press));
        self
    }

    pub fn test_id(mut self, id: &'static str) -> Self {
        self.test_id = Some(id);
        self
    }

    pub fn accessibility_label(mut self, label: impl Fn() -> SharedString + 'static) -> Self {
        self.accessibility_label = Some(Box::new(label));
        self
    }
}

impl IntoElement for IconButton {
    fn into_element(self) -> AnyElement {
        let label = self
            .accessibility_label
            .unwrap_or_else(|| panic!("icon_button 必须提供 accessibility_label"));
        let label = move || Some(label());
        let size = self.size;
        let pressed = signal(false);
        let press_down = pressed;
        let press_up = pressed;
        let scaled = pressed;
        let reduced = super::prefs::prefs().reduced;
        let disabled = self.disabled;
        let blocked: Rc<dyn Fn() -> bool> = Rc::new(move || disabled.as_ref().is_some_and(|f| f()));
        let blocked_for_disabled = blocked.clone();
        let blocked_for_press = blocked.clone();

        let mut el = base_button()
            .disabled(move || blocked_for_disabled())
            .on_press_down(move || press_down.set(true))
            .on_press_up(move || press_up.set(false))
            .size(px(size))
            .items_center()
            .justify_center()
            .rounded(move || theme().radius)
            .cursor_pointer()
            .transition_colors()
            .duration(move || motion::motion_duration(motion::FAST_MS, reduced.get()))
            .text_color(move || theme().foreground)
            .hover(|s| {
                s.bg(move || theme().accent)
                    .text_color(move || theme().accent_foreground)
            })
            .active(|s| {
                s.bg(move || {
                    theme()
                        .color("accent-active")
                        .unwrap_or_else(|| theme().accent)
                })
                .text_color(move || {
                    theme()
                        .color("accent-active-foreground")
                        .unwrap_or_else(|| theme().accent_foreground)
                })
            })
            .disabled_style(|s| s.opacity(0.5))
            .focus_visible(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
            .scale(move || {
                if scaled.get() && !reduced.get() {
                    PRESS_SCALE_ICON
                } else {
                    1.0
                }
            })
            .transition_transform()
            .duration(move || motion::motion_duration(motion::FAST_MS, reduced.get()))
            .accessibility_label(label);
        if let Some(on_press) = self.on_press {
            el = el.on_press(move || {
                if !blocked_for_press() {
                    on_press();
                }
            });
        }
        let content = self.content.unwrap_or_else(|| view().into_element());
        let content = match self.test_id {
            Some(id) => view().test_id(id).child(content).into_element(),
            None => content,
        };
        el.child(content).into_element()
    }
}
