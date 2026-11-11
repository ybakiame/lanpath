use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _,
    button::Button,
    checkbox::Checkbox,
    input::Input,
    radio::{Radio, RadioGroup},
    scroll::ScrollableElement as _,
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

use super::DistributionView;
use crate::distribution::migration::{CaScope, MigrationOptions, MigrationRecovery, SourceKind};

impl DistributionView {
    fn category(
        &self,
        id: &'static str,
        label: &'static str,
        field: fn(&mut MigrationOptions) -> &mut bool,
        cx: &mut Context<Self>,
    ) -> Checkbox {
        let mut options = self.options;
        Checkbox::new(id)
            .label(label)
            .checked(*field(&mut options))
            .disabled(self.editing_disabled())
            .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                if this.editing_disabled() {
                    return;
                }
                *field(&mut this.options) = *checked;
                this.changed(cx);
            }))
    }
}

impl Render for DistributionView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.unavailable();
        let editing = self.editing_disabled();
        let mode = match self.core.data_mode() {
            Ok(crate::data_root::DataMode::Portable) => "便携目录",
            Ok(crate::data_root::DataMode::Standard) => "标准目录",
            Err(_) => "目录不可用",
        };
        let status = if let Some(working) = self.working {
            working.to_owned()
        } else if let Some(error) = &self.error {
            error.clone()
        } else if let Some(notice) = &self.notice {
            notice.clone()
        } else {
            match self.recovery {
                MigrationRecovery::None => "没有待处理的迁移事务。".into(),
                MigrationRecovery::Pending => "事务已准备，重启后应用；当前业务写入已锁定。".into(),
                MigrationRecovery::RollbackPending => "回退意图已保存，重启后恢复原副本。".into(),
                MigrationRecovery::Applied => {
                    "导入已应用，回退副本仍保留。请验证后再确认结果。".into()
                }
                MigrationRecovery::RolledBack => "已恢复导入前副本；源目录未修改。".into(),
            }
        };
        let can_rollback = matches!(
            self.recovery,
            MigrationRecovery::Applied | MigrationRecovery::Pending
        );
        let can_acknowledge = matches!(
            self.recovery,
            MigrationRecovery::Applied | MigrationRecovery::RolledBack
        );
        let preview = self.preview.as_ref().map(|preview| {
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_3()
                .border_1()
                .border_color(cx.theme().border)
                .rounded_lg()
                .child(format!(
                    "预览：{} 个文件 · {} 字节",
                    preview.files.len(),
                    preview.bytes
                ))
                .child(format!("来源：{}", preview.source.display()))
                .child(format!(
                    "将替换的目标类别：{}",
                    if preview.conflicts.is_empty() {
                        "无现有冲突".into()
                    } else {
                        preview.conflicts.join("、")
                    }
                ))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("不迁移：{}", preview.omitted.join("；"))),
                )
                .child(
                    Button::new("distribution-prepare")
                        .label("确认并准备导入…")
                        .disabled(editing)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.confirm_prepare(window, cx)),
                        ),
                )
        });
        let available = self.available.as_ref().map(|available| {
            let release_url = available.release_url.clone();
            div()
                .flex()
                .items_center()
                .flex_wrap()
                .gap_2()
                .child(format!("已验证签名：{}", available.version))
                .child(
                    Button::new("distribution-download")
                        .label("下载并校验")
                        .disabled(disabled || self.pending)
                        .on_click(cx.listener(|this, _, _, cx| this.download_update(cx))),
                )
                .child(
                    Button::new("distribution-release")
                        .label("打开发行说明")
                        .disabled(disabled)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_target(Some(release_url.clone()), cx)
                        })),
                )
        });
        let downloaded = self.downloaded.as_ref().map(|downloaded| {
            div().flex().flex_col().gap_2()
                .child(format!("已校验 ZIP：{}", downloaded.path().display()))
                .child(div().text_sm().text_color(cx.theme().muted_foreground)
                    .child("便携与标准目录使用同一 ZIP。更新前退出程序，再从外部新版目录执行载荷更新；保留原有便携标记、data 目录及标准目录数据，不会启动安装器。"))
        });
        div().flex().flex_col().flex_1().min_h_0().min_w_0().overflow_y_scrollbar()
            .child(div().flex().flex_col().gap_6().p_4()
                .child(div().flex().flex_col().gap_2()
                    .child(crate::design::page_header("迁移与更新", "导入旧客户端数据，检查独立更新", div(), cx))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("Lanpath GPUI 与 Tauri / Android 分离。导入显式选择来源和类别；不读取旧目录作为运行数据，也不自动接管旧核心、系统代理、TUN 或证书信任。"))
                    .child(format!("数据目录模式：{mode} · 仅 ZIP 发行"))
                    .child(Button::new("distribution-open-data").label("打开 GPUI 数据目录").disabled(disabled)
                        .on_click(cx.listener(|this, _, _, cx| this.open_target(None, cx)))))
                .child(crate::design::surface(cx).flex().flex_col().gap_3().p_4()
                    .child(div().font_semibold().child("从旧客户端导入"))
                    .child(RadioGroup::horizontal("distribution-source-kind")
                        .selected_index(Some(if self.source_kind == SourceKind::Tauri { 0 } else { 1 }))
                        .disabled(editing)
                        .child(Radio::new("distribution-source-tauri").label("Tauri"))
                        .child(Radio::new("distribution-source-experimental").label("LanpathExperimental"))
                        .on_change(cx.listener(|this, ix: &usize, _, cx| {
                            if this.editing_disabled() { return; }
                            this.source_kind = if *ix == 0 { SourceKind::Tauri } else { SourceKind::Experimental };
                            this.changed(cx);
                        })))
                    .child(Input::new(&self.source).id("distribution-source").aria_label("迁移源数据目录").disabled(editing))
                    .child(Button::new("distribution-pick-source").label("选择目录…").disabled(editing)
                        .on_click(cx.listener(|this, _, window, cx| this.choose_source(window, cx))))
                    .child(self.category("distribution-category-config", "配置、订阅、规则和脚本", |o| &mut o.configuration, cx))
                    .child(self.category("distribution-category-history", "流量历史（独立 SQLite 快照）", |o| &mut o.history, cx))
                    .child(self.category("distribution-category-appearance", "兼容的外观偏好", |o| &mut o.appearance, cx))
                    .child(Checkbox::new("distribution-category-ca").label("导入 CA 身份和私钥（不导入系统信任）")
                        .checked(self.options.ca == CaScope::IdentityAndPrivateKey).disabled(editing)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            if this.editing_disabled() { return; }
                            this.options.ca = if *checked { CaScope::IdentityAndPrivateKey } else { CaScope::Exclude };
                            this.changed(cx);
                        })))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("准备前须停止源程序写入；本应用先恢复自有控制。预览过期、源或目标变化会拒绝导入，不会强行覆盖。私钥和回退副本只留在本机数据目录。"))
                    .child(div().flex().items_center().flex_wrap().gap_2()
                        .child(Button::new("distribution-inspect").label("检查并预览").disabled(editing)
                            .on_click(cx.listener(|this, _, _, cx| this.inspect(cx))))
                        .child(Button::new("distribution-discard").label("清除迁移草稿").disabled(editing || !self.draft)
                            .on_click(cx.listener(|this, _, window, cx| this.discard(window, cx)))))
                    .children(preview))
                .child(crate::design::surface(cx).flex().flex_col().gap_3().p_4()
                    .child(div().font_semibold().child("迁移结果与回退"))
                    .child(div().flex().items_center().flex_wrap().gap_2()
                        .child(Button::new("distribution-restart").label("重启完成事务").disabled(disabled || !self.pending)
                            .on_click(cx.listener(|this, _, _, cx| this.restart(cx))))
                        .child(Button::new("distribution-rollback").label("回退导入…").disabled(disabled || !can_rollback)
                            .on_click(cx.listener(|this, _, window, cx| this.request_rollback(window, cx))))
                        .child(Button::new("distribution-acknowledge").label("确认结果并删除回退副本…").disabled(disabled || !can_acknowledge)
                            .on_click(cx.listener(|this, _, window, cx| this.acknowledge(window, cx))))))
                .child(crate::design::surface(cx).flex().flex_col().gap_3().p_4()
                    .child(div().font_semibold().child("GPUI 独立签名更新"))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground)
                        .child("只接受独立产品、通道、架构和版本的 Ed25519 签名；下载再校验长度与 SHA-256。更新源尚未发布或无法访问会明确报错，不冒充“已是最新”。"))
                    .child(Button::new("distribution-check-update").label("检查签名更新").disabled(disabled || self.pending)
                        .on_click(cx.listener(|this, _, _, cx| this.check_update(cx))))
                    .children(available).children(downloaded))
                .child(div().id("distribution-status").role(Role::Status).aria_label(status.clone()).text_sm()
                    .text_color(if self.error.is_some() { cx.theme().danger } else { cx.theme().muted_foreground })
                    .child(status)))
    }
}
