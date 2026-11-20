//! 备份页：加密备份导出与恢复导入/确认。复用共享 `lanpath-client::backup`
//! 的 age 加密、一次性预览与启动事务（见 `backup` 业务模块）；本页只做
//! 交互与真实状态上屏。

mod export_card;
mod import_card;

use std::sync::Arc;
use std::time::Duration;

use lanpath_client::backup::BackupSummary;
use nsh_base::theme;
use nsh_core::prelude::*;

use crate::backup::Backup;
use crate::core_runtime::CoreRuntime;
use crate::ui::icon::{Icon, Tone, icon};
use crate::ui::layout::{AlertTone, SurfaceLevel, alert_banner, section_title, surface};

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
    pub(super) owner: Owner,
    pub(super) service: Option<Arc<Backup>>,
    pub(super) runtime: Option<Arc<CoreRuntime>>,
    pub(super) startup_error: Signal<Option<String>>,
    pub(super) restored: Signal<Option<BackupSummary>>,
    pub(super) acknowledged: Signal<bool>,
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
    let transaction = state.clone();
    crate::navigation::protect(move || {
        if transaction.exporting.get() || transaction.importing.get() || transaction.preparing.get()
        {
            crate::navigation::LeaveState::Busy
        } else if !transaction.pending.get()
            && (!transaction.export_password.get().is_empty()
                || !transaction.import_password.get().is_empty()
                || transaction.import_bytes.get().is_some())
        {
            crate::navigation::LeaveState::Dirty
        } else {
            crate::navigation::LeaveState::Clean
        }
    });
    let ack_state = state.clone();
    let restored_state = state.clone();
    let startup_state = state.clone();
    let cards_state = state.clone();
    let banner_state = state.clone();
    let prepare_status = state.prepare_status;
    view()
        .flex_1()
        .flex_col()
        .min_h(px(0.0))
        .child(
            scroll().flex_1().min_h(px(0.0)).child(
                view()
                    .flex_col()
                    .gap_5()
                    .max_w(px(760.0))
                    .child(crate::ui::page_header(
                        || t!("product.backup.title"),
                        || t!("product.backup.subtitle"),
                    ))
                    .child(when(
                        move || startup_state.startup_error.get().is_some(),
                        move || {
                            let state = startup_state.clone();
                            alert_banner(
                                AlertTone::Danger,
                                view()
                                    .test_id("backup-startup-error")
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        view()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .child(icon(Icon::Warning, Tone::Danger))
                                            .child(
                                                text(move || {
                                                    state
                                                        .startup_error
                                                        .get()
                                                        .unwrap_or_else(|| "恢复未完成".to_owned())
                                                })
                                                .text_size(px(13.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(move || theme().destructive),
                                            ),
                                    )
                                    .child(
                                        text(|| t!("product.backup.failed_hint"))
                                            .text_size(px(12.0))
                                            .text_color(move || theme().muted_foreground),
                                    ),
                            )
                        },
                    ))
                    .child(when(
                        move || {
                            restored_state.restored.get().is_some()
                                && !restored_state.acknowledged.get()
                        },
                        move || restored_notice(restored_state.clone()),
                    ))
                    .child(when(
                        move || !cards_state.pending.get(),
                        move || {
                            let export_state = cards_state.clone();
                            let import_state = cards_state.clone();
                            view()
                                .flex_col()
                                .gap_5()
                                .child(export_card::export_card(export_state))
                                .child(import_card::import_card(import_state))
                        },
                    ))
                    .child(when(move || banner_state.pending.get(), pending_banner))
                    .child(when(
                        move || prepare_status.get().is_some(),
                        move || {
                            alert_banner(
                                AlertTone::Info,
                                text(move || prepare_status.get().unwrap_or_default())
                                    .test_id("backup-prepare-status")
                                    .text_size(px(12.5))
                                    .text_color(move || theme().foreground),
                            )
                        },
                    )),
            ),
        )
        .child(import_card::restore_dialog(ack_state))
}

/// 上次启动已应用恢复的摘要 + 确认按钮：确认（acknowledge）后才清标记。
fn restored_notice(state: BackupPageState) -> impl IntoElement {
    let ack = state.clone();
    let summary_state = state;
    alert_banner(
        AlertTone::Success,
        view()
            .test_id("backup-restored")
            .flex_col()
            .gap_3()
            .child(
                text(move || {
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
                })
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM),
            )
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_3()
                    .child(
                        crate::ui::button(crate::ui::ButtonVariant::Primary)
                            .size(crate::ui::ButtonSize::Sm)
                            .on_press(move || ack.acknowledge())
                            .test_id("backup-acknowledge")
                            .label(|| t!("product.action.acknowledge")),
                    )
                    .child(
                        text(|| t!("product.backup.ack_hint"))
                            .text_size(px(12.0))
                            .text_color(move || theme().muted_foreground),
                    ),
            ),
    )
}

/// 待重启横幅：一切业务写已被业务层拒绝，只剩重启入口。
fn pending_banner() -> impl IntoElement {
    alert_banner(
        AlertTone::Warning,
        view()
            .test_id("backup-pending")
            .flex_col()
            .gap_3()
            .child(
                text(
                    "恢复已准备完成：请重启应用。重启前订阅、覆写、请求设置与核心库存的修改均被拒绝。",
                )
                .text_size(px(13.5))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(move || theme().destructive),
            )
            .child(
                text(|| t!("product.backup.rollback_hint"))
                    .text_size(px(12.5))
                    .text_color(move || theme().muted_foreground),
            )
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_3()
                    .child(
                        crate::ui::button(crate::ui::ButtonVariant::Primary)
                            .on_press(|| {
                                nsh_core::element::desktop::restart();
                            })
                            .test_id("backup-restart")
                            .label(|| "立即重启应用".to_owned()),
                    )
                    .child(
                        text(|| t!("product.backup.restart_hint"))
                            .text_size(px(12.0))
                            .text_color(move || theme().muted_foreground),
                    ),
            ),
    )
}

/// 预览有效期（与共享 `PreviewCache` 一致）：到期丢弃授权与摘要。
pub(super) const PREVIEW_TTL: Duration = Duration::from_secs(300);

/// 卡片容器：语义 Card 表面 + 标题 + 说明，子内容由调用方继续填充。
pub(super) fn section(title: &'static str, note: &'static str) -> View {
    surface(SurfaceLevel::Card)
        .p_5()
        .flex_col()
        .gap_3()
        .child(section_title(move || title.to_owned()))
        .child(
            text(note)
                .text_size(px(12.5))
                .text_color(move || theme().muted_foreground),
        )
}
