use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
    input::Editor,
};
use gpui_kit::{Context, ParentElement as _, Styled as _, div};
use lanpath_client::remote::ScriptDialect;

use super::{Operation, ScriptsView, render::section_title};

impl ScriptsView {
    pub(super) fn render_import(
        &self,
        disabled: bool,
        cx: &Context<Self>,
    ) -> impl gpui_kit::IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_2()
            .child(section_title(
                "配置导入",
                "粘贴 Surge 或 Loon 的 rewrite、script、task 和 MITM hostname 片段；脚本源会下载后合并进本地导入缓存。",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_sm().child("方言"))
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .child(
                                Button::new("import-dialect-surge")
                                    .label("Surge")
                                    .selected(self.import_dialect == ScriptDialect::Surge)
                                    .disabled(disabled)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.set_import_dialect(ScriptDialect::Surge, cx)
                                    })),
                            )
                            .child(
                                Button::new("import-dialect-loon")
                                    .label("Loon")
                                    .selected(self.import_dialect == ScriptDialect::Loon)
                                    .disabled(disabled)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.set_import_dialect(ScriptDialect::Loon, cx)
                                    })),
                            ),
                    ),
            )
            .child(
                div()
                    .h_96()
                    .child(
                        Editor::new(&self.inputs.import_content)
                            .aria_label("Surge 或 Loon 配置片段")
                            .readonly(disabled)
                            .size_full(),
                    ),
            )
            .child(
                Button::new("import-submit")
                    .label(if matches!(self.operation, Some(Operation::Import)) {
                        "导入中…"
                    } else {
                        "导入配置"
                    })
                    .primary()
                    .disabled(disabled || !self.import_dirty)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.import_config(window, cx)
                    })),
            )
            .children(self.import_summary.as_ref().map(|summary| {
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(format!(
                        "导入结果：重写 {}，脚本 {}，任务 {}，主机名 {}",
                        summary.rewrites, summary.scripts, summary.tasks, summary.hostnames
                    ))
                    .children(summary.meta.name.as_ref().map(|name| {
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "识别为：{}{}",
                                name,
                                summary
                                    .meta
                                    .desc
                                    .as_deref()
                                    .map(|description| format!(" — {description}"))
                                    .unwrap_or_default()
                            ))
                    }))
                    .children(summary.warnings.iter().map(|warning| {
                        div()
                            .text_sm()
                            .text_color(cx.theme().warning)
                            .child(warning.clone())
                    }))
            }))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "导入采用追加语义；重复导入会追加规则。当前实验版不启动 MITM，重写和 hook 脚本会保留到后续 MITM 迁移。",
                    ),
            )
    }
}
