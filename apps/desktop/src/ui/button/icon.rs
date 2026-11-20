use super::*;

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
        let reduced = crate::ui::prefs::prefs().reduced;
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
            .cursor(Cursor::Default)
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
