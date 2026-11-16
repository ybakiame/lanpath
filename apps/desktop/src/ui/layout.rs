//! 布局与结构组件：Surface、Separator、ListItem、Toolbar、分区标题。

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::motion;

/// Surface 层级。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceLevel {
    /// 页面背景层。
    Background,
    /// 卡片层（默认）。
    Card,
    /// 浮起层（弹层底色）。
    Popover,
}

/// 一个表面：底色/圆角/边框来自 token。
pub fn surface(level: SurfaceLevel) -> View {
    let bg = move || match level {
        SurfaceLevel::Background => theme().background,
        SurfaceLevel::Card => theme().card,
        SurfaceLevel::Popover => theme().popover,
    };
    view()
        .rounded(move || theme().radius)
        .bg(bg)
        .border(px(1.0))
        .border_color(move || theme().border)
}

/// 分隔线。
pub fn separator() -> AnyElement {
    view()
        .h(px(1.0))
        .w_full()
        .bg(move || theme().border)
        .into_element()
}

/// 列表行：高 36，hover 洗涤（instant token），选中填充，可选尾部槽。
pub struct ListItem {
    title: Box<dyn Fn() -> String>,
    subtitle: Option<Box<dyn Fn() -> String>>,
    selected: Option<Box<dyn Fn() -> bool>>,
    disabled: Option<Box<dyn Fn() -> bool>>,
    trailing: Option<AnyElement>,
    on_press: Option<Box<dyn Fn()>>,
    test_id: Option<&'static str>,
}

/// 一个列表行。
pub fn list_item(title: impl Fn() -> String + 'static) -> ListItem {
    ListItem {
        title: Box::new(title),
        subtitle: None,
        selected: None,
        disabled: None,
        trailing: None,
        on_press: None,
        test_id: None,
    }
}

impl ListItem {
    pub fn subtitle(mut self, subtitle: impl Fn() -> String + 'static) -> Self {
        self.subtitle = Some(Box::new(subtitle));
        self
    }

    pub fn selected(mut self, selected: impl Fn() -> bool + 'static) -> Self {
        self.selected = Some(Box::new(selected));
        self
    }

    pub fn disabled(mut self, disabled: impl Fn() -> bool + 'static) -> Self {
        self.disabled = Some(Box::new(disabled));
        self
    }

    /// 尾部槽（按钮、徽章、菜单）。
    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_element());
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
}

impl IntoElement for ListItem {
    fn into_element(self) -> AnyElement {
        let reduced = super::prefs::prefs().reduced;
        let disabled = self.disabled;
        let blocked: Rc<dyn Fn() -> bool> = Rc::new(move || disabled.as_ref().is_some_and(|f| f()));
        let blocked_for_disabled = blocked.clone();
        let blocked_for_press = blocked.clone();
        let selected = self.selected;
        let selected_state: Rc<dyn Fn() -> bool> =
            Rc::new(move || selected.as_ref().is_some_and(|f| f()));
        let selected_for_prop = selected_state.clone();
        let selected_for_bg = selected_state.clone();
        let mut el = nsh_base::button()
            .disabled(move || blocked_for_disabled())
            .selected(move || selected_for_prop())
            .w_full()
            .h(px(36.0))
            .px(px(12.0))
            .items_center()
            .gap_3()
            .cursor_pointer()
            .rounded(move || theme().radius)
            .transition_colors()
            .duration(move || motion::motion_duration(motion::INSTANT_MS, reduced.get()))
            .bg(move || {
                if selected_for_bg() {
                    theme().accent
                } else {
                    transparent_black().into()
                }
            })
            .hover(|s| s.bg(move || theme().accent.opacity(0.6)))
            .disabled_style(|s| s.opacity(0.5));
        if let Some(on_press) = self.on_press {
            el = el.on_press(move || {
                if !blocked_for_press() {
                    on_press();
                }
            });
        }
        let mut text_col = view().flex_col().gap_1().min_w(px(0.0)).flex_1();
        let title = self.title;
        text_col = text_col.child(
            text(title)
                .text_size(px(14.0))
                .text_color(move || theme().foreground)
                .truncate(),
        );
        if let Some(subtitle) = self.subtitle {
            text_col = text_col.child(
                text(subtitle)
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground)
                    .truncate(),
            );
        }
        let el = el
            .child(text_col)
            .child(self.trailing.unwrap_or_else(|| view().into_element()))
            .into_element();
        match self.test_id {
            Some(id) => view().test_id(id).child(el).into_element(),
            None => el,
        }
    }
}

/// 工具栏：高 28 的工具按钮横排，分隔符分组。
pub fn toolbar(groups: Vec<Vec<AnyElement>>) -> AnyElement {
    nsh_base::toolbar()
        .items_center()
        .gap_1()
        .children(groups.into_iter().enumerate().flat_map(|(index, group)| {
            let mut parts: Vec<AnyElement> = Vec::new();
            if index > 0 {
                parts.push(
                    nsh_base::toolbar_separator()
                        .my(px(2.0))
                        .h(px(16.0))
                        .w(px(1.0))
                        .bg(move || theme().border)
                        .into_element(),
                );
            }
            parts.extend(group);
            parts
        }))
        .into_element()
}

/// 分区标题（页面内 20pt 节标题）。
pub fn section_title(title: impl Fn() -> String + 'static) -> AnyElement {
    text(title)
        .text_size(px(20.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(move || theme().foreground)
        .into_element()
}

/// 页面标题（28pt）+ 副标题槽。
pub fn page_header(
    title: impl Fn() -> String + 'static,
    subtitle: impl Fn() -> String + 'static,
) -> AnyElement {
    view()
        .flex_col()
        .gap_2()
        .child(
            text(title)
                .text_size(px(28.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || theme().foreground),
        )
        .child(
            text(subtitle)
                .text_size(px(14.0))
                .text_color(move || theme().muted_foreground),
        )
        .into_element()
}
