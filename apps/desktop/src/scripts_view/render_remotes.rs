use super::{
    ScriptsView,
    editing::{dialect_label, kind_label},
};
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    switch::Switch,
    tag::Tag,
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, div,
};

impl ScriptsView {
    pub(super) fn render_remotes(&self, disabled: bool, cx: &Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap_3()
            .children(self.resources.is_empty().then(|| {
                crate::design::empty_state(
                    gpui_kit::assets::IconName::Code,
                    "还没有远程资源",
                    "添加 JS 脚本或配置片段，更新后供下次启动使用。",
                    cx,
                )
            }))
            .children(self.resources.iter().map(|resource| {
                let edit = resource.name.clone();
                let toggle = edit.clone();
                let remove = edit.clone();
                let enabled = resource.enabled;
                let interval = format!(
                    "每 {} 更新 · 缓存下次启动生效",
                    super::interval::display(resource.update_interval_secs)
                );
                crate::design::surface(cx)
                    .id(format!("remote-resource-{}", resource.name))
                    .accessibility_id(format!("remote-resource-{}", resource.name))
                    .role(Role::Group)
                    .aria_label(format!(
                        "{} · {} · {}",
                        resource.name,
                        if enabled { "已启用" } else { "已停用" },
                        interval
                    ))
                    .p_4()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                div()
                                    .font_semibold()
                                    .flex_1()
                                    .min_w_0()
                                    .child(resource.name.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!(
                                        "{} · {}",
                                        kind_label(resource.kind),
                                        dialect_label(resource.dialect)
                                    )),
                            )
                            .child(if enabled {
                                Tag::primary().small().child("已启用")
                            } else {
                                Tag::secondary().small().child("已停用")
                            }),
                    )
                    .children(
                        resource
                            .description
                            .as_ref()
                            .filter(|text| !text.is_empty())
                            .map(|description| div().text_sm().child(description.clone())),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(resource.url.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .flex_1()
                                    .child(interval),
                            )
                            .child(
                                Switch::new(format!("remote-toggle-{}", resource.name))
                                    .label("启用")
                                    .checked(enabled)
                                    .disabled(disabled || self.resource_dirty)
                                    .on_click(cx.listener(
                                        move |this, checked: &bool, window, cx| {
                                            this.toggle_resource(
                                                toggle.clone(),
                                                *checked,
                                                window,
                                                cx,
                                            );
                                        },
                                    )),
                            )
                            .child(
                                Button::new(format!("remote-select-{}", resource.name))
                                    .small()
                                    .label("编辑…")
                                    .disabled(disabled)
                                    .accessibility_label(format!("编辑资源 {}", resource.name))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.select_resource(edit.clone(), window, cx)
                                    })),
                            )
                            .child(
                                Button::new(format!("remote-delete-{}", resource.name))
                                    .small()
                                    .ghost()
                                    .danger()
                                    .label("删除…")
                                    .disabled(disabled)
                                    .accessibility_label(format!("删除资源 {}", resource.name))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if this.locked || this.busy() || this.resource_dirty {
                                            return;
                                        }
                                        this.selected = Some(remove.clone());
                                        this.creating = false;
                                        this.sync_resource(window, cx);
                                        this.request_delete_resource(window, cx);
                                    })),
                            ),
                    )
            }))
            .children(self.fetch_report.as_ref().map(|report| {
                crate::design::surface(cx)
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_sm().child(format!(
                        "上次更新 · {} 个资源 · {} 个脚本 · {} 条重写 · {} 个任务",
                        report.fetched, report.scripts, report.rewrites, report.tasks
                    )))
                    .children(report.warnings.iter().map(|warning| {
                        div()
                            .text_sm()
                            .text_color(cx.theme().warning)
                            .child(warning.clone())
                    }))
            }))
    }
}
