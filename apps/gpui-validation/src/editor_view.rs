use super::*;

impl ValidationView {
    pub(crate) fn render_editor_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = match self.active {
            EditorLanguage::Yaml => &self.yaml,
            EditorLanguage::Json => &self.json,
            EditorLanguage::JavaScript => &self.javascript,
        };
        let mode = if self.read_only {
            "只读"
        } else {
            "可编辑"
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("language-yaml")
                            .label("YAML")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.active = EditorLanguage::Yaml;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("language-json")
                            .label("JSON")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.active = EditorLanguage::Json;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("language-javascript")
                            .label("JavaScript")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.active = EditorLanguage::JavaScript;
                                cx.notify();
                            })),
                    )
                    .child(
                        Switch::new("editor-read-only")
                            .checked(self.read_only)
                            .label("只读")
                            .on_click(cx.listener(|this, checked, _, cx| {
                                this.read_only = *checked;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("validation-about")
                            .label("验证说明…")
                            .on_click(|_, window, cx| {
                                window.open_dialog(cx, |dialog, _, _| {
                                    dialog
                                        .title("本地验证范围")
                                        .child("编辑器与核心均为真实实现；核心仅使用隔离数据目录和 loopback mixed inbound，不修改系统代理或 TUN。")
                                });
                            }),
                    ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "当前：{} · {mode}；编辑器内 Ctrl-F 搜索，Esc 关闭搜索或 dialog",
                        self.active.label()
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(
                        Editor::new(editor)
                            .size_full()
                            .readonly(self.read_only)
                            .aria_label("Override 编辑器"),
                    ),
            )
    }
}
