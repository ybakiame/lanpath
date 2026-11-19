//! 浮层组件：确认对话框、Tooltip 的语义薄封装。
//! Dialog/Menu/Popover 直接用 `nsh_base` 原语组合（见画廊）；确认
//! 对话框是业务页共用的危险/未保存确认入口。

use std::cell::RefCell;
use std::rc::Rc;

use nsh_base::{dialog, theme, tooltip as base_tooltip};
use nsh_core::prelude::*;

use super::motion;

/// 确认回调存储（`FnMut` 经 `Rc<RefCell<..>>` 共享给惰性 `when` 分支）。
type ConfirmHandler = Rc<RefCell<Option<Box<dyn FnMut()>>>>;

/// 确认对话框（危险操作与未保存修改确认的统一入口）。
pub struct ConfirmDialog {
    open: Rc<dyn Fn() -> bool>,
    title: Rc<dyn Fn() -> String>,
    description: Rc<dyn Fn() -> String>,
    confirm_label: Rc<dyn Fn() -> String>,
    cancel_label: Rc<dyn Fn() -> String>,
    destructive: bool,
    on_confirm: ConfirmHandler,
    on_cancel: ConfirmHandler,
    test_id: Option<&'static str>,
}

/// 一个确认对话框。
pub fn confirm_dialog(open: impl Fn() -> bool + 'static) -> ConfirmDialog {
    ConfirmDialog {
        open: Rc::new(open),
        title: Rc::new(String::new),
        description: Rc::new(String::new),
        confirm_label: Rc::new(|| t!("common.confirm")),
        cancel_label: Rc::new(|| t!("common.cancel")),
        destructive: false,
        on_confirm: ConfirmHandler::default(),
        on_cancel: ConfirmHandler::default(),
        test_id: None,
    }
}

impl ConfirmDialog {
    pub fn title(mut self, title: impl Fn() -> String + 'static) -> Self {
        self.title = Rc::new(title);
        self
    }

    pub fn description(mut self, description: impl Fn() -> String + 'static) -> Self {
        self.description = Rc::new(description);
        self
    }

    pub fn confirm_label(mut self, label: impl Fn() -> String + 'static) -> Self {
        self.confirm_label = Rc::new(label);
        self
    }

    pub fn cancel_label(mut self, label: impl Fn() -> String + 'static) -> Self {
        self.cancel_label = Rc::new(label);
        self
    }

    /// 危险确认：确认按钮 destructive 语义色。
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    pub fn on_confirm(self, on_confirm: impl FnMut() + 'static) -> Self {
        *self.on_confirm.borrow_mut() = Some(Box::new(on_confirm));
        self
    }

    /// 取消回调：取消按钮、Escape 与点遮罩关闭时触发（弹层关闭后焦点
    /// 回到触发控件由 NSH dialog 原生负责，不自造）。
    pub fn on_cancel(self, on_cancel: impl FnMut() + 'static) -> Self {
        *self.on_cancel.borrow_mut() = Some(Box::new(on_cancel));
        self
    }

    pub fn test_id(mut self, id: &'static str) -> Self {
        self.test_id = Some(id);
        self
    }
}

impl IntoElement for ConfirmDialog {
    fn into_element(self) -> AnyElement {
        let open = self.open;
        let destructive = self.destructive;
        let test_id = self.test_id;
        let on_confirm = self.on_confirm;
        let on_cancel_dialog = self.on_cancel.clone();
        let on_cancel_button = self.on_cancel;
        let title = self.title;
        let description = self.description;
        let confirm_label = self.confirm_label;
        let cancel_label = self.cancel_label;
        when(
            move || open(),
            move || {
                let confirm_label = confirm_label.clone();
                let on_confirm = on_confirm.clone();
                let on_cancel_button = on_cancel_button.clone();
                let on_cancel_dialog = on_cancel_dialog.clone();
                let cancel_label = cancel_label.clone();
                let title = title.clone();
                let description = description.clone();
                let confirm = super::button::button(if destructive {
                    super::button::ButtonVariant::Destructive
                } else {
                    super::button::ButtonVariant::Primary
                })
                .size(super::button::ButtonSize::Md)
                .label(move || confirm_label())
                .on_press(move || {
                    if let Some(on_confirm) = on_confirm.borrow_mut().as_mut() {
                        on_confirm();
                    }
                });
                let mut card = view()
                    .flex_col()
                    .gap_3()
                    .w(px(420.0))
                    .p_4()
                    .rounded(move || theme().radius)
                    .bg(move || theme().card)
                    .border(px(1.0))
                    .border_color(move || theme().border)
                    .shadow_lg()
                    .child(
                        text(move || title())
                            .text_size(px(17.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(move || theme().foreground),
                    )
                    .child(
                        text(move || description())
                            .text_size(px(13.0))
                            .text_color(move || theme().muted_foreground),
                    )
                    .child(
                        view()
                            .flex_row()
                            .gap_2()
                            .justify_end()
                            .child(
                                super::button::button(super::button::ButtonVariant::Outline)
                                    .size(super::button::ButtonSize::Md)
                                    .label(move || cancel_label())
                                    .on_press(move || {
                                        if let Some(on_cancel) =
                                            on_cancel_button.borrow_mut().as_mut()
                                        {
                                            on_cancel();
                                        }
                                    }),
                            )
                            .child(confirm),
                    );
                if let Some(id) = test_id {
                    card = card.test_id(id);
                }
                dialog()
                    .role(AccessibilityRole::AlertDialog)
                    .on_cancel(move || {
                        if let Some(on_cancel) = on_cancel_dialog.borrow_mut().as_mut() {
                            on_cancel();
                        }
                    })
                    .child(
                        view()
                            .flex_1()
                            .items_center()
                            .justify_center()
                            .bg(transparent_black().opacity(0.4))
                            .child(card),
                    )
            },
        )
        .into_element()
    }
}

/// Tooltip：内容 token 化；进入位移 4pt，减少动效时原地出现。
pub fn tooltip(trigger: impl Interactive, content: impl Fn() -> String + 'static) -> AnyElement {
    let reduced = super::prefs::prefs().reduced;
    view()
        .child(trigger.into_element())
        .tooltip(
            base_tooltip()
                .starting_style(move |s| {
                    s.opacity(0.0).translate_y(px(if reduced.get() {
                        0.0
                    } else {
                        motion::DISTANCE_LABEL_PT
                    }))
                })
                .child(
                    view()
                        .px(px(8.0))
                        .py(px(4.0))
                        .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
                        .bg(move || theme().popover)
                        .text_color(move || theme().popover_foreground)
                        .shadow_md()
                        .child(text(content).text_size(px(12.0))),
                ),
        )
        .into_element()
}

/// 居中的编辑弹层：共用遮罩、焦点、进入动效与 Escape 行为。
pub fn editor_dialog(
    title: impl Fn() -> String + 'static,
    on_cancel: impl Fn() + 'static,
    content: impl IntoElement,
) -> AnyElement {
    let mut card = view().child(content);
    if !super::prefs::prefs().reduced.get() {
        let enter = nsh_core::prelude::motion().entering(
            MotionSpec::new(KeyframeTiming::new(motion::SLOW_MS).easing(Easing::EaseOut))
                .track(
                    MotionChannel::Opacity,
                    [Keyframe::new(0.0), Keyframe::new(1.0)],
                )
                .track(
                    MotionChannel::TranslateY,
                    [
                        Keyframe::new(motion::DISTANCE_DIALOG_PT),
                        Keyframe::new(0.0),
                    ],
                ),
        );
        card = card.bind_motion(&enter);
    }
    dialog()
        .label(move || SharedString::from(title()))
        .on_cancel(on_cancel)
        .child(
            view()
                .flex_1()
                .items_center()
                .justify_center()
                .bg(transparent_black().opacity(0.4))
                .child(card),
        )
        .into_element()
}
