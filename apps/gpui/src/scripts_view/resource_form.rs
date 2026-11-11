use super::{Operation, ScriptsView};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _,
    button::Button,
    input::{Editor, Input, InputState},
    select::Select,
    switch::Switch,
};
use gpui_kit::{App, Entity, IntoElement, ParentElement as _, Styled as _, Window, div};
use lanpath_client::{
    ArgKind,
    remote::{RemoteKind, ScriptDialect},
};

impl ScriptsView {
    pub(super) fn render_resource_fields(
        &self,
        disabled: bool,
        window: &Window,
        view: &Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(field("名称", &self.inputs.name, disabled))
                    .child(field("描述", &self.inputs.description, disabled)),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_end()
                    .gap_2()
                    .child(field("URL", &self.inputs.url, disabled))
                    .child(
                        Button::new("remote-detect")
                            .label(match self.operation {
                                Some(Operation::Detect) => "嗅探中…",
                                _ => "嗅探",
                            })
                            .disabled(disabled)
                            .on_click(window.listener_for(view, |this, _, window, cx| {
                                this.detect_resource(window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_sm().child("类型"))
                            .child(
                                div()
                                    .flex()
                                    .gap_1()
                                    .child(
                                        Button::new("remote-kind-script")
                                            .label("纯 JS")
                                            .selected(self.kind == RemoteKind::Script)
                                            .disabled(disabled)
                                            .on_click(window.listener_for(
                                                view,
                                                |this, _, _, cx| {
                                                    this.set_kind(RemoteKind::Script, cx)
                                                },
                                            )),
                                    )
                                    .child(
                                        Button::new("remote-kind-snippet")
                                            .label("配置片段")
                                            .selected(self.kind == RemoteKind::Snippet)
                                            .disabled(disabled)
                                            .on_click(window.listener_for(
                                                view,
                                                |this, _, _, cx| {
                                                    this.set_kind(RemoteKind::Snippet, cx)
                                                },
                                            )),
                                    ),
                            ),
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
                                        Button::new("remote-dialect-surge")
                                            .label("Surge")
                                            .selected(self.dialect == ScriptDialect::Surge)
                                            .disabled(disabled)
                                            .on_click(window.listener_for(
                                                view,
                                                |this, _, _, cx| {
                                                    this.set_dialect(ScriptDialect::Surge, cx)
                                                },
                                            )),
                                    )
                                    .child(
                                        Button::new("remote-dialect-loon")
                                            .label("Loon（暂不可选）")
                                            .selected(self.dialect == ScriptDialect::Loon)
                                            .disabled(true),
                                    )
                                    .child(
                                        Button::new("remote-dialect-qx")
                                            .label("QX")
                                            .selected(self.dialect == ScriptDialect::QuantumultX)
                                            .disabled(disabled)
                                            .on_click(window.listener_for(
                                                view,
                                                |this, _, _, cx| {
                                                    this.set_dialect(ScriptDialect::QuantumultX, cx)
                                                },
                                            )),
                                    )
                                    .child(
                                        Button::new("remote-dialect-stash")
                                            .label("Stash（暂不可选）")
                                            .selected(self.dialect == ScriptDialect::Stash)
                                            .disabled(true),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_sm().child("更新间隔"))
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        Input::new(&self.inputs.interval)
                                            .w_24()
                                            .aria_label("更新间隔")
                                            .disabled(disabled),
                                    )
                                    .child(
                                        Select::new(&self.inputs.interval_unit)
                                            .w_24()
                                            .accessibility_label("更新间隔单位")
                                            .disabled(disabled),
                                    ),
                            ),
                    )
                    .child(
                        Switch::new("remote-enabled")
                            .label("启用")
                            .accessibility_label("启用远程资源")
                            .checked(self.enabled)
                            .disabled(disabled)
                            .on_click(window.listener_for(view, |this, _, _, cx| {
                                this.set_enabled(!this.enabled, cx)
                            })),
                    ),
            )
            .children(
                self.resource_dialect_status()
                    .map(|status| div().text_sm().text_color(cx.theme().warning).child(status)),
            )
            .children((!self.arguments.is_empty()).then(|| {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().child("已声明模块参数"))
                    .children(self.arguments.iter().map(|argument| {
                        let options = if argument.kind == ArgKind::Select {
                            format!("；可选：{}", argument.options.join("、"))
                        } else {
                            String::new()
                        };
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{} · 默认 {}{}{}",
                                argument.key,
                                if argument.default_value.is_empty() {
                                    "空"
                                } else {
                                    argument.default_value.as_str()
                                },
                                options,
                                argument
                                    .description
                                    .as_deref()
                                    .map(|text| format!("；{text}"))
                                    .unwrap_or_default()
                            ))
                    }))
                    .child(
                        div()
                            .text_sm()
                            .child("参数值（每行 key=value，空值使用模块默认值）"),
                    )
                    .child(
                        div().h_32().child(
                            Editor::new(&self.inputs.arguments)
                                .aria_label("模块参数值")
                                .readonly(disabled)
                                .size_full(),
                        ),
                    )
            }))
            .child(field(
                "图标 URL（可选，留空移除）",
                &self.inputs.icon,
                disabled,
            ))
    }
}

fn field(label: impl Into<String>, input: &Entity<InputState>, disabled: bool) -> impl IntoElement {
    let label = label.into();
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap_1()
        .child(div().text_sm().child(label.clone()))
        .child(Input::new(input).aria_label(label).disabled(disabled))
}
