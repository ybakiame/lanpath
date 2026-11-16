//! 输入组件：Input/Textarea 的语义封装与 Field（标签 + 控件 + 提示/
//! 错误槽）。外观只来自 token；invalid 态由 `theme().destructive` 驱动。

use std::rc::Rc;

use nsh_base::{input as base_input, theme};
use nsh_core::prelude::*;

/// 单行输入（高 40，DESIGN.md 4.4）。
pub struct Input {
    value: Signal<String>,
    placeholder: Option<Box<dyn Fn() -> String>>,
    on_change: Option<Box<dyn Fn(String)>>,
    invalid: Option<Box<dyn Fn() -> bool>>,
    disabled: Option<Box<dyn Fn() -> bool>>,
    masked: bool,
    test_id: Option<&'static str>,
}

/// 一个单行输入。
pub fn input(value: Signal<String>) -> Input {
    Input {
        value,
        placeholder: None,
        on_change: None,
        invalid: None,
        disabled: None,
        masked: false,
        test_id: None,
    }
}

impl Input {
    pub fn placeholder(mut self, placeholder: impl Fn() -> String + 'static) -> Self {
        self.placeholder = Some(Box::new(placeholder));
        self
    }

    pub fn on_change(mut self, on_change: impl Fn(String) + 'static) -> Self {
        self.on_change = Some(Box::new(on_change));
        self
    }

    /// 无效态： destructive 边框。
    pub fn invalid(mut self, invalid: impl Fn() -> bool + 'static) -> Self {
        self.invalid = Some(Box::new(invalid));
        self
    }

    pub fn disabled(mut self, disabled: impl Fn() -> bool + 'static) -> Self {
        self.disabled = Some(Box::new(disabled));
        self
    }

    /// 掩码输入（口令）。
    pub fn masked(mut self) -> Self {
        self.masked = true;
        self
    }

    pub fn test_id(mut self, id: &'static str) -> Self {
        self.test_id = Some(id);
        self
    }
}

/// 多行输入（Textarea）：`nsh_base` Input 原语的 `multiline` 薄封装，
/// token 化外观，最小高度 80。
pub struct Textarea {
    value: Signal<String>,
    placeholder: Option<Box<dyn Fn() -> String>>,
    on_change: Option<Box<dyn Fn(String)>>,
    invalid: Option<Box<dyn Fn() -> bool>>,
    disabled: Option<Box<dyn Fn() -> bool>>,
}

/// 一个多行输入。
pub fn textarea(value: Signal<String>) -> Textarea {
    Textarea {
        value,
        placeholder: None,
        on_change: None,
        invalid: None,
        disabled: None,
    }
}

impl Textarea {
    pub fn placeholder(mut self, placeholder: impl Fn() -> String + 'static) -> Self {
        self.placeholder = Some(Box::new(placeholder));
        self
    }

    pub fn on_change(mut self, on_change: impl Fn(String) + 'static) -> Self {
        self.on_change = Some(Box::new(on_change));
        self
    }

    /// 无效态：destructive 边框。
    pub fn invalid(mut self, invalid: impl Fn() -> bool + 'static) -> Self {
        self.invalid = Some(Box::new(invalid));
        self
    }

    pub fn disabled(mut self, disabled: impl Fn() -> bool + 'static) -> Self {
        self.disabled = Some(Box::new(disabled));
        self
    }
}

impl IntoElement for Textarea {
    fn into_element(self) -> AnyElement {
        let value = self.value;
        let mut el = base_input()
            .value(value)
            .multiline(true)
            .min_h(px(80.0))
            .p(px(12.0))
            .rounded(move || theme().radius)
            .border(px(1.0))
            .bg(move || theme().background)
            .text_color(move || theme().foreground)
            .text_size(px(14.0))
            .transition_colors()
            .border_color(move || {
                if self.invalid.as_ref().is_some_and(|f| f()) {
                    theme().destructive
                } else {
                    theme().input
                }
            })
            .focus(|s| s.border_color(move || theme().ring));
        if let Some(placeholder) = self.placeholder {
            el = el.placeholder(placeholder);
        }
        if let Some(on_change) = self.on_change {
            el = el.on_change(move |text: &str| on_change(text.to_owned()));
        }
        if let Some(disabled) = self.disabled {
            el = el.disabled(disabled);
        }
        el.into_element()
    }
}

impl IntoElement for Input {
    fn into_element(self) -> AnyElement {
        let value = self.value;
        let mut el = base_input()
            .value(value)
            .h(px(40.0))
            .px(px(12.0))
            .items_center()
            .rounded(move || theme().radius)
            .border(px(1.0))
            .bg(move || theme().background)
            .text_color(move || theme().foreground)
            .text_size(px(14.0))
            .transition_colors()
            .border_color(move || {
                if self.invalid.as_ref().is_some_and(|f| f()) {
                    theme().destructive
                } else {
                    theme().input
                }
            })
            .focus(|s| s.border_color(move || theme().ring));
        if let Some(placeholder) = self.placeholder {
            el = el.placeholder(placeholder);
        }
        if let Some(on_change) = self.on_change {
            el = el.on_change(move |text: &str| on_change(text.to_owned()));
        }
        if self.masked {
            el = el.masked(true);
        }
        if let Some(disabled) = self.disabled {
            el = el.disabled(disabled);
        }
        if let Some(id) = self.test_id {
            el = el.test_id(id);
        }
        el.into_element()
    }
}

/// 字段容器：标签（可选）+ 控件 + 提示/错误文本槽。错误优先于提示。
pub struct Field {
    label: Option<Box<dyn Fn() -> String>>,
    hint: Option<Box<dyn Fn() -> String>>,
    error: Option<Box<dyn Fn() -> Option<String>>>,
    control: Option<AnyElement>,
}

/// 一个字段（标签 + 控件 + 提示/错误）。
pub fn field() -> Field {
    Field {
        label: None,
        hint: None,
        error: None,
        control: None,
    }
}

impl Field {
    pub fn label(mut self, label: impl Fn() -> String + 'static) -> Self {
        self.label = Some(Box::new(label));
        self
    }

    /// 常态提示（error 存在时被取代）。
    pub fn hint(mut self, hint: impl Fn() -> String + 'static) -> Self {
        self.hint = Some(Box::new(hint));
        self
    }

    /// 错误文本：`Some` 时 destructive 显示并取代 hint。
    pub fn error(mut self, error: impl Fn() -> Option<String> + 'static) -> Self {
        self.error = Some(Box::new(error));
        self
    }

    pub fn control(mut self, control: impl IntoElement) -> Self {
        self.control = Some(control.into_element());
        self
    }
}

impl IntoElement for Field {
    fn into_element(self) -> AnyElement {
        let mut el = view().flex_col().gap_2();
        if let Some(label) = self.label {
            el = el.child(
                text(label)
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(move || theme().foreground),
            );
        }
        if let Some(control) = self.control {
            el = el.child(control);
        }
        let hint = self.hint;
        let error = Rc::new(self.error);
        let error_for_text = error.clone();
        let error_for_color = error.clone();
        el.child(
            text(move || {
                error_for_text
                    .as_ref()
                    .as_ref()
                    .and_then(|f| f())
                    .or_else(|| hint.as_ref().map(|f| f()))
                    .unwrap_or_default()
            })
            .text_size(px(12.0))
            .text_color(move || {
                if error_for_color
                    .as_ref()
                    .as_ref()
                    .and_then(|f| f())
                    .is_some()
                {
                    theme().destructive
                } else {
                    theme().muted_foreground
                }
            }),
        )
        .into_element()
    }
}
