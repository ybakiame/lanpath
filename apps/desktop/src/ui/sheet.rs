//! 保留页面上下文的原生 Sheet；滚动归内容所有，头尾不随内容移动。
use nsh_base::{SheetEvent, sheet as base_sheet, theme};
use nsh_core::prelude::*;
use std::rc::Rc;

pub fn sheet(
    width: f32,
    title: impl Fn() -> String + 'static,
    open: impl Fn() -> bool + 'static,
    on_request_close: impl Fn() + 'static,
    on_closed: impl Fn() + 'static,
    content: impl IntoElement,
) -> AnyElement {
    let title = Rc::new(title);
    let heading = title.clone();
    let close: Rc<dyn Fn()> = Rc::new(on_request_close);
    let dismiss = close.clone();
    base_sheet()
        .open(move || Some(open()))
        .side(Placement::Right)
        .accessibility_label(move || Some(title().into()))
        .on_open_change(move |event: &SheetEvent| {
            if matches!(event, SheetEvent::OpenChange(false, _)) {
                dismiss();
            }
        })
        .on_close(on_closed)
        .backdrop_style(|s| {
            s.bg(move || theme().color("modal-scrim").unwrap_or(theme().background))
        })
        .popup_style(move |s| {
            s.w(move || px((window().width.0 - 56.0).clamp(280.0, width)))
                .top(px(super::title_bar::TITLE_BAR_HEIGHT))
                .h(move || px((window().height.0 - super::title_bar::TITLE_BAR_HEIGHT).max(0.0)))
                .border_l(px(1.0))
                .border_color(move || theme().border)
                .bg(move || theme().popover)
                .text_color(move || theme().popover_foreground)
                .shadow_xl()
        })
        .child(
            view()
                .size_full()
                .flex_col()
                .child(
                    view()
                        .test_id("workspace-sheet-header")
                        .flex_none()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .px_6()
                        .py_4()
                        .border_b(px(1.0))
                        .border_color(move || theme().border)
                        .child(
                            text(move || heading())
                                .role(AccessibilityRole::Header)
                                .text_size(px(20.0))
                                .font_weight(FontWeight::MEDIUM)
                                .flex_1(),
                        )
                        .child(
                            super::button(super::ButtonVariant::Ghost)
                                .test_id("workspace-sheet-close")
                                .label(|| t!("common.close"))
                                .on_press(move || close()),
                        ),
                )
                .child(view().flex_1().min_h(px(0.0)).p_6().child(content)),
        )
        .into_element()
}
