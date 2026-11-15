//! 备份页导入卡片：原生打开对话框选择 `.zip.age`（16 MiB 上限）→ 掩码
//! 密码 → 解密预览（一次性 preview_id、300 秒过期，改文件/改密码立即
//! discard）→ 确认恢复（停止核心 + 准备加密回滚 + pending）→ 待重启。
//! 密码与明文不落日志/storage；共享层对一切失败只回脱敏错误。

use std::sync::Arc;

use nsh_base::{button, dialog, input, theme};
use nsh_core::file_dialog::{FileFilter, OpenDialog};
use nsh_core::prelude::*;

use super::{BackupPageState, PREVIEW_TTL, section};

pub(super) fn import_card(state: BackupPageState) -> impl IntoElement {
    let pick_state = state.clone();
    let password_state = state.clone();
    let inspect_state = state.clone();
    let summary_state = state.clone();
    let confirm_state = state.clone();
    let status_state = state.clone();
    section(
        "从备份恢复",
        "选择 .zip.age 加密备份并输入密码解密预览；确认恢复会停止核心（系统代理按原值恢复）、生成加密回滚，准备完成后需重启应用。",
    )
    .child(
        view()
            .flex_row()
            .items_center()
            .gap_2()
            .child(pick_button(pick_state))
            .child(text(move || {
                status_state
                    .import_name
                    .get()
                    .map(|name| format!("已选择：{name}"))
                    .unwrap_or_else(|| "未选择文件".to_owned())
            })),
    )
    .child(import_password_input(password_state))
    .child(
        view()
            .flex_row()
            .items_center()
            .gap_2()
            .child(inspect_button(inspect_state.clone()))
            .child(
                text(move || {
                    if summary_state.importing.get() {
                        "解密中…".to_owned()
                    } else if summary_state.preparing.get() {
                        "准备恢复中…".to_owned()
                    } else {
                        String::new()
                    }
                })
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
            ),
    )
    .child(when(
        move || confirm_state.preview.get().is_some(),
        move || preview_summary(confirm_state.clone()),
    ))
    .child(
        text(move || status_state.import_status.get().unwrap_or_default())
            .test_id("backup-import-status")
            .text_size(px(12.0))
            .text_color(move || theme().destructive),
    )
}

fn pick_button(state: BackupPageState) -> impl IntoElement {
    let label = state.clone();
    button()
        .px_3()
        .py_1()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .on_press(move || request_pick(state.clone()))
        .child(
            text(move || {
                if label.import_name.get().is_some() {
                    "重新选择…"
                } else {
                    "选择备份文件…"
                }
            })
            .test_id("backup-pick")
            .text_color(move || theme().foreground),
        )
}

fn import_password_input(state: BackupPageState) -> impl IntoElement {
    let change_state = state.clone();
    input()
        .test_id("backup-import-password")
        .value(state.import_password)
        .placeholder("备份密码")
        .auto_correct(false)
        // 口令输入恒掩码；任何修改立即放弃当前预览（与改文件同一语义）。
        .masked(true)
        .on_change(move |text: &str| {
            change_state.import_password.set(text.to_owned());
            change_state.drop_preview();
            change_state.import_status.set(None);
        })
        .p_2()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().input)
        .bg(move || theme().background)
}

fn inspect_button(state: BackupPageState) -> impl IntoElement {
    button()
        .px_4()
        .py_2()
        .rounded(move || theme().radius)
        .bg(move || theme().primary)
        .on_press(move || request_inspect(state.clone()))
        .child(
            text("解密并预览")
                .test_id("backup-inspect")
                .text_color(move || theme().primary_foreground),
        )
}

/// 预览摘要 + 恢复入口；重新选择/改密码时已整体丢弃（父卡片条件渲染）。
fn preview_summary(state: BackupPageState) -> impl IntoElement {
    let confirm = state.clone();
    let reselect = state.clone();
    let summary_state = state.clone();
    view()
        .test_id("backup-preview")
        .flex_col()
        .gap_2()
        .p_3()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(text(move || {
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
        }))
        .child(
            text("预览绑定一次性授权，300 秒后过期；改密码或换文件需重新解密。确认后不可撤销，准备完成需重启应用。")
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    button()
                        .px_4()
                        .py_2()
                        .rounded(move || theme().radius)
                        .bg(move || theme().destructive)
                        .on_press(move || confirm.confirm_restore.set(true))
                        .child(
                            text("恢复此备份…")
                                .test_id("backup-confirm-open")
                                .text_color(move || theme().primary_foreground),
                        ),
                )
                .child(
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .border(px(1.0))
                        .border_color(move || theme().border)
                        .on_press(move || {
                            reselect.drop_preview();
                            reselect.import_status.set(None);
                        })
                        .child(text("放弃预览")),
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
                        // 换文件：旧预览立即失效（共享缓存 discard + 清摘要）。
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

/// 预览过期调度：到期时若该预览仍绑定，丢弃授权与摘要（共享缓存本身
/// 对过期 take 同样拒绝，这里是 UI 层的及时清理）。离页随 owner 取消。
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
    let cancel_state = state.clone();
    let confirm_state = state;
    dialog()
        .on_cancel(move || cancel_state.confirm_restore.set(false))
        .child(
            view()
                .flex_col()
                .gap_3()
                .w(px(460.0))
                .p_4()
                .rounded(move || theme().radius)
                .bg(move || theme().card)
                .border(px(1.0))
                .border_color(move || theme().border)
                .child(text("恢复此备份？"))
                .child(
                    text("将停止核心（系统代理按原值精确恢复），把当前配置加密保存为回滚备份，然后准备恢复所选备份。准备完成后需重启应用；重启前订阅、覆写、请求设置与核心库存的修改均被拒绝。")
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                )
                .child(
                    text("准备成功后在备份页重启应用即完成恢复；原配置始终可从本数据根 backup-history/ 的回滚备份还原。")
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                )
                .child(
                    view()
                        .flex_row()
                        .gap_2()
                        .justify_end()
                        .child(
                            button()
                                .px_3()
                                .py_1()
                                .rounded(move || theme().radius)
                                .border(px(1.0))
                                .border_color(move || theme().border)
                                .on_press(move || cancel_state.confirm_restore.set(false))
                                .child(text("取消")),
                        )
                        .child(
                            button()
                                .px_3()
                                .py_1()
                                .rounded(move || theme().radius)
                                .bg(move || theme().destructive)
                                .on_press(move || request_prepare(confirm_state.clone()))
                                .child(
                                    text("确认恢复")
                                        .test_id("backup-confirm-restore")
                                        .text_color(move || theme().primary_foreground),
                                ),
                        ),
                ),
        )
        .into_element()
}

/// 确认后的恢复准备：消费一次性预览授权 → 活动屏障写保护（等在途任务
/// 清零）→ 停止核心 → 业务锁内生成加密回滚并落 pending。成功标记待
/// 重启；失败真实上屏且不遗留 pending（授权已消费，需重新解密）。
fn request_prepare(state: BackupPageState) {
    if state.preparing.get() {
        return;
    }
    // 导出写出进行中不启动恢复：避免 pending 翻转杀死在途写出任务。
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
    // 从确认弹窗发起：弹窗 owner 在确认按下后销毁，spawn 必须挂页面
    // owner（离开页面才取消在途准备，绝不半途留下半准备状态）。
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
