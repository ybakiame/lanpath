//! 备份页：加密备份导出与恢复导入/确认。复用共享 `lanpath-client::backup`
//! 的 age 加密、一次性预览与启动事务（见 `backup` 业务模块）；本页只做
//! 交互与真实状态上屏：
//!
//! - 导出：密码输入恒掩码；至少 12 字符，或一键生成共享强密码；经 NSH
//!   原生保存对话框选定目标后写出真实 `.zip.age`——绝不覆盖已有文件，
//!   失败真实上屏。成功后清空密码输入。
//! - 导入：经原生打开对话框选择文件（16 MiB 上限）；密码输入后解密
//!   预览（版本/时间/订阅数/覆写数/文件数/字节），preview_id 一次性、
//!   300 秒过期；修改文件或密码立即 discard。密码与明文不落日志/storage。
//! - 恢复确认：确认后停止核心（含系统代理守护恢复），再准备加密回滚与
//!   `.backup-pending`；成功标记待重启，一切业务写被拒（业务层门），
//!   重启经 NSH 可靠重启 API（旧进程退出后才拉起新进程），失败提示
//!   手动重启，不伪装已重启。
//! - 启动结果：`.backup-applied.json` 摘要与 `.backup-failed` 真实错误
//!   在此展示；用户确认提示后才 acknowledge 清除标记。

mod export_card;
mod import_card;

use std::sync::Arc;
use std::time::Duration;

use lanpath_client::backup::BackupSummary;
use nsh_base::{button, theme};
use nsh_core::prelude::*;

use crate::backup::Backup;
use crate::core_runtime::CoreRuntime;

/// 预览绑定的展示信息：摘要 + 一次性 id（明文/口令只进共享进程内缓存）。
#[derive(Clone)]
pub(super) struct PreviewInfo {
    pub(super) summary: BackupSummary,
    pub(super) preview_id: String,
}

/// 页状态：全部为句柄（Signal/Arc/Stored），克隆廉价。任务句柄四槽：
/// 导出/导入/准备/预览过期调度各自独立，重复点击只替换未完成任务。
#[derive(Clone)]
pub(super) struct BackupPageState {
    /// 页面级 owner：在页面构建时捕获。恢复准备从确认弹窗发起——弹窗的
    /// 临时 owner 在确认按下后随弹窗关闭被销毁，在其下 spawn 会被立即
    /// 取消；工作任务必须挂在页面 owner 下（随离开页面才取消）。
    pub(super) owner: Owner,
    pub(super) service: Option<Arc<Backup>>,
    pub(super) runtime: Option<Arc<CoreRuntime>>,
    /// 打开数据根或读取恢复结果的真实错误（含 `.backup-failed`）。
    pub(super) startup_error: Signal<Option<String>>,
    /// 上次启动已应用恢复的摘要（待用户确认）。
    pub(super) restored: Signal<Option<BackupSummary>>,
    /// 用户已确认恢复提示（acknowledge 成功）。
    pub(super) acknowledged: Signal<bool>,
    /// 已有待恢复备份（准备成功后或启动时检测到 `.backup-pending`）。
    pub(super) pending: Signal<bool>,
    pub(super) export_password: Signal<String>,
    pub(super) exporting: Signal<bool>,
    pub(super) export_status: Signal<Option<String>>,
    pub(super) import_name: Signal<Option<String>>,
    pub(super) import_bytes: Signal<Option<Arc<Vec<u8>>>>,
    pub(super) import_password: Signal<String>,
    pub(super) preview: Signal<Option<PreviewInfo>>,
    pub(super) importing: Signal<bool>,
    pub(super) import_status: Signal<Option<String>>,
    pub(super) preparing: Signal<bool>,
    pub(super) prepare_status: Signal<Option<String>>,
    pub(super) confirm_restore: Signal<bool>,
    pub(super) export_task: Stored<Option<Spawned<()>>>,
    pub(super) import_task: Stored<Option<Spawned<()>>>,
    pub(super) prepare_task: Stored<Option<Spawned<()>>>,
    pub(super) expiry_task: Stored<Option<Spawned<()>>>,
}

impl BackupPageState {
    fn new() -> Self {
        let service = Backup::open().ok();
        let runtime = CoreRuntime::open().ok();
        let mut startup_error = None;
        let mut restored = None;
        let pending = service.as_ref().is_some_and(|backup| backup.is_pending());
        if let Some(backup) = &service {
            match backup.restored() {
                Ok(Some(summary)) => restored = Some(summary),
                Ok(None) => {}
                Err(error) => startup_error = Some(error.to_string()),
            }
        }
        Self {
            owner: Owner::current(),
            service,
            runtime,
            startup_error: signal(startup_error),
            restored: signal_with(restored, SignalEquals::Never),
            acknowledged: signal(false),
            pending: signal(pending),
            export_password: signal(String::new()),
            exporting: signal(false),
            export_status: signal(None),
            import_name: signal(None),
            import_bytes: signal_with(None, SignalEquals::Never),
            import_password: signal(String::new()),
            preview: signal_with(None, SignalEquals::Never),
            importing: signal(false),
            import_status: signal(None),
            preparing: signal(false),
            prepare_status: signal(None),
            confirm_restore: signal(false),
            export_task: stored(None),
            import_task: stored(None),
            prepare_task: stored(None),
            expiry_task: stored(None),
        }
    }

    fn no_service_error() -> String {
        "打开数据根失败".to_owned()
    }

    /// 放弃当前预览（修改文件/密码、过期、准备消费时调用）：共享缓存
    /// discard 幂等，UI 状态一并清空。
    pub(super) fn drop_preview(&self) {
        if let Some(preview) = self.preview.get() {
            if let Some(service) = &self.service {
                service.discard(&preview.preview_id);
            }
            self.preview.set(None);
        }
    }

    /// 用户已看到恢复提示：确认后才清除 `.backup-applied.json` 标记。
    fn acknowledge(&self) {
        let Some(service) = &self.service else {
            self.startup_error.set(Some(Self::no_service_error()));
            return;
        };
        match service.acknowledge() {
            Ok(()) => self.acknowledged.set(true),
            Err(error) => self
                .startup_error
                .set(Some(format!("清除恢复标记失败:{error}"))),
        }
    }
}

pub(crate) fn backup_page() -> impl IntoElement {
    let state = BackupPageState::new();
    let ack_state = state.clone();
    let restored_state = state.clone();
    let startup_state = state.clone();
    let cards_state = state.clone();
    let banner_state = state.clone();
    view()
        .flex_1()
        .flex_col()
        .gap_4()
        .child(text("备份").text_size(px(24.0)))
        .child(
            text("导出 age 加密备份（.zip.age），或从备份恢复 · 密码只保存在内存，丢失无法找回 · 恢复需重启应用完成")
                .text_color(move || theme().muted_foreground),
        )
        .child(when(
            move || startup_state.startup_error.get().is_some(),
            move || {
                let state = startup_state.clone();
                view()
                    .test_id("backup-startup-error")
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded(move || theme().radius)
                    .border(px(1.0))
                    .border_color(move || theme().destructive)
                    .child(
                        text(move || {
                            state
                                .startup_error
                                .get()
                                .unwrap_or_else(|| "恢复未完成".to_owned())
                        })
                        .text_color(move || theme().destructive),
                    )
                    .child(
                        text("原配置未被修改；处理后可重新导入备份，或联系支持。")
                            .text_size(px(12.0))
                            .text_color(move || theme().muted_foreground),
                    )
            },
        ))
        .child(when(
            move || restored_state.restored.get().is_some() && !restored_state.acknowledged.get(),
            move || restored_notice(restored_state.clone()),
        ))
        .child(when(
            move || !cards_state.pending.get(),
            move || {
                let export_state = cards_state.clone();
                let import_state = cards_state.clone();
                view()
                    .flex_1()
                    .flex_col()
                    .gap_4()
                    .child(export_card::export_card(export_state))
                    .child(import_card::import_card(import_state))
            },
        ))
        .child(when(
            move || banner_state.pending.get(),
            pending_banner,
        ))
        .child(
            text(move || banner_state.prepare_status.get().unwrap_or_default())
                .test_id("backup-prepare-status")
                .text_size(px(12.0))
                .text_color(move || theme().foreground),
        )
        .child(when(
            move || ack_state.confirm_restore.get(),
            move || import_card::restore_dialog(ack_state.clone()),
        ))
}

/// 上次启动已应用恢复的摘要 + 确认按钮：确认（acknowledge）后才清标记。
fn restored_notice(state: BackupPageState) -> impl IntoElement {
    let ack = state.clone();
    let summary_state = state;
    view()
        .test_id("backup-restored")
        .flex_col()
        .gap_2()
        .p_3()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(text(move || {
            summary_state
                .restored
                .get()
                .map(|summary| {
                    format!(
                        "上次启动已从备份恢复：版本 v{} · {} · 订阅 {} · 覆写 {} · 文件 {}",
                        summary.version,
                        summary.created_at,
                        summary.subscriptions,
                        summary.profiles,
                        summary.files
                    )
                })
                .unwrap_or_default()
        }))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .bg(move || theme().primary)
                        .on_press(move || ack.acknowledge())
                        .child(
                            text("我知道了")
                                .test_id("backup-acknowledge")
                                .text_color(move || theme().primary_foreground),
                        ),
                )
                .child(
                    text("确认后才清除恢复标记；恢复后的配置已在正常使用。")
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                ),
        )
}

/// 待重启横幅：一切业务写已被业务层拒绝，只剩重启入口。
fn pending_banner() -> impl IntoElement {
    view()
        .test_id("backup-pending")
        .flex_col()
        .gap_2()
        .p_3()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().destructive)
        .child(
            text(
                "恢复已准备完成：请重启应用。重启前订阅、覆写、请求设置与核心库存的修改均被拒绝。",
            )
            .text_color(move || theme().destructive),
        )
        .child(
            text("回滚备份已加密保存在本数据根 backup-history/；若新配置异常可据此还原。")
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    button()
                        .px_4()
                        .py_2()
                        .rounded(move || theme().radius)
                        .bg(move || theme().primary)
                        .on_press(|| {
                            // NSH 可靠重启：旧进程退出后才拉起同程序新进程，
                            // 新进程的启动事务在数据根锁内应用待恢复备份。
                            nsh_core::element::desktop::restart();
                        })
                        .child(
                            text("立即重启应用")
                                .test_id("backup-restart")
                                .text_color(move || theme().primary_foreground),
                        ),
                )
                .child(
                    text("重启自动完成恢复；若点击后应用没有重启，请手动重启。")
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                ),
        )
}

/// 预览有效期（与共享 `PreviewCache` 一致）：到期丢弃授权与摘要。
pub(super) const PREVIEW_TTL: Duration = Duration::from_secs(300);

/// 卡片容器：圆角边框 + 标题 + 说明，子内容由调用方继续填充。
pub(super) fn section(title: &'static str, note: &'static str) -> View {
    view()
        .flex_col()
        .gap_2()
        .p_3()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(text(title).text_size(px(15.0)))
        .child(
            text(note)
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
}
