use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    input::Input,
    scroll::ScrollableElement as _,
    switch::Switch,
};
use gpui_kit::{Context, IntoElement, ParentElement as _, Styled as _, div};
use lanpath_client::local_override::{RuleAction, RuleMatchType};

use super::{
    RulesView,
    editing::{action_label, rule_label},
    render::{field, section_title},
};

impl RulesView {
    pub(super) fn render_rules(&self, disabled: bool, cx: &Context<Self>) -> impl IntoElement {
        let rules = self
            .draft
            .as_ref()
            .map(|draft| draft.singbox.rules.as_slice())
            .unwrap_or_default();
        let selected = self.selected_rule_ref();
        let referenced = self
            .draft
            .as_ref()
            .map(|draft| {
                draft
                    .custom_rule_sets
                    .iter()
                    .map(|set| set.tag.as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
            .join("、");
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_2()
            .child(section_title(
                "路由规则",
                "启用规则按列表顺序优先匹配；最终规则会覆盖结构化配置中的默认路由。",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("rule-add")
                            .label("新建规则…")
                            .disabled(disabled)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_new_rule(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("rule-delete")
                            .label("删除")
                            .danger()
                            .disabled(disabled || selected.is_none())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.request_delete_rule(window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .h_auto()
                    .min_w_0()
                    .max_h_48()
                    .overflow_y_scrollbar()
                    .children(rules.iter().enumerate().map(|(index, rule)| {
                        let select_id = rule.id.clone();
                        let toggle_id = rule.id.clone();
                        let up_id = rule.id.clone();
                        let down_id = rule.id.clone();
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new(format!("rule-select-{}", rule.id))
                                    .min_w_0()
                                    .max_w_full()
                                    .tooltip(format!(
                                        "{} · {}",
                                        rule_label(rule),
                                        action_label(&rule.action)
                                    ))
                                    .label(format!(
                                        "{}{} · {}",
                                        if rule.builtin { "内置 · " } else { "" },
                                        rule_label(rule),
                                        action_label(&rule.action)
                                    ))
                                    .selected(
                                        self.selected_rule.as_deref() == Some(rule.id.as_str()),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.select_rule(select_id.clone(), window, cx)
                                    })),
                            )
                            .child(
                                Switch::new(format!("rule-toggle-{}", rule.id))
                                    .label("启用")
                                    .checked(rule.enabled)
                                    .disabled(disabled)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.toggle_rule(&toggle_id, cx)
                                    })),
                            )
                            .child(
                                Button::new(format!("rule-up-{}", rule.id))
                                    .icon(IconName::ArrowUp)
                                    .accessibility_label("上移规则")
                                    .tooltip("上移")
                                    .disabled(disabled || index == 0)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.move_rule(&up_id, -1, cx)
                                    })),
                            )
                            .child(
                                Button::new(format!("rule-down-{}", rule.id))
                                    .icon(IconName::ArrowDown)
                                    .accessibility_label("下移规则")
                                    .tooltip("下移")
                                    .disabled(disabled || index + 1 == rules.len())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.move_rule(&down_id, 1, cx)
                                    })),
                            )
                    })),
            )
            .children(rules.is_empty().then(|| {
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("暂无路由规则；新建规则或恢复默认规则开始配置。")
            }))
            .children(selected.map(|rule| {
                let is_final = matches!(rule.match_type, RuleMatchType::Final);
                let is_outbound = matches!(rule.action, RuleAction::Outbound { .. });
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(section_title(
                        "规则详情",
                        "字段修改先进入草稿，点击页面顶部“保存”后持久化。",
                        cx,
                    ))
                    .child(field("名称", &self.inputs.rule_name, disabled))
                    .child(
                        ButtonGroup::new("rule-match-type")
                            .small()
                            .compact()
                            .outline()
                            .disabled(disabled)
                            .children(match_types().into_iter().map(|(kind, label)| {
                                let selected = rule.match_type == kind;
                                Button::new(format!("rule-match-{label}"))
                                    .label(label)
                                    .selected(selected)
                            }))
                            .on_click(cx.listener(|this, indexes: &Vec<usize>, _, cx| {
                                if let Some((kind, _)) =
                                    indexes.first().map(|index| match_types()[*index].clone())
                                {
                                    this.set_rule_match(kind, cx);
                                }
                            })),
                    )
                    .children((!is_final).then(|| {
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(field("匹配目标", &self.inputs.rule_target, disabled))
                            .children(matches!(rule.match_type, RuleMatchType::RuleSet).then(
                                || {
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(if referenced.is_empty() {
                                            "当前没有规则集；先在“规则集”或“市场”中添加。"
                                                .to_string()
                                        } else {
                                            format!("可用 tag：{referenced}；多选用英文逗号分隔。")
                                        })
                                },
                            ))
                    }))
                    .children(is_final.then(|| {
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("最终规则不需要匹配目标；首个启用的最终规则写入 route.final。")
                    }))
                    .child(
                        ButtonGroup::new("rule-action")
                            .small()
                            .compact()
                            .outline()
                            .disabled(disabled)
                            .children(
                                [
                                    (RuleAction::Proxy, "代理"),
                                    (RuleAction::Direct, "直连"),
                                    (RuleAction::Reject, "拒绝"),
                                    (RuleAction::Outbound { tag: String::new() }, "指定出站"),
                                ]
                                .into_iter()
                                .map(|(action, label)| {
                                    let selected = same_action_kind(&rule.action, &action);
                                    Button::new(format!("rule-action-{label}"))
                                        .label(label)
                                        .selected(selected)
                                }),
                            )
                            .on_click(cx.listener(|this, indexes: &Vec<usize>, _, cx| {
                                let action = match indexes.first() {
                                    Some(0) => RuleAction::Proxy,
                                    Some(1) => RuleAction::Direct,
                                    Some(2) => RuleAction::Reject,
                                    Some(3) => RuleAction::Outbound { tag: String::new() },
                                    _ => return,
                                };
                                this.set_rule_action(action, cx);
                            })),
                    )
                    .children(
                        is_outbound
                            .then(|| field("出站 tag", &self.inputs.rule_outbound, disabled)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_3()
                            .child(
                                Switch::new("rule-no-resolve")
                                    .label("跳过 DNS 解析")
                                    .checked(rule.advanced.no_resolve)
                                    .disabled(disabled)
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.toggle_no_resolve(cx)),
                                    ),
                            )
                            .child(
                                Switch::new("rule-invert")
                                    .label("反选")
                                    .checked(rule.advanced.invert)
                                    .disabled(disabled)
                                    .on_click(cx.listener(|this, _, _, cx| this.toggle_invert(cx))),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_sm().child("备注"))
                            .child(
                                Input::new(&self.inputs.rule_note)
                                    .aria_label("规则备注")
                                    .disabled(disabled),
                            ),
                    )
            }))
    }
}

fn match_types() -> Vec<(RuleMatchType, &'static str)> {
    vec![
        (RuleMatchType::Domain, "域名"),
        (RuleMatchType::DomainSuffix, "域名后缀"),
        (RuleMatchType::DomainKeyword, "域名关键词"),
        (RuleMatchType::IpCidr, "IP 段"),
        (RuleMatchType::SourceIpCidr, "源 IP"),
        (RuleMatchType::RuleSet, "规则集"),
        #[cfg(not(target_os = "android"))]
        (RuleMatchType::ProcessName, "进程名"),
        #[cfg(target_os = "android")]
        (RuleMatchType::AppPackage, "应用包名"),
        (RuleMatchType::Port, "端口"),
        (RuleMatchType::Final, "最终规则"),
    ]
}

fn same_action_kind(left: &RuleAction, right: &RuleAction) -> bool {
    matches!(
        (left, right),
        (RuleAction::Proxy, RuleAction::Proxy)
            | (RuleAction::Direct, RuleAction::Direct)
            | (RuleAction::Reject, RuleAction::Reject)
            | (RuleAction::Outbound { .. }, RuleAction::Outbound { .. })
    )
}
