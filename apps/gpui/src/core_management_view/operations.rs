use std::path::PathBuf;

use futures::{StreamExt as _, channel::mpsc};
use gpui_kit::component::{WindowExt as _, button::ButtonVariant};
use gpui_kit::{AppContext as _, Context, Window};
use lanpath_client::{core_management::ManagedCore, cores::CoreChannel, cores::CoreDownloadStage};
use lanpath_common::CoreType;

use super::{CoreManagementView, CoreSelectionChanged};

enum Mutation {
    Download(String),
    Select(PathBuf),
    Delete(PathBuf),
}

enum Update {
    Stage(CoreDownloadStage),
    Finished(
        Result<(), String>,
        Result<Vec<ManagedCore>, String>,
        Option<ManagedCore>,
    ),
}

impl CoreManagementView {
    pub(super) fn select_core_type(&mut self, kind: CoreType, cx: &mut Context<Self>) {
        if self.actions_disabled() || self.selected_core == kind {
            return;
        }
        self.selected_core = kind;
        self.remote.clear();
        self.remote_checked = false;
        self.error = None;
        self.refresh(cx);
    }

    pub(super) fn select_channel(&mut self, channel: CoreChannel, cx: &mut Context<Self>) {
        if self.selected_channel != channel {
            self.selected_channel = channel;
            cx.notify();
        }
    }

    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.actions_disabled() {
            return;
        }
        let Some(service) = self
            .service
            .as_ref()
            .map(|service| service.for_core(self.selected_core))
        else {
            return;
        };
        match service.list_local() {
            Ok(local) => {
                self.local = local;
                self.active_core = self.service.as_ref().and_then(|s| s.active_core().ok());
                self.error = None;
            }
            Err(error) => self.error = Some(format!("刷新本地核心失败：{error}；保留上次列表")),
        }
        cx.notify();
    }

    pub(super) fn check_remote(&mut self, cx: &mut Context<Self>) {
        if self.actions_disabled() {
            return;
        }
        let Some(service) = self
            .service
            .as_ref()
            .map(|service| service.for_core(self.selected_core))
        else {
            return;
        };
        self.working = true;
        self.error = None;
        self.stage = Some(format!("正在查询 {} 更新", self.selected_core));
        let core = self.core.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.block_on(service.list_remote_channels())
                        .and_then(|result| result.map_err(|error| error.to_string()))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.working = false;
                this.task = None;
                this.stage = None;
                match result {
                    Ok(mut remote) => {
                        remote
                            .retain(|channel| matches!(channel.channel(), "stable" | "prerelease"));
                        this.remote = remote;
                        this.remote_checked = true;
                        this.error = None;
                    }
                    Err(error) => {
                        this.error = Some(format!("检查更新失败：{error}；保留上次版本信息"))
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn download(&mut self, version: String, cx: &mut Context<Self>) {
        if self.selection_disabled()
            || !self.remote.iter().any(|remote| remote.version() == version)
        {
            return;
        }
        self.mutate(Mutation::Download(version), cx);
    }

    pub(super) fn select(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.selection_disabled() {
            return;
        }
        self.mutate(Mutation::Select(path), cx);
    }

    pub(super) fn confirm_delete(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selection_disabled() {
            return;
        }
        let Some(core) = self
            .local
            .iter()
            .find(|core| core.path() == path && !core.is_active())
        else {
            return;
        };
        let title = format!("删除核心 {}？", core.version());
        let description = format!("删除此版本目录及其文件，无法撤销。\n{}", path.display());
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            let path = path.clone();
            dialog
                .title(title.clone())
                .description(description.clone())
                .confirm()
                .ok_text("删除")
                .ok_variant(ButtonVariant::Danger)
                .cancel_text("取消")
                .on_ok(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        if !this.selection_disabled() {
                            this.mutate(Mutation::Delete(path.clone()), cx);
                        }
                    });
                    true
                })
        });
    }

    fn mutate(&mut self, mutation: Mutation, cx: &mut Context<Self>) {
        if self.selection_disabled() {
            return;
        }
        let Some(service) = self
            .service
            .as_ref()
            .map(|service| service.for_core(self.selected_core))
        else {
            return;
        };
        let active_service = self.service.clone();
        self.working = true;
        self.error = None;
        self.stage = Some(match &mutation {
            Mutation::Download(version) => format!("准备下载 {version}"),
            Mutation::Select(_) => "正在探测并保存选中核心".into(),
            Mutation::Delete(_) => "正在删除核心版本目录".into(),
        });
        let selection_changed = !matches!(&mutation, Mutation::Delete(_));
        let core = self.core.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let (sender, mut receiver) = mpsc::unbounded();
            let work = cx.background_spawn(async move {
                let result = core.mutate_core_inventory(|| match mutation {
                    Mutation::Download(version) => {
                        let progress = |stage| {
                            let _ = sender.unbounded_send(Update::Stage(stage));
                        };
                        core.block_on(service.download_with_progress(&version, &progress))
                            .and_then(|result| {
                                result.map(|_| ()).map_err(|error| error.to_string())
                            })
                    }
                    Mutation::Select(path) => {
                        service.select(&path).map_err(|error| error.to_string())
                    }
                    Mutation::Delete(path) => {
                        service.delete(&path).map_err(|error| error.to_string())
                    }
                });
                let local = service.list_local().map_err(|error| error.to_string());
                let active = active_service.and_then(|service| service.active_core().ok());
                let _ = sender.unbounded_send(Update::Finished(result, local, active));
            });
            while let Some(update) = receiver.next().await {
                let finished = matches!(&update, Update::Finished(_, _, _));
                let _ = this.update(cx, |this, cx| {
                    match update {
                        Update::Stage(stage) => this.stage = Some(stage_label(stage).into()),
                        Update::Finished(result, local, active) => {
                            this.working = false;
                            this.stage = None;
                            this.task = None;
                            let changed = selection_changed && result.is_ok();
                            this.error = result.err();
                            this.active_core = active;
                            match local {
                                Ok(local) => this.local = local,
                                Err(error) => {
                                    let previous = this
                                        .error
                                        .take()
                                        .map(|error| format!("{error}；"))
                                        .unwrap_or_default();
                                    this.error = Some(format!(
                                        "{previous}刷新列表失败：{error}；保留上次列表"
                                    ));
                                }
                            }
                            if changed {
                                cx.emit(CoreSelectionChanged);
                            }
                        }
                    }
                    cx.notify();
                });
                if finished {
                    break;
                }
            }
            work.await;
        }));
        cx.notify();
    }
}

fn stage_label(stage: CoreDownloadStage) -> &'static str {
    match stage {
        CoreDownloadStage::ResolvingRelease => "正在查询指定版本的发布资产",
        CoreDownloadStage::Downloading => "正在下载核心归档",
        CoreDownloadStage::Extracting => "正在解压核心二进制",
        CoreDownloadStage::Verifying => "正在探测并校验核心版本",
        CoreDownloadStage::Selecting => "正在保存选中核心",
    }
}
