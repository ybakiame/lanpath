use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
    input::{Input, InputState},
    scroll::ScrollableElement as _,
};
use gpui_kit::{
    Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};

use super::{ConfigSlicesView, Operation, Section};

impl Render for ConfigSlicesView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.locked || self.busy();
        let status = self
            .error
            .clone()
            .or_else(|| self.notice.clone())
            .unwrap_or_else(|| {
                if self.locked {
                    "实验核心运行时结构化配置锁定；停止核心后编辑".into()
                } else if self.dirty {
                    "有未保存修改；保存后配置预览自动重新生成".into()
                } else {
                    "结构化配置独立持久化；保存前执行完整跨切片校验".into()
                }
            });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .flex_shrink_0()
                    .gap_2()
                    .child("结构化配置")
                    .child(
                        Button::new("slices-save")
                            .label(match self.operation {
                                Some(Operation::Save) => "保存中…",
                                _ => "保存",
                            })
                            .primary()
                            .disabled(disabled || !self.dirty || !self.input_errors.is_empty())
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    )
                    .child(
                        Button::new("slices-reload")
                            .label("重新载入")
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, window, cx| this.reload(window, cx))),
                    ),
            )
            .child(
                div()
                    .id("slices-status")
                    .role(Role::Status)
                    .aria_label(status.clone())
                    .text_sm()
                    .flex_shrink_0()
                    .min_w_0()
                    .max_h_16()
                    .overflow_y_scrollbar()
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
                        Button::new("slices-section-dns")
                            .label("DNS")
                            .selected(self.section == Section::Dns)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_section(Section::Dns, window, cx)
                            })),
                    )
                    .child(
                        Button::new("slices-section-outbounds")
                            .label("出站与分组")
                            .selected(self.section == Section::Outbounds)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_section(Section::Outbounds, window, cx)
                            })),
                    )
                    .child(
                        Button::new("slices-section-route")
                            .label("默认路由与缓存")
                            .selected(self.section == Section::RouteCache)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_section(Section::RouteCache, window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_y_scrollbar()
                    .child(match self.section {
                        Section::Dns => self.render_dns(disabled, cx).into_any_element(),
                        Section::Outbounds => {
                            self.render_outbounds(disabled, cx).into_any_element()
                        }
                        Section::RouteCache => {
                            self.render_route_cache(disabled, cx).into_any_element()
                        }
                    }),
            )
    }
}

pub(super) fn field(
    label: impl Into<String>,
    input: &Entity<InputState>,
    disabled: bool,
) -> impl IntoElement {
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

pub(super) fn section_title(
    title: impl Into<String>,
    description: impl Into<String>,
    cx: &Context<ConfigSlicesView>,
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
