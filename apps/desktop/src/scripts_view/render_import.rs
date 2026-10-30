use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
    input::Editor,
};
use gpui_kit::{Context, ParentElement as _, Styled as _, div};
use lanpath_client::remote::ScriptDialect;

use crate::script_dialects::is_selectable;

use super::{DIALECT_SCOPE_NOTICE, Operation, ScriptsView, render::section_title};

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
                "粘贴公开明文配置片段；规则与下载的脚本保存到本地集合。",
                cx,
            ))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(DIALECT_SCOPE_NOTICE),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_sm().child("方言"))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
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
                                    .label("Loon（暂不可选）")
                                    .selected(self.import_dialect == ScriptDialect::Loon)
                                    .disabled(true),
                            )
                            .child(
                                Button::new("import-dialect-qx")
                                    .label("QX")
                                    .selected(self.import_dialect == ScriptDialect::QuantumultX)
                                    .disabled(disabled)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.set_import_dialect(ScriptDialect::QuantumultX, cx)
                                    })),
                            )
                            .child(
                                Button::new("import-dialect-stash")
                                    .label("Stash（暂不可选）")
                                    .selected(self.import_dialect == ScriptDialect::Stash)
                                    .disabled(true),
                            ),
                    ),
            )
            .child(
                div()
                    .h_96()
                    .child(
                        Editor::new(&self.inputs.import_content)
                            .aria_label("Surge / QX 公开明文配置片段")
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
                    .disabled(disabled || !self.import_dirty || !is_selectable(self.import_dialect))
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
                        "导入内容保存到本地集合；重复导入会追加规则。下载的纯 JS 不会自动绑定到 hook。",
                    ),
            )
    }
}
