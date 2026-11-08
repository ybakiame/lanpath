use gpui_kit::base::StyledExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _,
    button::{Button, ButtonVariants as _},
    input::{Input, InputState},
};
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement as _, Render, Styled as _, Window, div,
};

use crate::workspace::workspace_tabs;

use super::{Operation, RulesView, Section};

impl Render for RulesView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = self.locked || self.busy();
        let selected_index = match self.section {
            Section::Rules => 0,
            Section::RuleSets => 1,
            Section::Market => 2,
        };
        let status = self
            .error
            .clone()
            .or_else(|| self.notice.clone())
            .unwrap_or_else(|| {
                if self.locked {
                    "核心运行中，停止后可编辑".into()
                } else if self.dirty {
                    "有未保存修改".into()
                } else {
                    "当前规则与规则集已载入".into()
                }
            });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .gap_3()
            .child(workspace_tabs(
                "rules-sections",
                ["路由规则", "规则集", "市场"],
                selected_index,
                cx.listener(|this, index: &usize, _, cx| {
                    let section = match index {
                        0 => Some(Section::Rules),
                        1 => Some(Section::RuleSets),
                        2 => Some(Section::Market),
                        _ => None,
                    };
                    if let Some(section) = section {
                        this.set_section(section, cx);
                    }
                }),
                match self.section {
                    Section::Rules => self.render_rules(disabled, cx).into_any_element(),
                    Section::RuleSets => self.render_sets(disabled, cx).into_any_element(),
                    Section::Market => self.render_market(disabled, cx).into_any_element(),
                },
                cx,
            ))
            .child(crate::design::form_footer(
                "rules-status",
                status,
                self.error.is_some(),
                crate::design::toolbar()
                    .child(
                        Button::new("rules-save")
                            .label(match self.operation {
                                Some(Operation::Save) => "保存中…",
                                _ => "保存配置",
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
                cx,
            ))
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
        .child(div().font_semibold().child(title.into()))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(description.into()),
        )
}
