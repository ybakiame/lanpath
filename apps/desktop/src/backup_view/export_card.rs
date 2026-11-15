//! 备份页导出卡片：掩码密码输入（至少 12 字符或生成强密码）+ 原生保存
//! 对话框 + 真实 `.zip.age` 写出。加密在 blocking 线程执行（age scrypt
//! CPU 密集，不占引擎线程）；成功后清空密码输入。

use nsh_base::{button, input, theme};
use nsh_core::file_dialog::{FileFilter, SaveDialog};
use nsh_core::prelude::*;

use super::{BackupPageState, section};

pub(super) fn export_card(state: BackupPageState) -> impl IntoElement {
    let password_state = state.clone();
    let generate_state = state.clone();
    let button_state = state.clone();
    let status_state = state.clone();
    section(
        "导出加密备份",
        "把当前订阅、覆写与请求设置打包为 age 加密备份（.zip.age）。密码只用于加密，丢失无法找回；请使用密码管理器保存。",
    )
    .child(password_input(password_state))
    .child(
        view()
            .flex_row()
            .items_center()
            .gap_2()
            .child(generate_button(generate_state))
            .child(export_button(button_state))
            .child(
                text(move || {
                    if status_state.exporting.get() {
                        "导出中…".to_owned()
                    } else {
                        String::new()
                    }
                })
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
            ),
    )
    .child(
        text(move || status_state.export_status.get().unwrap_or_default())
            .test_id("backup-export-status")
            .text_size(px(12.0))
            .text_color(move || {
                if status_state.export_status.get().is_some() {
                    theme().foreground
                } else {
                    theme().muted_foreground
                }
            }),
    )
}

fn password_input(state: BackupPageState) -> impl IntoElement {
    let change_state = state.clone();
    input()
        .test_id("backup-export-password")
        .value(state.export_password)
        .placeholder("备份密码（至少 12 个字符）")
        .auto_correct(false)
        // 口令输入恒掩码：任何状态下都不明文显示。
        .masked(true)
        .on_change(move |text: &str| {
            change_state.export_password.set(text.to_owned());
            change_state.export_status.set(None);
        })
        .p_2()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().input)
        .bg(move || theme().background)
}

fn generate_button(state: BackupPageState) -> impl IntoElement {
    button()
        .px_3()
        .py_1()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .on_press(move || {
            // 共享强密码（24 位字母数字，满足 ≥12 约束）；只写回输入框，
            // 不落 storage/日志。
            state
                .export_password
                .set(lanpath_client::backup::generate_password());
            state.export_status.set(Some(
                "已生成随机强密码：请立即保存到密码管理器，导出成功后此处会清空".to_owned(),
            ));
        })
        .child(text("生成强密码").test_id("backup-generate-password"))
}

fn export_button(state: BackupPageState) -> impl IntoElement {
    let label = state.clone();
    button()
        .px_4()
        .py_2()
        .rounded(move || theme().radius)
        .bg(move || theme().primary)
        .on_press(move || request_export(state.clone()))
        .child(
            text(move || {
                if label.exporting.get() {
                    "导出中…"
                } else {
                    "导出备份…"
                }
            })
            .test_id("backup-export")
            .text_color(move || theme().primary_foreground),
        )
}

/// 导出任务：校验口令 → blocking 线程真实加密 → 原生保存对话框 → 写出。
/// 取消不做事；对话框无法显示与写出失败都真实上屏，绝不伪成功。
fn request_export(state: BackupPageState) {
    if state.exporting.get() || state.importing.get() || state.preparing.get() {
        return;
    }
    let Some(service) = state.service.clone() else {
        state.export_status.set(Some(format!(
            "导出失败：{}",
            BackupPageState::no_service_error()
        )));
        return;
    };
    let password = state.export_password.get();
    if password.chars().count() < 12 {
        state.export_status.set(Some(
            "导出失败：备份密码至少 12 个字符；建议使用“生成强密码”".to_owned(),
        ));
        return;
    }
    state.exporting.set(true);
    state.export_status.set(None);
    // 页面 owner 下发任务：卡片显隐（pending 翻转/切页）不杀死在途写出，
    // 避免写出半途而废留下半成品文件。
    state.owner.run(move || {
        state.export_task.with(|slot| {
            slot.replace(spawn(async move {
                let export_service = service.clone();
                let encrypted =
                    match blocking(move || export_service.export_encrypted(&password)).await {
                        Ok(result) => result,
                        Err(error) => {
                            state.exporting.set(false);
                            state
                                .export_status
                                .set(Some(format!("导出任务中断:{error}")));
                            return;
                        }
                    };
                let result = match encrypted {
                    Ok(bytes) => export_to_picked_path(&service, bytes).await,
                    Err(error) => Err(error),
                };
                state.exporting.set(false);
                match result {
                    // 用户在保存对话框取消：不是失败，静默结束（口令保留待重试）。
                    Ok(None) => {}
                    Ok(Some(path)) => {
                        // 成功后清空口令输入：口令不留在页面状态里。
                        state.export_password.set(String::new());
                        state
                            .export_status
                            .set(Some(format!("已导出加密备份：{}", path.display())));
                    }
                    Err(error) => state.export_status.set(Some(format!("导出失败:{error}"))),
                }
            }));
        });
    });
}

/// 原生保存对话框选定目标后写出加密字节；用户取消返回 `Ok(None)`（不是
/// 失败，调用方静默结束）。
async fn export_to_picked_path(
    service: &std::sync::Arc<crate::backup::Backup>,
    bytes: Vec<u8>,
) -> lanpath_common::LanpathResult<Option<std::path::PathBuf>> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let picked = pick_save_path(
        SaveDialog::new()
            .file_name(format!("Lanpath-backup-{stamp}.zip.age"))
            .title("导出加密备份")
            .button_label("导出")
            .filter(FileFilter::new("加密备份", ["age"])),
    )
    .await;
    match picked {
        Err(error) => Err(lanpath_common::LanpathError::Config(format!(
            "保存对话框无法显示:{error}"
        ))),
        Ok(None) => Ok(None),
        Ok(Some(path)) => service.write_export(&path, &bytes).map(Some),
    }
}
