use super::super::ScriptsView;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    input::{Editor, Input},
    switch::Switch,
    tab::{Tab, TabBar},
};
use gpui_kit::{App, Entity, IntoElement, ParentElement as _, Styled as _, Window, div, img};
use lanpath_client::remote::ScriptDialect;
impl ScriptsView {
    pub(super) fn local_fields(
        &self,
        disabled: bool,
        window: &mut Window,
        view: &Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let header = div()
            .flex()
            .items_center()
            .gap_3()
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&self.local.name)
                        .aria_label("本地资源名称")
                        .disabled(disabled),
                ),
            )
            .child(
                Switch::new("local-enabled")
                    .label("启用资源")
                    .checked(self.local.enabled)
                    .disabled(disabled)
                    .on_click(window.listener_for(view, |this, value: &bool, _, cx| {
                        this.local.enabled = *value;
                        this.local_changed(cx);
                    })),
            );
        let tabs = TabBar::new("local-form-tabs")
            .segmented()
            .selected_index(self.local.form_tab)
            .children(
                [
                    "配置片段".to_owned(),
                    format!("关联 JS · {}", self.local.assets.len()),
                    "配套图标".into(),
                ]
                .map(|label| Tab::new().label(label)),
            )
            .on_click(window.listener_for(view, |this, index: &usize, _, cx| {
                this.local.form_tab = *index;
                cx.notify();
            }));
        let body = match self.local.form_tab {
            1 => self
                .local_script_fields(disabled, window, view, cx)
                .into_any_element(),
            2 => self
                .local_icon_fields(disabled, window, view, cx)
                .into_any_element(),
            _ => self
                .local_config_fields(disabled, window, view, cx)
                .into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(header)
            .child(tabs)
            .child(body)
    }
    fn local_config_fields(
        &self,
        disabled: bool,
        window: &mut Window,
        view: &Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let dialects = ButtonGroup::new("local-dialect")
            .small()
            .compact()
            .outline()
            .disabled(disabled)
            .children(
                [
                    (ScriptDialect::Surge, "Surge"),
                    (ScriptDialect::QuantumultX, "QX"),
                ]
                .map(|(dialect, label)| {
                    Button::new(format!("local-dialect-{label}"))
                        .label(label)
                        .selected(self.local.dialect == dialect)
                }),
            )
            .on_click(
                window.listener_for(view, |this, indices: &Vec<usize>, _, cx| {
                    if let Some(dialect) = indices.first().and_then(|index| {
                        [ScriptDialect::Surge, ScriptDialect::QuantumultX].get(*index)
                    }) {
                        this.local.dialect = *dialect;
                        this.local.advisories.clear();
                        this.local_changed(cx);
                    }
                }),
            );
        div().flex().flex_col().gap_3()
            .children(self.local.original.as_ref().is_some_and(|document| document.source().content.is_none()).then(|| {
                div().text_sm().text_color(cx.theme().muted_foreground)
                    .child("历史导入未保存原始片段。留空可保留既有解析规则并编辑 JS；填写片段后将按新内容重新解析。")
            }))
            .child(dialects)
            .child(div().h(window.viewport_size().height * 0.36).child(Editor::new(&self.local.content).aria_label("本地配置片段").readonly(disabled).size_full()))
            .child(Button::new("local-identify").accessibility_id("local-identify").outline().label("识别关联脚本").disabled(disabled)
                .on_click(window.listener_for(view, |this, _, window, cx| this.identify_local_assets(window, cx))))
            .child(div().text_xs().text_color(cx.theme().muted_foreground)
                .child("按片段中的脚本文件名或 URL 绑定本地 JS；保存与识别不会联网下载。Surge / QX 支持范围与远程片段一致。"))
    }
    fn local_script_fields(
        &self,
        disabled: bool,
        window: &mut Window,
        view: &Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let index = self
            .local
            .asset_index
            .min(self.local.assets.len().saturating_sub(1));
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().flex().flex_wrap().gap_2().children(
                self.local.assets.iter().enumerate().map(|(index, _)| {
                    Button::new(format!("local-asset-{index}"))
                        .small()
                        .label(format!("JS {}", index + 1))
                        .selected(self.local.asset_index == index)
                        .on_click(window.listener_for(view, move |this, _, _, cx| {
                            this.local.asset_index = index;
                            cx.notify();
                        }))
                }),
            ))
            .children(self.local.assets.get(index).map(|asset| {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(asset.reference.clone()),
                    )
                    .child(
                        div().h(window.viewport_size().height * 0.36).child(
                            Editor::new(&asset.editor)
                                .aria_label(format!("关联 JS {}", index + 1))
                                .readonly(disabled)
                                .size_full(),
                        ),
                    )
                    .child(
                        Button::new("local-asset-file")
                            .outline()
                            .label("从 JS 文件读取…")
                            .disabled(disabled)
                            .on_click(window.listener_for(view, move |this, _, window, cx| {
                                this.choose_local_file(Some(index), window, cx)
                            })),
                    )
            }))
            .children(self.local.assets.is_empty().then(|| {
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("请先在配置片段中识别脚本引用；仅包含重写或主机名的片段无需 JS。")
            }))
    }
    fn local_icon_fields(
        &self,
        disabled: bool,
        window: &mut Window,
        view: &Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .children(self.local.icon.as_ref().map(|icon| {
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .children(super::icon_image(icon).map(|image| img(image).size_16()))
                    .child(icon.name.clone())
            }))
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("PNG、JPEG 或 WebP，最大 512 KiB；图标内容随资源保存。"),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("local-icon-file")
                            .outline()
                            .label(if self.local.icon.is_some() {
                                "替换图标…"
                            } else {
                                "选择图标…"
                            })
                            .disabled(disabled)
                            .on_click(window.listener_for(view, |this, _, window, cx| {
                                this.choose_local_file(None, window, cx)
                            })),
                    )
                    .children(self.local.icon.is_some().then(|| {
                        Button::new("local-icon-remove")
                            .ghost()
                            .label("移除图标")
                            .disabled(disabled)
                            .on_click(window.listener_for(view, |this, _, _, cx| {
                                this.local.icon = None;
                                this.local_changed(cx);
                            }))
                    })),
            )
    }
}
