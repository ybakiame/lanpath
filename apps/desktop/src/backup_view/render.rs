use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _,
    button::Button,
    input::{Input, InputContentType},
    scroll::ScrollableElement as _,
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use lanpath_client::backup::BackupSummary;

use crate::appearance::AppearanceState;

use super::BackupView;

impl Render for BackupView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.editing_disabled();
        let status = if let Some(working) = self.working {
            working.to_owned()
        } else if let Some(error) = &self.error {
            error.clone()
        } else if self.pending {
            "恢复事务已准备；实验配置写入已锁定，重启后才会应用。".into()
        } else if self.closing {
            "正在关闭；等待已接受的文件选择、加密或解密工作完成。".into()
        } else if self.locked {
            "其他操作进行中；完成后可操作备份。".into()
        } else {
            "就绪；导出仅保存密文，导入先验证摘要再确认覆盖。".into()
        };
        let scope = self.root.as_ref().map_or_else(
            || "实验数据目录不可用；不会访问生产数据".into(),
            |root| format!("目标范围：仅当前实验数据目录 {}", root.display()),
        );
        let source = self
            .source
            .as_ref()
            .map_or("尚未选择备份".to_owned(), |source| {
                source.label.clone()
            });
        let masked = self
            .export_password
            .read(cx)
            .context_menu_capabilities()
            .is_masked();
        let mut body = div().flex().flex_col().flex_shrink_0().gap_6().min_w_0()
            .w_full().max_w(gpui_kit::rems(64.0))
            .child(div().flex().flex_col().gap_2()
                .child(crate::design::page_header("备份与恢复", "加密导出配置，或预览并恢复备份", div(), cx))
                .child(value("backup-root-scope", scope))
                .child(div().text_sm().text_color(cx.theme().muted_foreground)
                    .child("仅备份可移植配置与资源；不含核心、证书、日志或本机 app.toml 外观段。外观仅携带主题、强调色和纯黑模式，本机缩放保持不变。")))
            .child(div().flex().flex_col().gap_2()
                .child(value("backup-status", status).text_color(if self.error.is_some() { cx.theme().danger } else { cx.theme().muted_foreground }))
                .children(self.notice.as_ref().map(|notice| value("backup-notice", format!("操作结果：{notice}"))))
                .child(Button::new("backup-refresh").accessibility_id("backup-refresh").label("重新载入状态")
                    .disabled(self.busy() || self.closing || self.service.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx)))));
        if let Some(error) = &self.startup_error {
            body = body
                .child(value("backup-startup-error", error.clone()).text_color(cx.theme().danger));
            if self.restored.is_none() {
                body = body.child(
                    Button::new("backup-acknowledge")
                        .accessibility_id("backup-acknowledge")
                        .label("确认恢复失败")
                        .disabled(disabled)
                        .on_click(cx.listener(|this, _, _, cx| this.acknowledge(cx))),
                );
            }
        }
        if self.pending {
            body = body.child(div().flex().flex_col().gap_3()
                .child(value("backup-pending", "待重启恢复：当前配置尚未替换。重启将应用已验证的备份；失败时保留事务状态以便排查。"))
                .child(Button::new("backup-restart").accessibility_id("backup-restart").label("立即重启并恢复")
                    .disabled(self.busy() || self.closing)
                    .on_click(cx.listener(|this, _, _, cx| this.request_restart(cx)))));
        }
        if let Some(summary) = &self.restored {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().text_base().child("启动恢复结果"))
                    .child(summary_values("backup-restored", summary))
                    .child(
                        Button::new("backup-acknowledge")
                            .accessibility_id("backup-acknowledge")
                            .label("确认恢复结果")
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, _, cx| this.acknowledge(cx))),
                    ),
            );
        }
        if let Some(warning) = cx.global::<AppearanceState>().restore_warning() {
            body = body.child(
                value("backup-appearance-warning", warning.to_owned())
                    .text_color(cx.theme().warning),
            );
        }
        body = body.child(div().flex().flex_col().gap_3().min_w_0()
            .child(div().text_base().font_semibold().child("导出加密备份"))
            .child(div().flex().flex_col().gap_2()
                .child("导出密码")
                .child(Input::new(&self.export_password).id("backup-export-password")
                    .accessibility_id("backup-export-password").aria_label("导出密码")
                    .content_type(InputContentType::NewPassword).disabled(disabled)))
            .child(div().flex().flex_wrap().items_center().gap_2()
                .child(Button::new("backup-generate-password").accessibility_id("backup-generate-password").label("生成密码")
                    .disabled(disabled).on_click(cx.listener(|this, _, window, cx| this.generate_password(window, cx))))
                .child(Button::new("backup-reveal-password").accessibility_id("backup-reveal-password")
                    .label(if masked { "显示密码" } else { "隐藏密码" }).disabled(disabled)
                    .on_click(cx.listener(|this, _, window, cx| this.reveal_password(window, cx))))
                .child(Button::new("backup-copy-password").accessibility_id("backup-copy-password").label("复制密码")
                    .disabled(disabled || self.export_password.read(cx).value().is_empty())
                    .on_click(cx.listener(|this, _, _, cx| this.copy_password(cx))))
                .child(Button::new("backup-export").accessibility_id("backup-export").label("导出备份…")
                    .disabled(disabled).on_click(cx.listener(|this, _, _, cx| this.export(cx)))))
            .child(div().text_sm().text_color(cx.theme().muted_foreground)
                .child("密码至少 12 个字符，默认隐藏且不会出现在状态或报告中。密码丢失无法找回；请将备份保存在实验数据目录之外。已有文件不会被覆盖。")))
            .child(div().flex().flex_col().gap_3().min_w_0()
                .child(div().text_base().font_semibold().child("导入与回滚"))
                .child(div().flex().flex_wrap().items_center().gap_2()
                    .child(Button::new("backup-choose-file").accessibility_id("backup-choose-file").label("选择备份…")
                        .disabled(disabled).on_click(cx.listener(|this, _, _, cx| this.choose_source(cx))))
                    .child(Button::new("backup-load-rollback").accessibility_id("backup-load-rollback").label("载入最新回滚")
                        .disabled(disabled).on_click(cx.listener(|this, _, _, cx| this.load_previous(cx)))))
                .child(value("backup-source", source))
                .child(div().text_sm().text_color(cx.theme().muted_foreground)
                    .child("仅接受普通、无链接的 .zip.age 文件，密文不超过 16 MiB。选中后读取密文快照；磁盘文件随后变化不影响该来源，需重新选择才会载入新内容。回滚同样需要密码和预览。"))
                .child(div().flex().flex_col().gap_2()
                    .child("导入密码")
                    .child(Input::new(&self.import_password).id("backup-import-password")
                        .accessibility_id("backup-import-password").aria_label("导入密码")
                        .content_type(InputContentType::Password).disabled(disabled)))
                .child(Button::new("backup-inspect").accessibility_id("backup-inspect").label("解密检查")
                    .disabled(disabled || self.source.is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.inspect(cx)))));
        if let Some(inspected) = &self.inspected {
            let validity = if self.preview_valid() {
                "预览有效：自检查完成起 300 秒内且只能使用一次；修改密码或来源会使预览失效。"
            } else {
                "预览已过期，摘要仅供参考；请重新解密检查后恢复。"
            };
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .min_w_0()
                    .child(div().text_base().child("恢复预览"))
                    .child(value("backup-preview-source", inspected.source.clone()))
                    .child(value("backup-preview-validity", validity))
                    .child(summary_values("backup-preview", &inspected.preview.summary))
                    .child(
                        Button::new("backup-restore")
                            .accessibility_id("backup-restore")
                            .label("准备恢复…")
                            .disabled(disabled || !self.preview_valid())
                            .on_click(cx.listener(|this, _, _, cx| this.request_restore(cx))),
                    ),
            );
        }
        div()
            .id("backup-pane")
            .accessibility_id("backup-pane")
            .role(Role::Group)
            .aria_label("加密备份与事务恢复")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scrollbar()
            .child(body)
    }
}

fn value(
    id: impl Into<SharedString>,
    text: impl Into<SharedString>,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    let id = id.into();
    let text = text.into();
    div()
        .id(id.clone())
        .accessibility_id(id)
        .role(Role::Status)
        .aria_label(text.clone())
        .text_sm()
        .min_w_0()
        .w_full()
        .whitespace_normal()
        .child(text)
}

fn summary_values(prefix: &'static str, summary: &BackupSummary) -> impl IntoElement {
    let mut section = div()
        .id(prefix)
        .accessibility_id(prefix)
        .role(Role::Group)
        .aria_label("备份摘要")
        .flex()
        .flex_col()
        .gap_2()
        .min_w_0();
    for (key, text) in [
        ("version", format!("归档版本：{}", summary.version)),
        ("app-version", format!("应用版本：{}", summary.app_version)),
        ("platform", format!("来源平台：{}", summary.platform)),
        ("created-at", format!("创建时间：{}", summary.created_at)),
        (
            "subscriptions",
            format!("订阅数量：{}", summary.subscriptions),
        ),
        ("profiles", format!("配置数量：{}", summary.profiles)),
        ("files", format!("文件数量：{}", summary.files)),
        ("bytes", format!("展开大小：{} 字节", summary.bytes)),
    ] {
        section = section.child(value(format!("{prefix}-{key}"), text));
    }
    let appearance = summary.appearance.as_ref().map_or(
        "外观：归档不含外观偏好".into(),
        |appearance| {
            format!(
                "外观：主题 {} · 强调色 {} · 纯黑模式 {}；本机缩放保持不变",
                appearance.theme,
                appearance.accent,
                if appearance.pure_black {
                    "开启"
                } else {
                    "关闭"
                }
            )
        },
    );
    section = section.child(value(format!("{prefix}-appearance"), appearance));
    if summary.appearance.as_ref().is_some_and(|appearance| {
        !matches!(
            appearance.accent.as_str(),
            "iris" | "teal" | "rose" | "blue"
        )
    }) {
        section = section.child(value(
            format!("{prefix}-accent-warning"),
            "此备份的强调色暂不受 GPUI 支持；恢复时保留当前强调色，不影响其他兼容配置。",
        ));
    }
    section
}
