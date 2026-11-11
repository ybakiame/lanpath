use gpui_kit::{AppContext as _, Context, PathPromptOptions};

use crate::appearance::AppearanceState;

use super::{BackupView, Source, files};

impl BackupView {
    pub(super) fn choose_source(&mut self, cx: &mut Context<Self>) {
        if self.editing_disabled() {
            return;
        }
        self.begin("正在选择加密备份文件", cx);
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("选择 .zip.age 备份".into()),
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let selected = match prompt.await {
                Ok(Ok(Some(mut paths))) if paths.len() == 1 => Ok(paths.pop()),
                Ok(Ok(None)) => Ok(None),
                Ok(Ok(Some(_))) => Err("请选择一个 .zip.age 备份文件".to_string()),
                Ok(Err(error)) => Err(format!("无法打开文件选择器：{error}")),
                Err(_) => Err("文件选择器未返回结果；请重新选择".to_string()),
            };
            let result = match selected {
                Ok(Some(path)) => {
                    let label = format!("文件：{}（已读取的密文快照）", path.display());
                    let _ = this.update(cx, |this, cx| {
                        this.invalidate_preview();
                        this.source = None;
                        this.working = Some("正在读取并检查加密备份文件大小");
                        cx.notify();
                    });
                    Some(
                        cx.background_spawn(async move {
                            files::read_encrypted(&path)
                                .map_err(|error| {
                                    format!("读取所选备份 {} 失败：{error}", path.display())
                                })
                                .map(|encrypted| Source {
                                    label,
                                    encrypted: encrypted.into(),
                                })
                        })
                        .await,
                    )
                }
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            };
            let _ = this.update(cx, |this, cx| {
                match result {
                    Some(Ok(source)) => {
                        this.source = Some(source);
                        this.notice = Some(
                            "已读取加密文件；输入密码后解密检查。仅显示摘要，不展示归档明文。"
                                .into(),
                        );
                    }
                    Some(Err(error)) => this.error = Some(error),
                    None => {}
                }
                this.finish(cx);
            });
        }));
    }

    pub(super) fn export(&mut self, cx: &mut Context<Self>) {
        if self.editing_disabled() {
            return;
        }
        let (Some(service), Some(root)) = (self.service.clone(), self.root.clone()) else {
            return;
        };
        let password = self.export_password.read(cx).value().to_string();
        if password.chars().count() < 12 || password.len() > 1024 {
            self.error =
                Some("导出密码至少 12 个字符且不超过 1024 字节；可使用“生成密码”。".into());
            cx.notify();
            return;
        }
        let appearance = cx
            .global::<AppearanceState>()
            .preferences()
            .backup_appearance();
        let initial = root.parent().unwrap_or(&root);
        self.begin("正在选择加密备份保存位置", cx);
        let prompt = cx.prompt_for_new_path(initial, Some("lanpath-backup.zip.age"));
        self.task = Some(cx.spawn(async move |this, cx| {
            let selected = match prompt.await {
                Ok(Ok(path)) => Ok(path),
                Ok(Err(error)) => Err(format!("无法打开保存对话框：{error}")),
                Err(_) => Err("保存对话框未返回结果；请重新导出".to_string()),
            };
            let result = match selected {
                Ok(Some(path)) => {
                    let _ = this.update(cx, |this, cx| {
                        this.working = Some("正在捕获配置、加密并原子保存备份");
                        cx.notify();
                    });
                    Some(
                        cx.background_spawn(async move {
                            let path = files::export_destination(&path, &root)
                                .map_err(|error| error.to_string())?;
                            let encrypted = service
                                .export(&password, Some(appearance))
                                .map_err(|error| error.to_string())?;
                            files::save_encrypted(&path, &root, &encrypted)
                                .map_err(|error| error.to_string())?;
                            Ok::<_, String>(format!(
                                "加密备份已保存：{}。请另外保存密码。",
                                path.display()
                            ))
                        })
                        .await,
                    )
                }
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            };
            let _ = this.update(cx, |this, cx| {
                match result {
                    Some(Ok(notice)) => this.notice = Some(notice),
                    Some(Err(error)) => {
                        this.error = Some(format!("导出失败：{error}；已有备份不会被覆盖。"))
                    }
                    None => {}
                }
                this.finish(cx);
            });
        }));
    }
}
