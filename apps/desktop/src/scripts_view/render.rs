use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, button::Button,
    scroll::ScrollableElement as _,
};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

use super::{Operation, ScriptsView, Section};

impl Render for ScriptsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editing_disabled = self.locked || self.busy();
        let status = if self.operation == Some(Operation::AutomaticFetch) {
            "正在自动更新到期资源；仅刷新缓存，当前脚本、任务和路由保持不变…".into()
        } else {
            self.error
                .clone()
                .or_else(|| self.notice.clone())
                .unwrap_or_else(|| {
                    if self.dirty() {
                        "有未提交内容；自动更新暂缓，保存资源或完成导入后才能启动核心".into()
                    } else if !self.automatic_eligible {
                        "生命周期、其他业务操作或草稿未完成，自动更新暂缓；缓存仅在下次启动核心时加载".into()
                    } else if self.running {
                        "核心运行中仅自动刷新安全缓存；当前脚本、定时任务与路由保持不变，下次启动时加载更新".into()
                    } else {
                        "已启用资源按保存的间隔自动更新；每 5 秒检查到期，缓存在下次启动核心时加载"
                            .into()
                    }
                })
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap_3()
            .child(crate::design::page_header(
                "脚本与片段",
                "导入、更新与调试订阅脚本",
                crate::design::toolbar()
                    .child(
                        Button::new("scripts-reload")
                            .label("重新载入")
                            .disabled(editing_disabled)
                            .on_click(cx.listener(|this, _, window, cx| this.reload(window, cx))),
                    )
                    .child(
                        Button::new("scripts-fetch")
                            .label(match self.operation {
                                Some(Operation::Fetch) => "更新中…",
                                Some(Operation::AutomaticFetch) => "自动更新中…",
                                _ => "立即更新全部",
                            })
                            .disabled(editing_disabled || self.dirty() || self.resources.is_empty())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.fetch_resources(window, cx)),
                            ),
                    ),
                cx,
            ))
            .child(
                div()
                    .id("scripts-status")
                    .role(Role::Status)
                    .aria_label(status.clone())
                    .text_sm()
                    .flex_shrink_0()
                    .min_w_0()
                    .max_h_16()
                    .overflow_y_scroll()
                    .text_color(if self.error.is_some() {
                        cx.theme().danger
                    } else {
                        cx.theme().muted_foreground
                    })
                    .child(status),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .flex_shrink_0()
                    .gap_2()
                    .child(
                        Button::new("scripts-section-remotes")
                            .label("远程资源")
                            .selected(self.section == Section::Remotes)
                            .on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.set_section(Section::Remotes, cx)
                                }),
                            ),
                    )
                    .child(
                        Button::new("scripts-section-tasks")
                            .label("定时任务")
                            .selected(self.section == Section::Tasks)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.set_section(Section::Tasks, cx)),
                            ),
                    )
                    .child(
                        Button::new("scripts-section-import")
                            .label("配置导入")
                            .selected(self.section == Section::Import)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.set_section(Section::Import, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_y_scrollbar()
                    .child(match self.section {
                        Section::Remotes => {
                            self.render_remotes(editing_disabled, cx).into_any_element()
                        }
                        Section::Tasks => self.render_tasks(cx).into_any_element(),
                        Section::Import => {
                            self.render_import(editing_disabled, cx).into_any_element()
                        }
                    }),
            )
    }
}

pub(super) fn section_title(
    title: impl Into<String>,
    description: impl Into<String>,
    cx: &Context<ScriptsView>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().font_semibold().child(title.into()))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(description.into()),
        )
}
