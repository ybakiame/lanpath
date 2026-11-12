//! 配置页：共享合成预览。用户点击后在后台线程按已保存配置快照合成完整
//! 配置（sing-box 为 pretty JSON，Mihomo 为 YAML），页面以只读滚动区域
//! 展示正文、生成警告与真实错误。明确文案：共享合成预览，未进行核心
//! 二进制校验——绝不显示“核心已验证/已启动”。本页不下载/安装/校验核心、
//! 不启动核心/系统代理/TUN；Mihomo 现阶段支持订阅配置合成运行，不做
//! Mihomo 结构化切片编辑。
//!
//! 任务模型：busy 单飞（在途预览未完成时新的点击不启动，完成后可再生成）；
//! 合成在 `background`（tokio 线程）执行，Signal 留在 UI 线程 `spawn` 任务
//! 内回写，不跨 Send。新预览进行中保留旧预览并明确 busy；失败保留旧预览
//! 并显示真实错误，不伪报新成功。离开页面由 owner 取消在途任务，重进页面
//! 是全新状态，不会永久 busy。

use nsh_base::{button, theme};
use nsh_core::prelude::*;

use crate::config_composer::{ConfigComposer, ConfigPreview};

const BUSY_COMPOSING: &str = "正在合成配置预览…";

/// 页状态：全部为句柄（Signal/Stored），克隆廉价。
#[derive(Clone)]
struct ConfigPageState {
    service: Option<ConfigComposer>,
    /// 最近一次成功合成的预览；进行中/失败时保留旧值不伪造。
    preview: Signal<Option<ConfigPreview>>,
    busy: Signal<bool>,
    error: Signal<Option<String>>,
    task: Stored<Option<Spawned<()>>>,
}

impl ConfigPageState {
    fn new() -> Self {
        match ConfigComposer::open() {
            Ok(service) => Self {
                service: Some(service),
                preview: signal(None),
                busy: signal(false),
                error: signal(None),
                task: stored(None),
            },
            Err(error) => Self {
                service: None,
                preview: signal(None),
                busy: signal(false),
                error: signal(Some(format!("打开数据根失败:{error}"))),
                task: stored(None),
            },
        }
    }

    /// 开始一次预览；全局单飞（busy 时新的点击不启动，完成后可再生成）。
    fn begin_compose(&self) -> bool {
        if self.busy.get() {
            return false;
        }
        self.busy.set(true);
        // 新预览开始：保留旧预览，只清掉上一次的错误反馈。
        self.error.set(None);
        true
    }

    fn finish_compose(&self) {
        self.busy.set(false);
    }
}

pub(crate) fn config_page() -> impl IntoElement {
    let state = ConfigPageState::new();
    let button_state = state.clone();
    let busy_state = state.clone();
    let summary_state = state.clone();
    let warnings_state = state.clone();
    let error_state = state;
    view()
        .flex_1()
        .flex_col()
        .gap_4()
        .child(text("配置").text_size(px(24.0)))
        .child(
            text("共享合成预览，未进行核心二进制校验 · 按当前生效订阅与已保存设置合成 sing-box JSON / Mihomo YAML · 不启动核心/系统代理/TUN")
                .text_color(move || theme().muted_foreground),
        )
        .child(
            view()
                .flex_row()
                .gap_3()
                .items_center()
                .child(
                    view().test_id("config-compose").child(
                        button()
                            .px_4()
                            .py_2()
                            .rounded(move || theme().radius)
                            .bg(move || theme().primary)
                            .on_press(move || {
                                spawn_compose(button_state.clone());
                            })
                            .child(
                                text("生成配置预览")
                                    .text_color(move || theme().primary_foreground),
                            ),
                    ),
                )
                .child(
                    text(move || if busy_state.busy.get() {
                        BUSY_COMPOSING
                    } else {
                        ""
                    })
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground),
                ),
        )
        .child(
            text(move || {
                summary_state
                    .preview
                    .get()
                    .map(|preview| {
                        format!(
                            "上次成功生成：{} · {} · {} 个节点 · {} 条警告 · 缺失规则集 {} 个",
                            preview.core_type(),
                            preview.format().label(),
                            preview.node_count(),
                            preview.warnings().len(),
                            preview.missing_tags().len(),
                        )
                    })
                    .unwrap_or_else(|| "尚未生成配置预览".to_owned())
            })
            .text_size(px(12.0))
            .text_color(move || theme().muted_foreground),
        )
        .child(
            view().flex_col().gap_1().child(text(move || {
                warnings_state
                    .preview
                    .get()
                    .map(|preview| {
                        preview
                            .warnings()
                            .iter()
                            .map(|warning| format!("警告:{warning}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default()
            }))
            .text_size(px(12.0))
            .text_color(move || theme().destructive),
        )
        .child(
            text(move || error_state.error.get().unwrap_or_default())
                .text_color(move || theme().destructive),
        )
        .child(
            scroll()
                .test_id("config-preview-scroll")
                .flex_1()
                .min_h(px(200.0))
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().border)
                .p_3()
                .child(
                    text(move || {
                        error_state
                            .preview
                            .get()
                            .map(|preview| preview.content().to_owned())
                            .unwrap_or_else(|| "点击「生成配置预览」查看完整 sing-box JSON / Mihomo YAML。".to_owned())
                    })
                    .selectable(true)
                    .text_size(px(12.0))
                    .text_color(move || theme().foreground),
                ),
        )
}

/// 启动后台合成：busy 单飞；合成在 tokio 线程执行，结果回到 UI 线程写
/// Signal（不跨 Send）。成功张贴新预览；失败保留旧预览并显示真实错误。
fn spawn_compose(state: ConfigPageState) {
    if !state.begin_compose() {
        return;
    }
    let Some(service) = state.service.clone() else {
        state.finish_compose();
        state.error.set(Some("打开数据根失败".to_owned()));
        return;
    };
    let done = state.clone();
    state.task.with(|slot| {
        slot.replace(spawn(async move {
            let result = background(async move { service.compose().await }).await;
            let result = match result {
                Ok(result) => result,
                Err(error) => Err(lanpath_common::LanpathError::Client(format!(
                    "合成任务中断:{error}"
                ))),
            };
            done.finish_compose();
            match result {
                Ok(preview) => {
                    done.error.set(None);
                    done.preview.set(Some(preview));
                }
                Err(error) => {
                    // 失败保留旧预览，只显示真实错误，不伪报新成功。
                    done.error.set(Some(format!("生成配置预览失败:{error}")));
                }
            }
        }));
    });
}
