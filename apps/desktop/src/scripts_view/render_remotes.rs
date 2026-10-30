use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
    input::{Editor, Input, InputState},
    switch::Switch,
};
use gpui_kit::{Context, Entity, IntoElement, ParentElement as _, Styled as _, div};
use lanpath_client::{
    ArgKind,
    remote::{RemoteKind, ScriptDialect},
};

use super::{
    DIALECT_SCOPE_NOTICE, Operation, ScriptsView,
    editing::{dialect_label, kind_label},
    render::section_title,
};

impl ScriptsView {
    pub(super) fn render_remotes(&self, disabled: bool, cx: &Context<Self>) -> impl IntoElement {
        let selected = self.selected_resource();
        let form_visible = self.creating || selected.is_some();
        let dialect_allowed = self.resource_dialect_allowed();
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap_3()
            .p_2()
            .child(section_title(
                "远程资源",
                "订阅纯 JS 或公开明文片段。已启用资源按保存间隔自动更新；运行核心、业务操作或未保存草稿期间暂缓。“立即更新全部”强制更新已启用资源。缓存下次启动生效，纯 JS 不自动绑定 hook。",
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
                    .flex_wrap()
                    .items_start()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w_64()
                            .max_w_full()
                            .min_w_0()
                            .flex_shrink_0()
                            .gap_2()
                            .child(
                                Button::new("remote-add")
                                    .label("添加资源…")
                                    .disabled(disabled)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.start_new(window, cx)
                                    })),
                            )
                            .children(self.resources.iter().map(|resource| {
                                let select_name = resource.name.clone();
                                let toggle_name = resource.name.clone();
                                let next_enabled = !resource.enabled;
                                div()
                                    .flex()
                                    .items_center()
                                    .flex_wrap()
                                    .gap_1()
                                    .child(
                                        Button::new(format!("remote-select-{}", resource.name))
                                            .label(format!(
                                                "{} · {}",
                                                resource.name,
                                                kind_label(resource.kind)
                                            ))
                                            .selected(
                                                !self.creating
                                                    && self.selected.as_deref()
                                                        == Some(resource.name.as_str()),
                                            )
                                            .on_click(cx.listener(
                                                move |this, _, window, cx| {
                                                    this.select_resource(
                                                        select_name.clone(),
                                                        window,
                                                        cx,
                                                    )
                                                },
                                            )),
                                    )
                                    .child(
                                        Button::new(format!("remote-toggle-{}", resource.name))
                                            .label(if resource.enabled { "启用" } else { "停用" })
                                            .selected(resource.enabled)
                                            .disabled(disabled || self.resource_dirty)
                                            .on_click(cx.listener(
                                                move |this, _, window, cx| {
                                                    this.toggle_resource(
                                                        toggle_name.clone(),
                                                        next_enabled,
                                                        window,
                                                        cx,
                                                    )
                                                },
                                            )),
                                    )
                            }))
                            .children(self.resources.is_empty().then(|| {
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("暂无远程资源；创建后先更新本地集合，再启动核心以加载 cron 任务。")
                            })),
                    )
                    .children(form_visible.then(|| {
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .flex_basis(gpui_kit::rems(20.0))
                            .min_w_0()
                            .max_w_full()
                            .gap_3()
                            .child(if self.creating {
                                "添加远程资源"
                            } else {
                                "编辑远程资源"
                            })
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
                                            .on_click(cx.listener(|this, _, window, cx| {
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
                                                            .selected(
                                                                self.kind == RemoteKind::Script,
                                                            )
                                                            .disabled(disabled)
                                                            .on_click(cx.listener(
                                                                |this, _, _, cx| {
                                                                    this.set_kind(
                                                                        RemoteKind::Script,
                                                                        cx,
                                                                    )
                                                                },
                                                            )),
                                                    )
                                                    .child(
                                                        Button::new("remote-kind-snippet")
                                                            .label("配置片段")
                                                            .selected(
                                                                self.kind == RemoteKind::Snippet,
                                                            )
                                                            .disabled(disabled)
                                                            .on_click(cx.listener(
                                                                |this, _, _, cx| {
                                                                    this.set_kind(
                                                                        RemoteKind::Snippet,
                                                                        cx,
                                                                    )
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
                                                            .selected(
                                                                self.dialect
                                                                    == ScriptDialect::Surge,
                                                            )
                                                            .disabled(disabled)
                                                            .on_click(cx.listener(
                                                                |this, _, _, cx| {
                                                                    this.set_dialect(
                                                                        ScriptDialect::Surge,
                                                                        cx,
                                                                    )
                                                                },
                                                            )),
                                                    )
                                                    .child(
                                                        Button::new("remote-dialect-loon")
                                                            .label("Loon（暂不可选）")
                                                            .selected(
                                                                self.dialect == ScriptDialect::Loon,
                                                            )
                                                            .disabled(true),
                                                    )
                                                    .child(
                                                        Button::new("remote-dialect-qx")
                                                            .label("QX")
                                                            .selected(
                                                                self.dialect
                                                                    == ScriptDialect::QuantumultX,
                                                            )
                                                            .disabled(disabled)
                                                            .on_click(cx.listener(
                                                                |this, _, _, cx| {
                                                                    this.set_dialect(
                                                                        ScriptDialect::QuantumultX,
                                                                        cx,
                                                                    )
                                                                },
                                                            )),
                                                    )
                                                    .child(
                                                        Button::new("remote-dialect-stash")
                                                            .label("Stash（暂不可选）")
                                                            .selected(
                                                                self.dialect == ScriptDialect::Stash,
                                                            )
                                                            .disabled(true),
                                                    ),
                                            ),
                                    )
                                    .child(field("更新间隔（秒）", &self.inputs.interval, disabled))
                                    .child(
                                        Switch::new("remote-enabled")
                                            .label("启用")
                                            .checked(self.enabled)
                                            .disabled(disabled)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.set_enabled(!this.enabled, cx)
                                            })),
                                    ),
                            )
                            .children(self.resource_dialect_status().map(|status| {
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().warning)
                                    .child(status)
                            }))
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
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!(
                                        "当前：{} / {}{}",
                                        kind_label(self.kind),
                                        dialect_label(self.dialect),
                                        self.icon
                                            .as_deref()
                                            .map(|icon| format!("；图标 {icon}"))
                                            .unwrap_or_default()
                                    )),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        Button::new("remote-save")
                                            .label(match self.operation {
                                                Some(Operation::Save) => "保存中…",
                                                _ if self.creating => "添加",
                                                _ => "保存",
                                            })
                                            .primary()
                                            .disabled(
                                                disabled || !self.resource_dirty || !dialect_allowed,
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.save_resource(window, cx)
                                            })),
                                    )
                                    .child(
                                        Button::new("remote-delete")
                                            .label("删除")
                                            .danger()
                                            .disabled(
                                                disabled
                                                    || self.creating
                                                    || self.selected.is_none()
                                                    || self.resource_dirty,
                                            )
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.request_delete_resource(window, cx)
                                            })),
                                    ),
                            )
                    })),
            )
            .children(self.fetch_report.as_ref().map(|report| {
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(format!(
                        "上次更新：资源 {}，脚本 {}，重写 {}，任务 {}",
                        report.fetched, report.scripts, report.rewrites, report.tasks
                    ))
                    .children(report.warnings.iter().map(|warning| {
                        div()
                            .text_sm()
                            .text_color(cx.theme().warning)
                            .child(warning.clone())
                    }))
            }))
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
