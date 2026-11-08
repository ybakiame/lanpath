use super::super::ScriptsView;
use super::icon_image;
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    tag::Tag,
};
use gpui_kit::{Context, IntoElement, ParentElement as _, Styled as _, div, img};
impl ScriptsView {
    pub(in crate::scripts_view) fn render_local(
        &self,
        disabled: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_2()
            .children(self.local.load_error.as_ref().map(|error| {
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(format!("本地资源读取失败：{error}"))
            }))
            .children(
                (self.local.rows.is_empty() && self.local.load_error.is_none()).then(|| {
                    crate::design::empty_state(
                        gpui_kit::assets::IconName::FileCode,
                        "暂无本地资源",
                        "添加配置片段，并为片段中的引用绑定配套 JS。",
                        cx,
                    )
                }),
            )
            .children(self.local.rows.iter().map(|document| {
                let source = document.source();
                let edit_id = document.id.clone();
                let toggle_id = document.id.clone();
                let delete_id = document.id.clone();
                let enabled = source.enabled;
                crate::design::surface(cx)
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .children(
                                source
                                    .icon
                                    .as_ref()
                                    .and_then(icon_image)
                                    .map(|image| img(image).size_10().rounded_md()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(div().font_semibold().child(source.name.clone()))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!(
                                                "{} · {} 个 JS · {} 条重写 · {} 个任务{}",
                                                super::super::editing::dialect_label(
                                                    source.dialect
                                                ),
                                                document.assets().len(),
                                                document.cached.rewrites.len(),
                                                document.cached.task_scripts.len(),
                                                if source.content.is_none() {
                                                    " · 历史解析结果"
                                                } else {
                                                    ""
                                                }
                                            )),
                                    ),
                            )
                            .child(if enabled {
                                Tag::primary().small().child("已启用")
                            } else {
                                Tag::secondary().small().child("已停用")
                            }),
                    )
                    .child(
                        crate::design::toolbar()
                            .child(
                                Button::new(format!("local-edit-{}", document.id))
                                    .accessibility_id(format!("local-edit-{}", document.id))
                                    .outline()
                                    .label("编辑资源…")
                                    .disabled(disabled)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.edit_local(Some(edit_id.clone()), window, cx)
                                    })),
                            )
                            .child(
                                Button::new(format!("local-toggle-{}", document.id))
                                    .label(if enabled { "停用" } else { "启用" })
                                    .disabled(disabled)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.toggle_local(toggle_id.clone(), !enabled, window, cx)
                                    })),
                            )
                            .child(div().flex_1())
                            .child(
                                Button::new(format!("local-delete-{}", document.id))
                                    .ghost()
                                    .label("删除")
                                    .disabled(disabled)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.delete_local(delete_id.clone(), window, cx)
                                    })),
                            ),
                    )
            }))
    }
}
