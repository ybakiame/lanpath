use std::time::{Duration, Instant};

use gpui_kit::{AppContext as _, ClipboardItem, Context, Window};

use super::{BackupEvent, BackupView, Inspected, Source};

impl BackupView {
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.busy() || self.closing {
            return;
        }
        let Some(service) = self.service.clone() else {
            return;
        };
        self.working = Some("正在读取恢复事务状态");
        self.sync_readonly(cx);
        self.task = Some(cx.spawn(async move |this, cx| {
            let (restored, pending) = cx
                .background_spawn(async move {
                    (
                        service.restored().map_err(|error| error.to_string()),
                        service.is_pending(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let startup_notice = pending || !matches!(&restored, Ok(None));
                this.pending = pending;
                match restored {
                    Ok(summary) => {
                        this.restored = summary;
                        this.startup_error = None;
                    }
                    Err(error) => {
                        this.startup_error = Some(format!(
                            "读取启动恢复结果失败：{error}。修复后重新载入状态；已有摘要保留。"
                        ))
                    }
                }
                this.finish(cx);
                if startup_notice && !this.startup_notice_sent {
                    this.startup_notice_sent = true;
                    cx.emit(BackupEvent::StartupNotice);
                }
            });
        }));
        cx.notify();
    }

    pub(super) fn generate_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_disabled() {
            return;
        }
        self.begin("正在生成备份密码", cx);
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let password = cx
                .background_spawn(async { lanpath_client::backup::generate_password() })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.export_password.update(cx, |state, cx| {
                    state.set_masked(true, window, cx);
                    state.set_value(password, window, cx);
                });
                this.notice = Some("已生成新密码；请单独保存密码，丢失后无法解密备份。".into());
                this.finish(cx);
            });
        }));
    }

    pub(super) fn reveal_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_disabled() {
            return;
        }
        let masked = self
            .export_password
            .read(cx)
            .context_menu_capabilities()
            .is_masked();
        self.export_password
            .update(cx, |state, cx| state.set_masked(!masked, window, cx));
        cx.notify();
    }

    pub(super) fn copy_password(&mut self, cx: &mut Context<Self>) {
        if self.editing_disabled() {
            return;
        }
        let password = self.export_password.read(cx).value();
        if password.is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(password.to_string()));
        self.notice = Some("密码已复制到系统剪贴板；使用后请自行清除，勿粘贴到日志或报告。".into());
        cx.notify();
    }

    pub(super) fn load_previous(&mut self, cx: &mut Context<Self>) {
        if self.editing_disabled() {
            return;
        }
        let Some(service) = self.service.clone() else {
            return;
        };
        self.begin("正在读取最新加密回滚备份", cx);
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(
                    async move { service.previous().map_err(|error| error.to_string()) },
                )
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(Some(encrypted)) => {
                        this.invalidate_preview();
                        this.source = Some(Source {
                            label: "最新加密回滚备份（已读取的密文快照）".into(),
                            encrypted: encrypted.into(),
                        });
                        this.notice = Some(
                            "已载入最新回滚备份；输入准备该次恢复时使用的备份密码，再解密检查。"
                                .into(),
                        );
                    }
                    Ok(None) => {
                        this.notice =
                            Some("尚无回滚备份；成功准备恢复后才会生成加密回滚快照。".into())
                    }
                    Err(error) => {
                        this.error = Some(format!("读取回滚备份失败：{error}；原有来源保留。"))
                    }
                }
                this.finish(cx);
            });
        }));
    }

    pub(super) fn inspect(&mut self, cx: &mut Context<Self>) {
        if self.editing_disabled() {
            return;
        }
        let (Some(service), Some(source)) = (self.service.clone(), &self.source) else {
            return;
        };
        let encrypted = source.encrypted.clone();
        let source = source.label.clone();
        let password = self.import_password.read(cx).value().to_string();
        if password.is_empty() {
            self.error = Some("先输入备份密码，再解密检查。".into());
            cx.notify();
            return;
        }
        // The shared cache invalidates its single slot even when inspection fails.
        self.invalidate_preview();
        self.begin("正在解密并验证备份；不会写入实验配置", cx);
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    service
                        .inspect(encrypted.as_slice(), password)
                        .map_err(|error| error.to_string())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(preview) => {
                        this.inspected = Some(Inspected {
                            preview,
                            source,
                            inspected_at: Instant::now(),
                            expired: false,
                        });
                        this.notice =
                            Some("备份验证通过；预览仅在本进程保留 300 秒且只能恢复一次。".into());
                        this.expiry_task = Some(cx.spawn(async move |this, cx| {
                            cx.background_executor()
                                .timer(Duration::from_secs(300))
                                .await;
                            let _ = this.update(cx, |this, cx| {
                                if let Some(inspected) = &mut this.inspected {
                                    inspected.expired = true;
                                    if let Some(service) = &this.service {
                                        service.discard(&inspected.preview.preview_id);
                                    }
                                }
                                cx.notify();
                            });
                        }));
                    }
                    Err(error) => {
                        this.inspected = None;
                        this.error = Some(format!(
                            "解密检查失败：{error}。旧预览已失效，请检查密码或重新选择备份。"
                        ));
                    }
                }
                this.finish(cx);
            });
        }));
    }

    pub(super) fn request_restore(&mut self, cx: &mut Context<Self>) {
        if self.editing_disabled() {
            return;
        }
        if !self.preview_valid() {
            self.error = Some("备份预览已过期或已使用，请重新解密检查。".into());
            cx.notify();
            return;
        }
        if let Some(inspected) = &self.inspected {
            cx.emit(BackupEvent::RestoreRequested {
                preview_id: inspected.preview.preview_id.clone(),
            });
        }
    }

    pub(super) fn request_restart(&self, cx: &mut Context<Self>) {
        // Ordinary peer locking must not trap a prepared transaction.
        if self.pending && !self.busy() && !self.closing {
            cx.emit(BackupEvent::RestartRequested);
        }
    }

    pub(super) fn acknowledge(&mut self, cx: &mut Context<Self>) {
        if self.busy()
            || self.closing
            || self.pending
            || self.locked
            || (self.restored.is_none() && self.startup_error.is_none())
        {
            return;
        }
        let Some(service) = self.service.clone() else {
            return;
        };
        self.begin("正在确认启动恢复结果", cx);
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    service.acknowledge().map_err(|error| error.to_string())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.restored = None;
                        this.startup_error = None;
                        this.notice = Some("已确认恢复结果；加密回滚备份仍保留。".into());
                    }
                    Err(error) => {
                        this.error = Some(format!("确认恢复结果失败：{error}；恢复摘要保留。"))
                    }
                }
                this.finish(cx);
            });
        }));
    }
}
