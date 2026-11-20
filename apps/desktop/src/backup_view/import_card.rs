//! 备份页导入卡片：原生打开对话框选择 `.zip.age`（16 MiB 上限）→ 掩码
//! 密码 → 解密预览（一次性 preview_id、300 秒过期，改文件/改密码立即
//! discard）→ 确认恢复（停止核心 + 准备加密回滚 + pending）→ 待重启。
//! 密码与明文不落日志/storage；共享层对一切失败只回脱敏错误。

use std::sync::Arc;

use nsh_base::theme;
use nsh_core::file_dialog::{FileFilter, OpenDialog};
use nsh_core::prelude::*;

use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::layout::{AlertTone, alert_banner};

use super::{BackupPageState, PREVIEW_TTL, section};

pub(super) fn import_card(state: BackupPageState) -> impl IntoElement {
    let pick_state = state.clone();
    let password_state = state.clone();
    let inspect_state = state.clone();
    let summary_state = state.clone();
    let confirm_state = state.clone();
    let import_name = state.import_name;
    let import_status = state.import_status;
    section(
        "从备份恢复",
        "选择 .zip.age 加密备份并输入密码解密预览；确认恢复会停止核心（系统代理按原值恢复）、生成加密回滚，准备完成后需重启应用。",
    )
    .child(
        view()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_3()
            .child(pick_button(pick_state))
            .child(
                text(move || {
                    import_name
                        .get()
                        .map(|name| format!("已选择：{name}"))
                        .unwrap_or_else(|| "未选择文件".to_owned())
                })
                .text_size(px(13.0))
                .text_color(move || {
                    if import_name.get().is_some() {
                        theme().foreground
                    } else {
                        theme().muted_foreground
                    }
                }),
            ),
    )
    .child(import_password_input(password_state))
    .child(
        view()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_3()
            .child(inspect_button(inspect_state))
            .child(when(
                move || summary_state.importing.get() || summary_state.preparing.get(),
                move || {
                    let s = summary_state.clone();
                    text(move || {
                        if s.importing.get() {
                            "解密中…".to_owned()
                        } else if s.preparing.get() {
                            "准备恢复中…".to_owned()
                        } else {
                            String::new()
                        }
                    })
                    .text_size(px(12.5))
                    .text_color(move || theme().muted_foreground)
                },
            )),
    )
    .child(when(
        move || confirm_state.preview.get().is_some(),
        move || preview_summary(confirm_state.clone()),
    ))
    .child(when(
        move || import_status.get().is_some(),
        move || {
            alert_banner(
                AlertTone::Danger,
                text(move || import_status.get().unwrap_or_default())
                    .test_id("backup-import-status")
                    .text_size(px(12.5))
                    .text_color(move || theme().destructive),
            )
        },
    ))
}

fn pick_button(state: BackupPageState) -> impl IntoElement {
    let label = state.clone();
    crate::ui::button(crate::ui::ButtonVariant::Outline)
        .size(crate::ui::ButtonSize::Sm)
        .prefix(|| icon_sized(Icon::File, Tone::Muted, 13.0))
        .on_press(move || request_pick(state.clone()))
        .test_id("backup-pick")
        .label(move || {
            if label.import_name.get().is_some() {
                "重新选择…".to_owned()
            } else {
                "选择备份文件…".to_owned()
            }
        })
}

fn import_password_input(state: BackupPageState) -> impl IntoElement {
    let change_state = state.clone();
    crate::ui::field()
        .label(|| t!("backup.import_password"))
        .control(
            crate::ui::input(state.import_password)
                .test_id("backup-import-password")
                .accessible_name(|| t!("backup.import_password"))
                .placeholder(|| t!("product.backup.password_short"))
                .masked()
                .disabled(move || {
                    state.exporting.get()
                        || state.importing.get()
                        || state.preparing.get()
                        || state.pending.get()
                })
                .on_change(move |text| {
                    change_state.import_password.set(text);
                    change_state.drop_preview();
                    change_state.import_status.set(None);
                }),
        )
}

fn inspect_button(state: BackupPageState) -> impl IntoElement {
    let loading = state.clone();
    crate::ui::button(crate::ui::ButtonVariant::Primary)
        .size(crate::ui::ButtonSize::Md)
        .loading(move || loading.importing.get())
        .on_press(move || request_inspect(state.clone()))
        .test_id("backup-inspect")
        .label(|| "解密并预览".to_owned())
}

/// 预览摘要 + 恢复入口；重新选择/改密码时已整体丢弃（父卡片条件渲染）。
fn preview_summary(state: BackupPageState) -> impl IntoElement {
    let confirm = state.clone();
    let reselect = state.clone();
    let summary_state = state;
    view()
        .test_id("backup-preview")
        .flex_col()
        .gap_3()
        .p_4()
        .rounded(move || theme().radius)
        .bg(move || theme().muted.opacity(0.45))
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(
            text(move || {
                summary_state
                    .preview
                    .get()
                    .map(|preview| {
                        let summary = &preview.summary;
                        format!(
                            "备份版本 v{} · {} · 平台 {} · 订阅 {} · 覆写 {} · 文件 {} · {} 字节",
                            summary.version,
                            summary.created_at,
                            summary.platform,
                            summary.subscriptions,
                            summary.profiles,
                            summary.files,
                            summary.bytes
                        )
                    })
                    .unwrap_or_default()
            })
            .text_size(px(13.0))
            .font_weight(FontWeight::MEDIUM)
            .selectable(true),
        )
        .child(
            text(|| t!("product.backup.preview_hint"))
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    crate::ui::button(crate::ui::ButtonVariant::Destructive)
                        .size(crate::ui::ButtonSize::Md)
                        .on_press(move || confirm.confirm_restore.set(true))
                        .test_id("backup-confirm-open")
                        .label(|| "恢复此备份…".to_owned()),
                )
                .child(
                    crate::ui::button(crate::ui::ButtonVariant::Outline)
                        .size(crate::ui::ButtonSize::Sm)
                        .on_press(move || {
                            reselect.drop_preview();
                            reselect.import_status.set(None);
                        })
                        .label(|| "放弃预览".to_owned()),
                ),
        )
}

/// 选择备份文件：原生打开对话框 → 限长读入 → 放弃旧预览（改文件即
/// 失效）。取消静默；无法显示/读取失败真实上屏。
fn request_pick(state: BackupPageState) {
    if state.importing.get() || state.preparing.get() {
        return;
    }
    let Some(service) = state.service.clone() else {
        state.import_status.set(Some(format!(
            "选择失败：{}",
            BackupPageState::no_service_error()
        )));
        return;
    };
    let task_state = state.clone();
    state.owner.run(move || {
        state.import_task.with(|slot| {
            slot.replace(spawn(async move {
                let state = task_state;
                let picked = pick_paths(
                    OpenDialog::files()
                        .title("选择加密备份")
                        .button_label("选择")
                        .filter(FileFilter::new("加密备份", ["age"])),
                )
                .await;
                let result = match picked {
                    Err(error) => Err(lanpath_common::LanpathError::Config(format!(
                        "打开对话框无法显示:{error}"
                    ))),
                    Ok(None) => Ok(None),
                    Ok(Some(paths)) => paths
                        .first()
                        .map(|path| service.read_import(path).map(|bytes| (path.clone(), bytes)))
                        .transpose(),
                };
                match result {
                    Ok(None) => {}
                    Ok(Some((path, bytes))) => {
                        state.drop_preview();
                        state.import_name.set(Some(
                            path.file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|| path.display().to_string()),
                        ));
                        state.import_bytes.set(Some(Arc::new(bytes)));
                        state.import_status.set(None);
                    }
                    Err(error) => state.import_status.set(Some(format!("选择失败:{error}"))),
                }
            }));
        });
    });
}

/// 解密并预览：真实 age 解密（blocking 线程），成功后展示摘要并调度
/// 300 秒过期；失败只显示共享层脱敏错误（不含明文/口令细节）。
fn request_inspect(state: BackupPageState) {
    if state.importing.get() || state.preparing.get() {
        return;
    }
    let Some(service) = state.service.clone() else {
        state.import_status.set(Some(format!(
            "解密失败：{}",
            BackupPageState::no_service_error()
        )));
        return;
    };
    let Some(bytes) = state.import_bytes.get() else {
        state.import_status.set(Some("请先选择备份文件".to_owned()));
        return;
    };
    if state.import_password.get().is_empty() {
        state.import_status.set(Some("请输入备份密码".to_owned()));
        return;
    }
    state.importing.set(true);
    state.import_status.set(None);
    state.drop_preview();
    let password = state.import_password.get();
    let task_state = state.clone();
    state.owner.run(move || {
        state.import_task.with(|slot| {
            slot.replace(spawn(async move {
                let state = task_state;
                let result = match blocking(move || service.inspect(&bytes, password)).await {
                    Ok(result) => result,
                    Err(error) => {
                        state.importing.set(false);
                        state
                            .import_status
                            .set(Some(format!("解密任务中断:{error}")));
                        return;
                    }
                };
                state.importing.set(false);
                match result {
                    Ok(preview) => {
                        let preview_id = preview.preview_id.clone();
                        state.preview.set(Some(super::PreviewInfo {
                            summary: preview.summary,
                            preview_id: preview_id.clone(),
                        }));
                        schedule_preview_expiry(state.clone(), preview_id);
                    }
                    Err(error) => state
                        .import_status
                        .set(Some(format!("解密预览失败:{error}"))),
                }
            }));
        });
    });
}

/// 预览过期调度：到期时若该预览仍绑定，丢弃授权与摘要。
fn schedule_preview_expiry(state: BackupPageState, preview_id: String) {
    let task_state = state.clone();
    state.owner.run(move || {
        state.expiry_task.with(|slot| {
            slot.replace(spawn(async move {
                let state = task_state;
                timer(PREVIEW_TTL).await;
                if state
                    .preview
                    .get()
                    .is_some_and(|preview| preview.preview_id == preview_id)
                {
                    state.drop_preview();
                    state
                        .import_status
                        .set(Some("备份预览已过期，请重新解密".to_owned()));
                }
            }));
        });
    });
}

/// 恢复确认弹窗：明确停止核心/系统代理与重启要求，确认后才准备。
pub(super) fn restore_dialog(state: BackupPageState) -> AnyElement {
    let open = state.confirm_restore;
    let cancel_state = state.clone();
    let confirm_state = state;
    crate::ui::confirm_dialog(move || open.get())
        .destructive()
        .confirm_test_id("backup-confirm-restore")
        .title(|| t!("product.backup.confirm_title"))
        .description(|| {
            format!(
                "{}\n{}",
                t!("product.backup.confirm_hint"),
                t!("product.backup.pending_hint")
            )
        })
        .cancel_label(|| t!("product.action.cancel"))
        .confirm_label(|| "确认恢复".to_owned())
        .on_cancel(move || cancel_state.confirm_restore.set(false))
        .on_confirm(move || request_prepare(confirm_state.clone()))
        .into_element()
}

/// 确认后的恢复准备：消费一次性预览授权 → 活动屏障写保护 → 停止核心 →
/// 业务锁内生成加密回滚并落 pending。
fn request_prepare(state: BackupPageState) {
    if state.preparing.get() {
        return;
    }
    if state.exporting.get() {
        state
            .prepare_status
            .set(Some("导出进行中，请完成或稍后再恢复".to_owned()));
        return;
    }
    state.confirm_restore.set(false);
    let Some(service) = state.service.clone() else {
        state.prepare_status.set(Some(format!(
            "准备失败：{}",
            BackupPageState::no_service_error()
        )));
        return;
    };
    let Some(runtime) = state.runtime.clone() else {
        state
            .prepare_status
            .set(Some("准备失败：核心运行时不可用".to_owned()));
        return;
    };
    let Some(preview) = state.preview.get() else {
        state
            .prepare_status
            .set(Some("准备失败：预览已失效，请重新解密".to_owned()));
        return;
    };
    state.preparing.set(true);
    state.prepare_status.set(None);
    state.import_status.set(None);
    let preview_id = preview.preview_id;
    state.owner.run(move || {
        state.prepare_task.with(|slot| {
            slot.replace(spawn(async move {
                let result = service
                    .prepare_confirmed_restore(&preview_id, &runtime)
                    .await;
                state.preparing.set(false);
                state.preview.set(None);
                state.confirm_restore.set(false);
                match result {
                    Ok(rollback) => {
                        state.import_password.set(String::new());
                        state.import_bytes.set(None);
                        state.import_name.set(None);
                        state.pending.set(true);
                        state.prepare_status.set(Some(format!(
                            "恢复准备完成，请立即重启应用：回滚备份 {}",
                            rollback.display()
                        )));
                    }
                    Err(error) => state
                        .prepare_status
                        .set(Some(format!("恢复准备失败:{error}"))),
                }
            }));
        });
    });
}
