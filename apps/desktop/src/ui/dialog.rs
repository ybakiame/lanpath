//! 固定头尾的原生表单 Dialog；遮罩占满窗口，内容尺寸不取决于父布局。
use nsh_base::{dialog as base_dialog, theme};
use nsh_core::prelude::*;
use std::rc::Rc;

#[derive(Clone, Copy)]
pub enum DialogSize {
    Form,
    Code,
}
pub fn form_dialog(
    title: impl Fn() -> String + 'static,
    size: DialogSize,
    initial_focus: ElementRef,
    on_cancel: impl Fn() + 'static,
    body: impl IntoElement,
    footer: impl IntoElement,
) -> AnyElement {
    let title = Rc::new(title);
    let label = title.clone();
    let cancel: Rc<dyn Fn()> = Rc::new(on_cancel);
    let dismiss = cancel.clone();
    let reduced = super::prefs().reduced;
    let width = match size {
        DialogSize::Form => 560.0,
        DialogSize::Code => 900.0,
    };
    let height = match size {
        DialogSize::Form => 650.0,
        DialogSize::Code => 740.0,
    };
    let enter = motion().entering(
        MotionSpec::new(
            KeyframeTiming::new(super::motion::motion_duration(
                super::motion::BASE_MS,
                reduced.get(),
            ))
            .easing(Easing::EaseOut),
        )
        .values(MotionChannel::Opacity, [0.0, 1.0])
        .values(
            MotionChannel::Scale,
            [if reduced.get() { 1.0 } else { 0.97 }, 1.0],
        ),
    );
    base_dialog()
        .flex_1()
        .animation(ModalAnimation::None)
        .close_on_escape(true)
        .label(move || SharedString::from(label()))
        .on_open(move || initial_focus.focus())
        .on_cancel(move || dismiss())
        .child(
            view()
                .size_full()
                .flex_1()
                .items_center()
                .justify_center()
                .p_6()
                .bg(move || theme().color("modal-scrim").unwrap_or(theme().background))
                .child(
                    view()
                        .test_id("form-dialog-panel")
                        .w(move || px((window().width.0 - 64.0).min(width).max(280.0)))
                        .h(move || px((window().height.0 - 80.0).min(height).max(280.0)))
                        .flex_col()
                        .rounded(px(8.0))
                        .border(px(1.0))
                        .border_color(move || theme().border)
                        .bg(move || theme().popover)
                        .text_color(move || theme().popover_foreground)
                        .shadow_xl()
                        .bind_motion(&enter)
                        .child(
                            view()
                                .flex_row()
                                .flex_none()
                                .items_center()
                                .gap_3()
                                .px_6()
                                .py_4()
                                .border_b(px(1.0))
                                .border_color(move || theme().border)
                                .child(
                                    text(move || title())
                                        .role(AccessibilityRole::Header)
                                        .text_size(px(20.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .flex_1()
                                        .min_w(px(0.0)),
                                )
                                .child(
                                    super::button(super::ButtonVariant::Ghost)
                                        .size(super::ButtonSize::Sm)
                                        .label(|| t!("common.close"))
                                        .on_press(move || cancel()),
                                ),
                        )
                        .child(view().flex_1().min_h(px(0.0)).p_6().child(body))
                        .child(
                            view()
                                .flex_none()
                                .px_6()
                                .py_4()
                                .border_t(px(1.0))
                                .border_color(move || theme().border)
                                .child(footer),
                        ),
                ),
        )
        .into_element()
}
