use std::path::PathBuf;

use gpui_kit::component::WindowExt as _;
use gpui_kit::{Context, PathPromptOptions, Window};

use super::{DistributionEvent, DistributionView, Outcome};
use crate::distribution::{migration::MigrationService, native, updater::UpdateService};

impl DistributionView {
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.run(
            "读取迁移事务状态",
            |core| {
                MigrationService::new(core.data_root()?)
                    .status()
                    .map(Outcome::State)
                    .map_err(|error| error.to_string())
            },
            cx,
        );
    }

    pub(super) fn choose_source(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_disabled() {
            return;
        }
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("选择迁移源数据目录".into()),
        });
        self.working = Some("选择迁移源目录");
        self.sync_readonly(cx);
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = match prompt.await {
                Ok(Ok(Some(mut paths))) if paths.len() == 1 => Ok(paths.pop()),
                Ok(Ok(None)) => Ok(None),
                Ok(Ok(Some(_))) => Err("请选择一个目录".to_string()),
                Ok(Err(error)) => Err(format!("目录选择器失败：{error}")),
                Err(_) => Err("目录选择器未返回结果".into()),
            };
            let _ = cx.update(|window, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.working = None;
                    match result {
                        Ok(Some(path)) => this.source.update(cx, |input, cx| {
                            input.set_value(path.to_string_lossy().into_owned(), window, cx)
                        }),
                        Ok(None) => {}
                        Err(error) => this.error = Some(error),
                    }
                    this.sync_readonly(cx);
                    cx.notify();
                });
            });
        }));
        cx.notify();
    }

    pub(super) fn inspect(&mut self, cx: &mut Context<Self>) {
        if self.editing_disabled() {
            return;
        }
        let source = PathBuf::from(self.source.read(cx).value().trim());
        if !source.is_absolute() {
            self.error = Some("请选择完整的本地迁移源路径".into());
            cx.notify();
            return;
        }
        let kind = self.source_kind;
        let options = self.options;
        self.run(
            "检查源与目标；生成无副作用预览",
            move |core| {
                MigrationService::new(core.data_root()?)
                    .preview(&source, kind, options)
                    .map(Outcome::Preview)
                    .map_err(|error| error.to_string())
            },
            cx,
        );
    }

    pub(super) fn confirm_prepare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_disabled() {
            return;
        }
        let Some(preview) = self.preview.clone() else {
            return;
        };
        let description = format!(
            "将复制 {} 个文件（{} 字节），目标冲突类别 {} 项。源目录保持不变；先保存目标回滚副本，再于重启时替换所选类别。未选择的历史、CA 及其他类别不会导入；核心和系统授权不迁移。",
            preview.files.len(),
            preview.bytes,
            preview.conflicts.len()
        );
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            let preview = preview.clone();
            dialog.title("准备导入并保留回退副本？").description(description.clone()).confirm()
                .ok_text("准备事务").cancel_text("取消")
                .on_ok(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        if this.editing_disabled() { return; }
                        let preview = preview.clone();
                        this.run("停止自有控制并准备迁移事务", move |core| {
                            let _stopped = core.stop_for_migration()?;
                            let prepared = MigrationService::new(core.data_root()?).prepare(preview)
                                .map_err(|error| error.to_string())?;
                            Ok(Outcome::Prepared(format!("事务 {} 已准备：{} 个文件、{} 字节。重启后应用；启动系统接管保持关闭。", prepared.id, prepared.files, prepared.bytes)))
                        }, cx);
                    });
                    true
                })
        });
    }

    pub(super) fn request_rollback(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.unavailable() {
            return;
        }
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog.title("重启时回退导入？")
                .description("将恢复本次导入前所选类别的目标副本；导入后的这些类别修改会被替换。这里只保存回退意图，重启前不替换已打开的存储。源目录不变。")
                .confirm().ok_text("保存回退意图").cancel_text("取消")
                .on_ok(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| this.run("保存可重复恢复的回退意图", |core| {
                        MigrationService::new(core.data_root()?).request_rollback()
                            .map(|()| Outcome::RollbackRequested).map_err(|error| error.to_string())
                    }, cx));
                    true
                })
        });
    }

    pub(super) fn acknowledge(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.unavailable() || self.pending {
            return;
        }
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog.title("确认迁移结果并删除回退副本？")
                .description("此操作只删除本应用保存的本次迁移副本；之后不能再用此事务回退。不会删除源数据。")
                .confirm().ok_text("确认并删除副本").cancel_text("保留副本")
                .on_ok(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| this.run("确认迁移结果", |core| {
                        let service = MigrationService::new(core.data_root()?);
                        service.acknowledge().map_err(|error| error.to_string())?;
                        service.status().map(Outcome::State).map_err(|error| error.to_string())
                    }, cx));
                    true
                })
        });
    }

    pub(super) fn check_update(&mut self, cx: &mut Context<Self>) {
        if self.editing_disabled() {
            return;
        }
        let Some(config) = self.saved_update_request_policy(cx) else {
            return;
        };
        self.run(
            "读取独立 GPUI 更新源并验证签名",
            move |core| {
                let service = UpdateService::new(core.data_root()?, &config)
                    .map_err(|error| error.to_string())?;
                core.block_on(service.check())?
                    .map(Outcome::Checked)
                    .map_err(|error| error.to_string())
            },
            cx,
        );
    }

    pub(super) fn download_update(&mut self, cx: &mut Context<Self>) {
        if self.editing_disabled() {
            return;
        }
        let Some(available) = self.available.clone() else {
            return;
        };
        let Some(config) = self.saved_update_request_policy(cx) else {
            return;
        };
        self.run(
            "下载并验证更新；不替换运行程序",
            move |core| {
                let service = UpdateService::new(core.data_root()?, &config)
                    .map_err(|error| error.to_string())?;
                core.block_on(service.download(&available))?
                    .map(Outcome::Downloaded)
                    .map_err(|error| error.to_string())
            },
            cx,
        );
    }

    fn saved_update_request_policy(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<lanpath_client::ClientConfig> {
        // Capture saved settings before accepting background work, never an editor draft.
        match self.core.business().and_then(|business| {
            business
                .saved_request_policy()
                .map(|(_, config)| config)
                .map_err(|error| error.to_string())
        }) {
            Ok(config) => Some(config),
            Err(error) => {
                self.error = Some(error);
                self.notice = None;
                cx.notify();
                None
            }
        }
    }

    pub(super) fn open_target(&mut self, url: Option<String>, cx: &mut Context<Self>) {
        self.run(
            "请求系统打开所选目标",
            move |core| {
                match url {
                    Some(url) => native::open_url(&url),
                    None => native::open_path(core.data_root()?.path()),
                }
                .map(|()| Outcome::Opened)
                .map_err(|error| error.to_string())
            },
            cx,
        );
    }

    pub(super) fn discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_disabled() {
            return;
        }
        self.source
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.preview = None;
        self.draft = false;
        self.notice = None;
        self.error = None;
        cx.notify();
    }

    pub(super) fn restart(&self, cx: &mut Context<Self>) {
        if self.pending && !self.unavailable() {
            cx.emit(DistributionEvent::RestartRequested);
        }
    }
}
