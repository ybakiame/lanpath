//! 备份页导出卡片：掩码密码输入（至少 12 字符或生成强密码）+ 原生保存
//! 对话框 + 真实 `.zip.age` 写出。加密在 blocking 线程执行（age scrypt
//! CPU 密集，不占引擎线程）；成功后清空密码输入。

use nsh_base::theme;
use nsh_core::file_dialog::{FileFilter, SaveDialog};
use nsh_core::prelude::*;

use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::layout::{AlertTone, alert_banner};

use super::{BackupPageState, section};

pub(super) fn export_card(state: BackupPageState) -> impl IntoElement {
    let password_state = state.clone();
    let generate_state = state.clone();
    let button_state = state.clone();
    let status_state = state.clone();
    let copy_state = state.clone();
    let copy_password = state.export_password;
    let export_status = state.export_status;
    section(
        "导出加密备份",
        "把当前订阅、覆写与请求设置打包为 age 加密备份（.zip.age）。密码只用于加密，丢失无法找回；请使用密码管理器保存。",
    )
    .child(password_input(password_state))
    .child(
        view()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(export_button(button_state))
            .child(generate_button(generate_state))
            .child(
                crate::ui::button(crate::ui::ButtonVariant::Outline)
                    .size(crate::ui::ButtonSize::Sm)
                    .prefix(|| icon_sized(Icon::Copy, Tone::Muted, 13.0))
                    .label(|| t!("backup.copy_password"))
                    .test_id("backup-copy-password")
                    .disabled(move || copy_password.get().is_empty())
                    .on_press(move || {
                        set_clipboard(copy_state.export_password.get());
                        copy_state.export_status.set(None);
                        crate::ui::feedback::show_toast(t!("backup.password_copied"));
                    }),
            )
            .child(when(
                move || status_state.exporting.get(),
                || {
                    text(|| "导出中…".to_owned())
                        .text_size(px(12.5))
                        .text_color(move || theme().muted_foreground)
                },
            )),
    )
    .child(when(
        move || export_status.get().is_some(),
        move || {
            alert_banner(
                AlertTone::Danger,
                text(move || export_status.get().unwrap_or_default())
                    .test_id("backup-export-status")
                    .text_size(px(12.5))
                    .text_color(move || theme().destructive),
            )
        },
    ))
}

fn password_input(state: BackupPageState) -> impl IntoElement {
    let change_state = state.clone();
    crate::ui::field()
        .label(|| t!("backup.password"))
        .hint(|| t!("backup.password_hint"))
        .control(
            crate::ui::input(state.export_password)
                .test_id("backup-export-password")
                .accessible_name(|| t!("backup.password"))
                .masked()
                .disabled(move || {
                    state.exporting.get()
                        || state.importing.get()
                        || state.preparing.get()
                        || state.pending.get()
                })
                .on_change(move |value| {
                    change_state.export_password.set(value);
                    change_state.export_status.set(None);
                }),
        )
}

fn generate_button(state: BackupPageState) -> impl IntoElement {
    let blocked = state.clone();
    crate::ui::button(crate::ui::ButtonVariant::Outline)
        .disabled(move || {
            blocked.exporting.get()
                || blocked.importing.get()
                || blocked.preparing.get()
                || blocked.pending.get()
        })
        .size(crate::ui::ButtonSize::Sm)
        .on_press(move || {
            state
                .export_password
                .set(lanpath_client::backup::generate_password());
            state.export_status.set(None);
            crate::ui::feedback::show_toast(
                "已生成随机强密码：请立即保存到密码管理器，导出成功后此处会清空",
            );
        })
        .test_id("backup-generate-password")
        .label(|| t!("product.action.generate_password"))
}

fn export_button(state: BackupPageState) -> impl IntoElement {
    let label = state.clone();
    crate::ui::button(crate::ui::ButtonVariant::Primary)
        .size(crate::ui::ButtonSize::Md)
        .prefix(|| icon_sized(Icon::Download, Tone::OnPrimary, 14.0))
        .loading(move || label.exporting.get())
        .on_press(move || request_export(state.clone()))
        .test_id("backup-export")
        .label(move || {
            if label.exporting.get() {
                "导出中…".to_owned()
            } else {
                "导出备份…".to_owned()
            }
        })
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
                    Ok(None) => {}
                    Ok(Some(path)) => {
                        state.export_password.set(String::new());
                        state.export_status.set(None);
                        crate::ui::feedback::show_toast(format!(
                            "已导出加密备份：{}",
                            path.display()
                        ));
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
