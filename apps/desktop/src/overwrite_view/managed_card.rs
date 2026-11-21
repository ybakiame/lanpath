//! 跨核心受管入站端口与控制 API 卡片（持久化在 `app.toml` `[client]`）。
//!
//! 明确说明 `inbounds`（仅限 127.0.0.1 mixed）与 `experimental.clash_api` 由应用
//! 受管注入，供 sing-box / Mihomo / 系统代理守护 / 诊断共用，避免切片改坏端口契约。

use nsh_base::theme;
use nsh_core::prelude::*;

use super::state::OverwriteState;
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::icon::{Icon, Tone, icon};
use crate::ui::input::{field, input};
use crate::ui::layout::{SurfaceLevel, section_title, surface};
use crate::ui::select::switch_labeled;

pub(super) fn managed_bindings_card(state: OverwriteState) -> impl IntoElement {
    let mixed_port = state.mixed_port;
    let clash_enabled = state.clash_api_enabled;
    let clash_port = state.clash_api_port;
    let clash_secret = state.clash_api_secret;
    let save_state = state.clone();
    let dirty_state = state;

    surface(SurfaceLevel::Card)
        .p_5()
        .flex_col()
        .gap_4()
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(icon(Icon::Port, Tone::Primary))
                        .child(section_title(|| t!("overwrite.bindings.title"))),
                )
                .child(crate::ui::badge(crate::ui::BadgeVariant::Outline, || {
                    t!("overwrite.bindings.scope_badge")
                })),
        )
        .child(
            text(|| t!("overwrite.bindings.hint"))
                .text_size(px(12.5))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_start()
                .gap_4()
                .child(
                    view().w(px(176.0)).child(
                        field()
                            .label(|| t!("overwrite.bindings.mixed_port"))
                            .control(
                                input(mixed_port)
                                    .test_id("overwrite-mixed-port")
                                    .accessible_name(|| t!("overwrite.bindings.mixed_port"))
                                    .placeholder(|| "17899".into())
                                    .on_change(move |v| mixed_port.set(v)),
                            ),
                    ),
                )
                .child(
                    view()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            text(|| t!("overwrite.bindings.clash_api_enabled"))
                                .text_size(px(12.0))
                                .text_color(move || theme().foreground),
                        )
                        .child(
                            view()
                                .h(px(36.0))
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .test_id("overwrite-clash-api-enabled")
                                .child(switch_labeled(
                                    move || clash_enabled.get(),
                                    || t!("overwrite.bindings.clash_api_enabled").into(),
                                    move |enabled| clash_enabled.set(enabled),
                                )),
                        ),
                )
                .child(
                    view().w(px(176.0)).child(
                        field()
                            .label(|| t!("overwrite.bindings.clash_api_port"))
                            .control(
                                input(clash_port)
                                    .test_id("overwrite-clash-api-port")
                                    .accessible_name(|| t!("overwrite.bindings.clash_api_port"))
                                    .placeholder(|| "19090".into())
                                    .disabled(move || !clash_enabled.get())
                                    .on_change(move |v| clash_port.set(v)),
                            ),
                    ),
                )
                .child(
                    view().flex_1().min_w(px(200.0)).child(
                        field()
                            .label(|| t!("overwrite.bindings.clash_api_secret"))
                            .control(
                                input(clash_secret)
                                    .test_id("overwrite-clash-api-secret")
                                    .accessible_name(|| t!("overwrite.bindings.clash_api_secret"))
                                    .placeholder(|| t!("overwrite.bindings.secret_placeholder"))
                                    .masked()
                                    .disabled(move || !clash_enabled.get())
                                    .on_change(move |v| clash_secret.set(v)),
                            ),
                    ),
                ),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_3()
                .child(
                    button(ButtonVariant::Outline)
                        .size(ButtonSize::Md)
                        .test_id("overwrite-bindings-save")
                        .label(|| t!("overwrite.bindings.save"))
                        .on_press(move || save_state.save_bindings()),
                )
                .child(when(
                    move || dirty_state.is_bindings_dirty(),
                    || {
                        crate::ui::badge(crate::ui::BadgeVariant::Warning, || {
                            t!("navigation.dirty_title")
                        })
                    },
                )),
        )
}
