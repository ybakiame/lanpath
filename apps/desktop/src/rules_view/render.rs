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

use super::{Operation, RulesView, Section};

impl Render for RulesView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.locked || self.busy();
        let status = self
            .error
            .clone()
            .or_else(|| self.notice.clone())
            .unwrap_or_else(|| {
                if self.locked {
                    "实验核心运行时规则配置锁定；停止核心后编辑".into()
                } else if self.dirty {
                    "有未保存修改；保存后配置预览自动重新生成".into()
                } else {
                    "规则按顺序注入；规则集仅在被规则或 DNS 引用时进入运行配置".into()
                }
            });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child("规则与规则集")
                    .child(
                        Button::new("rules-save")
                            .label(match self.operation {
                                Some(Operation::Save) => "保存中…",
                                _ => "保存",
                            })
                            .primary()
                            .disabled(disabled || !self.dirty)
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    )
                    .child(
                        Button::new("rules-reload")
                            .label("重新载入")
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, window, cx| this.reload(window, cx))),
                    )
                    .child(
                        Button::new("rules-restore-builtins")
                            .label("恢复默认")
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, _, cx| this.restore_builtins(cx))),
                    ),
            )
            .child(
                div()
                    .id("rules-status")
                    .role(Role::Status)
                    .aria_label(status.clone())
                    .text_sm()
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
                    .gap_2()
                    .child(
                        Button::new("rules-section-rules")
                            .label("路由规则")
                            .selected(self.section == Section::Rules)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.set_section(Section::Rules, cx)),
                            ),
                    )
                    .child(
                        Button::new("rules-section-sets")
                            .label("规则集")
                            .selected(self.section == Section::RuleSets)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_section(Section::RuleSets, cx)
                            })),
                    )
                    .child(
                        Button::new("rules-section-market")
                            .label("市场")
                            .selected(self.section == Section::Market)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.set_section(Section::Market, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(match self.section {
                        Section::Rules => self.render_rules(disabled, cx).into_any_element(),
                        Section::RuleSets => self.render_sets(disabled, cx).into_any_element(),
                        Section::Market => self.render_market(disabled, cx).into_any_element(),
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
    cx: &Context<RulesView>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().child(title.into()))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(description.into()),
        )
}
