//! 核心页“运行”卡片渲染：真实 running/busy/结果/停止 kind/错误展示与
//! “校验并启动”“停止”按钮。状态视图模型与后台操作在 `super::runtime`。

use nsh_base::{button, theme};
use nsh_core::prelude::*;

use super::CorePageState;
use super::runtime::{spawn_start, spawn_stop};

pub(super) fn runtime_card(state: CorePageState) -> impl IntoElement {
    let status_state = state.clone();
    let start_state = state.clone();
    let stop_state = state.clone();
    let exit_state = state.clone();
    let result_state = state.clone();
    view()
        .flex_col()
        .gap_2()
        .p_3()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(text("运行").text_size(px(15.0)))
        .child(
            text("校验并启动：先以真实核心二进制校验配置（sing-box check / Mihomo -t），再以用户态 loopback mixed + Clash API 启动 · TUN/VPN 不接管；系统代理经独立守护接管/恢复（见下方分区）")
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(text(move || {
            let view = status_state.status.get();
            if let Some(operation) = view.operation {
                return match view.stage {
                    Some(stage) => format!("{operation}：{stage}…"),
                    None => format!("{operation}…"),
                };
            }
            match view.summary {
                Some(summary) => format!("运行中：{summary}"),
                None => "未运行".to_owned(),
            }
        }))
        .child(
            text(move || exit_state.status.get().exit_error.unwrap_or_default())
                .text_size(px(12.0))
                .text_color(move || theme().destructive),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child({
                    let press_state = start_state.clone();
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .bg(move || theme().primary)
                        .on_press(move || spawn_start(press_state.clone()))
                        .child(
                            view().test_id("core-start").child(
                                text("校验并启动")
                                    .text_color(move || theme().primary_foreground),
                            ),
                        )
                })
                .child({
                    let press_state = stop_state.clone();
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .border(px(1.0))
                        .border_color(move || theme().border)
                        .on_press(move || spawn_stop(press_state.clone()))
                        .child(
                            view()
                                .test_id("core-stop")
                                .child(text("停止").text_size(px(12.0))),
                        )
                }),
        )
        .child(
            text(move || {
                let started = result_state.start_result.get().unwrap_or_default();
                let stopped = result_state.stop_result.get().unwrap_or_default();
                match (started.is_empty(), stopped.is_empty()) {
                    (false, _) => started,
                    (_, false) => stopped,
                    _ => String::new(),
                }
            })
            .text_size(px(12.0))
            .text_color(move || theme().primary),
        )
}
