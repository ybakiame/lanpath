//! 语义滑块：`nsh_base::slider` 的薄封装，轨道、进度段与圆形拇指全部由语义
//! token 驱动，支持键盘方向键调节与禁用降对比反馈。

use std::rc::Rc;

use nsh_base::{SliderValue, slider as base_slider, theme};
use nsh_core::prelude::*;

/// 水平数值滑块：拖动触发 `on_change`，松手/按键确认触发 `on_change_end`。
#[allow(clippy::too_many_arguments)]
pub fn slider(
    value: impl Fn() -> f64 + 'static,
    min: f64,
    max: f64,
    step: f64,
    disabled: impl Fn() -> bool + 'static,
    label: impl Fn() -> String + 'static,
    test_id: &'static str,
    on_change: impl Fn(f64) + 'static,
    on_change_end: impl Fn(f64) + 'static,
) -> AnyElement {
    let disabled = Rc::new(disabled);
    let container_disabled = disabled.clone();
    let reduced = super::prefs::prefs().reduced;
    view()
        .test_id(test_id)
        .flex_1()
        .min_w(px(140.0))
        .style_when(move || container_disabled(), |s| s.opacity(0.45))
        .child(
            base_slider()
                .value(move || Some(SliderValue::Single(value())))
                .min(min)
                .max(max)
                .step(step)
                .disabled(move || disabled())
                .accessibility_label(move || Some(SharedString::from(label())))
                .on_change(move |v: &SliderValue| on_change(v.start()))
                .on_change_end(move |v: &SliderValue| on_change_end(v.start()))
                .w_full()
                .track_style(|s| {
                    s.w_full()
                        .h(px(24.0))
                        .justify_center()
                        .cursor(Cursor::Default)
                })
                .indicator_style(|s| {
                    s.w_full()
                        .h(px(6.0))
                        .rounded_full()
                        .bg(move || theme().muted)
                        .border(px(1.0))
                        .border_color(move || theme().border)
                })
                .range_style(|s| s.h_full().rounded_full().bg(move || theme().primary))
                .thumb_style(move |s| {
                    s.size(px(16.0))
                        .rounded_full()
                        .bg(move || theme().card)
                        .border(px(2.0))
                        .border_color(move || theme().primary)
                        .shadow_sm()
                        .transition_transform()
                        .duration(move || {
                            super::motion::motion_duration(super::motion::INSTANT_MS, reduced.get())
                        })
                }),
        )
        .into_element()
}
