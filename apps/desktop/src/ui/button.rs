//! 语义按钮：`nsh_base::button`（行为原语）的薄封装，外观完全由语义
//! token 驱动——功能页只传 variant/size/label/状态与事件，不传颜色、
//! 圆角或字体。状态覆盖 normal/hover/pressed/focus-visible/disabled/
//! loading；press 缩放 0.97（图标按钮 0.95），时长 fast token，减少
//! 动效时过渡为 0ms，立即可读。

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

/// 按钮尺寸（DESIGN.md 4.4：标准 32、工具 28）。`Content` 为内容
/// 自适应高度（卡片式前缀槽等复合内容使用，不裁剪子内容）。
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

/// 变体的 normal 表面（底色/文字/是否描边）。
struct Surface {
    bg: fn(&nsh_base::Theme) -> AdaptiveColor,
    fg: fn(&nsh_base::Theme) -> AdaptiveColor,
    bordered: bool,
}

fn surface(variant: ButtonVariant) -> Surface {
    match variant {
        ButtonVariant::Primary => Surface {
            bg: |t| t.primary,
            fg: |t| t.primary_foreground,
            bordered: false,
        },
        ButtonVariant::Secondary => Surface {
            bg: |t| t.secondary,
            fg: |t| t.secondary_foreground,
            bordered: false,
        },
        ButtonVariant::Outline => Surface {
            bg: |_| transparent_black().into(),
            fg: |t| t.foreground,
            bordered: true,
        },
        ButtonVariant::Ghost => Surface {
            bg: |_| transparent_black().into(),
            fg: |t| t.foreground,
            bordered: false,
        },
        ButtonVariant::Destructive => Surface {
            bg: |t| t.destructive,
            fg: |t| t.color("destructive-foreground").unwrap_or(t.foreground),
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
    prefix: Option<AnyElement>,
    on_press: Option<Box<dyn Fn()>>,
    test_id: Option<&'static str>,
    accessibility_label: Option<SharedString>,
}

/// 一个语义按钮。
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

    /// 前缀内容槽（图标）。
    pub fn prefix(mut self, prefix: impl IntoElement) -> Self {
        self.prefix = Some(prefix.into_element());
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
}

impl IntoElement for Button {
    fn into_element(self) -> AnyElement {
        let surface = surface(self.variant);
        let size = self.size;
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
        if let Some(height) = size.height() {
            el = el.h(px(height));
        }
        let mut el = el
            .px(px(size.pad_x()))
            .items_center()
            .gap_2()
            .rounded(move || theme().radius)
            .cursor_pointer()
            .transition_colors()
            .duration(move || motion::motion_duration(motion::FAST_MS, reduced.get()))
            .bg(move || (surface.bg)(&theme()))
            .text_color(move || (surface.fg)(&theme()))
            .hover(|s| {
                let s = s.bg(move || {
                    theme()
                        .accent
                        .opacity(if surface.bordered { 1.0 } else { 0.5 })
                });
                if surface.bordered {
                    s.text_color(move || theme().accent_foreground)
                } else {
                    s
                }
            })
            .active(|s| s.opacity(0.85))
            .disabled_style(|s| s.opacity(0.5))
            .focus_visible(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
            .scale(move || if scaled.get() { PRESS_SCALE } else { 1.0 })
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

        let mut children: Vec<AnyElement> = Vec::new();
        if loading.as_ref().as_ref().is_some_and(|f| f()) {
            children.push(super::feedback::spinner().into_element());
        } else if let Some(prefix) = self.prefix {
            children.push(prefix.into_element());
        }
        if let Some(label) = self.label {
            children.push(
                text(label)
                    .text_size(px(size.text()))
                    .font_weight(FontWeight::MEDIUM)
                    .into_element(),
            );
        }
        let el = el.children(children).into_element();
        match self.test_id {
            Some(id) => view().test_id(id).child(el).into_element(),
            None => el,
        }
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

/// 一个图标按钮。
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
            .hover(|s| s.bg(move || theme().accent))
            .active(|s| s.opacity(0.85))
            .disabled_style(|s| s.opacity(0.5))
            .focus_visible(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
            .scale(move || if scaled.get() { PRESS_SCALE_ICON } else { 1.0 })
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
        let el = el
            .child(self.content.unwrap_or_else(|| view().into_element()))
            .into_element();
        match self.test_id {
            Some(id) => view().test_id(id).child(el).into_element(),
            None => el,
        }
    }
}
