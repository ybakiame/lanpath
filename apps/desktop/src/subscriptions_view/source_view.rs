//! 订阅原文弹窗（阶段 1 语义组件迁移）：只读展示最后一次成功刷新的
//! 原文（共享持久化，重开页面仍可读）；从未成功刷新过时明确提示先刷新，
//! 不伪造内容。

use lanpath_client::Subscription;
use nsh_base::{dialog, theme};
use nsh_core::prelude::*;

use crate::ui::button::{ButtonSize, ButtonVariant, button};

use super::{SourceView, SubscriptionPageState};

/// 打开原文弹窗；没有已保存原文时明确提示“请先刷新”。
pub(super) fn open_source(state: &SubscriptionPageState, subscription: &Subscription) {
    let Some(service) = state.service.clone() else {
        state.post_error(t!("subscription.root_unavailable"));
        return;
    };
    match service.source(subscription.id) {
        Ok(Some(source)) => {
            state.bump_serial();
            state.source_view.set(Some(SourceView {
                name: subscription.name.clone(),
                source,
            }));
        }
        Ok(None) => {
            state.bump_serial();
            state.post_notice(t!("subscription.source_missing", name = subscription.name));
        }
        Err(error) => {
            state.bump_serial();
            state.post_error(t!(
                "subscription.source_load_failed",
                error = error.to_string()
            ));
        }
    }
}

pub(super) fn source_dialog(state: SubscriptionPageState) -> AnyElement {
    let Some(open) = state.source_view.get() else {
        return view().into_element();
    };
    let close_state = state.clone();
    let content = open.source.content.clone();
    dialog()
        .on_cancel(move || close_state.source_view.set(None))
        .child(
            view()
                .flex_col()
                .gap_2()
                .w(px(560.0))
                .p_4()
                .rounded(move || theme().radius)
                .bg(move || theme().card)
                .border(px(1.0))
                .border_color(move || theme().border)
                .child(text(t!("subscription.source_title", name = open.name)).text_size(px(15.0)))
                .child(
                    text(t!(
                        "subscription.source_format",
                        format = format!("{:?}", open.source.format)
                    ))
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground),
                )
                .child(
                    scroll().max_h(px(320.0)).child(
                        text(content)
                            .selectable(true)
                            .text_size(px(12.0))
                            .text_color(move || theme().foreground),
                    ),
                )
                .child(
                    view().flex_row().justify_end().child(
                        button(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .label(|| t!("common.close"))
                            .on_press(move || state.source_view.set(None)),
                    ),
                ),
        )
        .into_element()
}
