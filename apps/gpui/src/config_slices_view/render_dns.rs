use gpui_kit::component::{
    Disableable as _, Selectable as _, Sizable as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    scroll::ScrollableElement as _,
    switch::Switch,
};
use gpui_kit::{Context, IntoElement, ParentElement as _, Styled as _, div};
use lanpath_client::config_slices::{
    DnsMatchType, DnsMode, DnsRuleAction, DnsServerType, DnsStrategy,
};

use super::{
    ConfigSlicesView, Operation,
    render::{field, section_title},
};

impl ConfigSlicesView {
    pub(super) fn render_dns(&self, disabled: bool, cx: &Context<Self>) -> impl IntoElement {
        let Some(slices) = self.draft.as_ref() else {
            return div().child("DNS 配置不可用");
        };
        let follow_system = slices.dns == self.builtin_dns;
        let server = self
            .selected_dns_server
            .and_then(|index| slices.dns.servers.get(index));
        let rule = self
            .selected_dns_rule
            .and_then(|index| slices.dns.rules.get(index));
        let server_kind = server.map(|server| server.server_type);
        let rule_match = rule.map(|rule| rule.match_type);
        let rule_action = rule.map(|rule| rule.action);
        div()
            .flex()
            .flex_col()
            .gap_3()
            .pb_4()
            .child(section_title(
                "DNS 模式",
                "跟随系统使用内置 DNS；任意结构化修改会切换为 DNS 接管。",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        ButtonGroup::new("dns-mode")
                            .small()
                            .compact()
                            .outline()
                            .disabled(disabled)
                            .children([
                                Button::new("dns-follow")
                                    .label("跟随系统")
                                    .selected(follow_system),
                                Button::new("dns-takeover")
                                    .label("DNS 接管")
                                    .selected(!follow_system),
                            ])
                            .on_click(cx.listener(|this, indexes: &Vec<usize>, _, cx| {
                                let mode = if indexes.first() == Some(&1) {
                                    DnsMode::Takeover
                                } else {
                                    DnsMode::FollowSystem
                                };
                                this.set_dns_mode(mode, cx);
                            })),
                    )
                    .child(
                        Switch::new("dns-reverse")
                            .label("反向映射")
                            .checked(slices.dns.reverse_mapping)
                            .disabled(disabled)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.toggle_reverse_mapping(cx)),
                            ),
                    ),
            )
            .child(
                div().flex().flex_wrap().gap_2().child(
                    ButtonGroup::new("dns-strategy")
                        .small()
                        .compact()
                        .outline()
                        .disabled(disabled)
                        .children(dns_strategies().into_iter().map(|(strategy, label)| {
                            Button::new(format!("dns-strategy-{}", strategy.as_str()))
                                .label(label)
                                .selected(slices.dns.strategy == strategy)
                        }))
                        .on_click(cx.listener(|this, indexes: &Vec<usize>, _, cx| {
                            if let Some((strategy, _)) =
                                indexes.first().map(|index| dns_strategies()[*index])
                            {
                                this.set_dns_strategy(strategy, cx);
                            }
                        })),
                ),
            )
            .child(field(
                "默认 DNS 服务器 tag",
                &self.inputs.dns_final,
                disabled,
            ))
            .child(section_title(
                "DNS 服务器",
                "支持 UDP/TLS/HTTPS/QUIC/H3/Local/FakeIP；仅启用项参与生成。",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .min_w_0()
                    .h_auto()
                    .max_h_32()
                    .overflow_y_scrollbar()
                    .gap_2()
                    .children(slices.dns.servers.iter().enumerate().map(|(index, item)| {
                        Button::new(("dns-server", index))
                            .min_w_0()
                            .max_w_full()
                            .tooltip(item.tag.clone())
                            .label(format!(
                                "{}{}",
                                if item.enabled { "" } else { "停用 · " },
                                item.tag
                            ))
                            .selected(self.selected_dns_server == Some(index))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_dns_server(index, window, cx)
                            }))
                    }))
                    .child(
                        Button::new("dns-server-add")
                            .label("新增")
                            .disabled(disabled)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.add_dns_server(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("dns-server-remove")
                            .label("删除")
                            .danger()
                            .disabled(disabled || server.is_none())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.remove_dns_server(window, cx)
                            })),
                    ),
            )
            .children(server.map(|server| {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                Switch::new("dns-server-enabled")
                                    .label("启用此服务器")
                                    .checked(server.enabled)
                                    .disabled(disabled)
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.toggle_dns_server(cx)),
                                    ),
                            )
                            .child(
                                Button::new("dns-server-probe")
                                    .label(match self.operation {
                                        Some(Operation::Probe) => "探测中…",
                                        _ => "探测",
                                    })
                                    .disabled(
                                        disabled
                                            || !server.enabled
                                            || !server.server_type.uses_server(),
                                    )
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.probe_selected_dns(cx)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(field("Tag", &self.inputs.dns_server_tag, disabled))
                            .child(field("显示名称", &self.inputs.dns_server_name, disabled)),
                    )
                    .child(
                        ButtonGroup::new("dns-server-type")
                            .small()
                            .compact()
                            .outline()
                            .disabled(disabled)
                            .children(dns_server_types().into_iter().map(|(kind, label)| {
                                Button::new(format!("dns-server-type-{}", kind.as_str()))
                                    .label(label)
                                    .selected(server_kind == Some(kind))
                            }))
                            .on_click(cx.listener(|this, indexes: &Vec<usize>, _, cx| {
                                if let Some((kind, _)) =
                                    indexes.first().map(|index| dns_server_types()[*index])
                                {
                                    this.set_dns_server_type(kind, cx);
                                }
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(field(
                                "服务器地址",
                                &self.inputs.dns_server_address,
                                disabled || !server.server_type.uses_server(),
                            ))
                            .child(field(
                                "端口",
                                &self.inputs.dns_server_port,
                                disabled || !server.server_type.uses_server(),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(field(
                                "FakeIP IPv4",
                                &self.inputs.dns_server_inet4,
                                disabled || server.server_type != DnsServerType::Fakeip,
                            ))
                            .child(field(
                                "FakeIP IPv6",
                                &self.inputs.dns_server_inet6,
                                disabled || server.server_type != DnsServerType::Fakeip,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(field(
                                "Detour",
                                &self.inputs.dns_server_detour,
                                disabled || server.server_type == DnsServerType::Fakeip,
                            ))
                            .child(field(
                                "Domain resolver",
                                &self.inputs.dns_server_resolver,
                                disabled || server.server_type == DnsServerType::Fakeip,
                            )),
                    )
                    .child(
                        ButtonGroup::new("dns-server-strategy")
                            .small()
                            .compact()
                            .outline()
                            .disabled(disabled)
                            .children(optional_dns_strategies().into_iter().map(
                                |(strategy, label)| {
                                    Button::new(format!("dns-server-strategy-{label}"))
                                        .label(label)
                                        .selected(server.strategy == strategy)
                                },
                            ))
                            .on_click(cx.listener(|this, indexes: &Vec<usize>, _, cx| {
                                if let Some((strategy, _)) = indexes
                                    .first()
                                    .map(|index| optional_dns_strategies()[*index])
                                {
                                    this.set_dns_server_strategy(strategy, cx);
                                }
                            })),
                    )
            }))
            .child(section_title(
                "DNS 规则",
                "匹配目标支持逗号分隔；route 需要服务器 tag，predefined 需要合法 rcode。",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .min_w_0()
                    .h_auto()
                    .max_h_32()
                    .overflow_y_scrollbar()
                    .gap_2()
                    .children(slices.dns.rules.iter().enumerate().map(|(index, item)| {
                        Button::new(("dns-rule", index))
                            .label(format!(
                                "{}规则 {}",
                                if item.enabled { "" } else { "停用 · " },
                                index + 1
                            ))
                            .selected(self.selected_dns_rule == Some(index))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_dns_rule(index, window, cx)
                            }))
                    }))
                    .child(
                        Button::new("dns-rule-add")
                            .label("新增")
                            .disabled(disabled)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.add_dns_rule(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("dns-rule-remove")
                            .label("删除")
                            .danger()
                            .disabled(disabled || rule.is_none())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.remove_dns_rule(window, cx)),
                            ),
                    ),
            )
            .children(rule.map(|rule| {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        Switch::new("dns-rule-enabled")
                            .label("启用此规则")
                            .checked(rule.enabled)
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_dns_rule(cx))),
                    )
                    .child(
                        ButtonGroup::new("dns-match-type")
                            .small()
                            .compact()
                            .outline()
                            .disabled(disabled)
                            .children(dns_match_types().into_iter().map(|(kind, label)| {
                                Button::new(format!("dns-match-{}", kind.field()))
                                    .label(label)
                                    .selected(rule_match == Some(kind))
                            }))
                            .on_click(cx.listener(|this, indexes: &Vec<usize>, _, cx| {
                                if let Some((kind, _)) =
                                    indexes.first().map(|index| dns_match_types()[*index])
                                {
                                    this.set_dns_rule_match(kind, cx);
                                }
                            })),
                    )
                    .child(field("匹配目标", &self.inputs.dns_rule_target, disabled))
                    .child(
                        ButtonGroup::new("dns-rule-action")
                            .small()
                            .compact()
                            .outline()
                            .disabled(disabled)
                            .children(dns_actions().into_iter().map(|(action, label)| {
                                Button::new(format!("dns-action-{label}"))
                                    .label(label)
                                    .selected(rule_action == Some(action))
                            }))
                            .on_click(cx.listener(|this, indexes: &Vec<usize>, _, cx| {
                                if let Some((action, _)) =
                                    indexes.first().map(|index| dns_actions()[*index])
                                {
                                    this.set_dns_rule_action(action, cx);
                                }
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(field(
                                "目标 DNS tag",
                                &self.inputs.dns_rule_server,
                                disabled || rule.action != DnsRuleAction::Route,
                            ))
                            .child(field(
                                "预定义响应码",
                                &self.inputs.dns_rule_rcode,
                                disabled || rule.action != DnsRuleAction::Predefined,
                            )),
                    )
            }))
    }

    pub(super) fn set_dns_server_strategy(
        &mut self,
        strategy: Option<DnsStrategy>,
        cx: &mut Context<Self>,
    ) {
        if let Some(server) = self.current_dns_server_mut() {
            server.strategy = strategy;
        }
        self.finish_edit(cx);
    }
}

fn dns_strategies() -> [(DnsStrategy, &'static str); 4] {
    [
        (DnsStrategy::PreferIpv4, "优先 IPv4"),
        (DnsStrategy::PreferIpv6, "优先 IPv6"),
        (DnsStrategy::Ipv4Only, "仅 IPv4"),
        (DnsStrategy::Ipv6Only, "仅 IPv6"),
    ]
}
fn optional_dns_strategies() -> [(Option<DnsStrategy>, &'static str); 5] {
    [
        (None, "继承全局"),
        (Some(DnsStrategy::PreferIpv4), "优先 IPv4"),
        (Some(DnsStrategy::PreferIpv6), "优先 IPv6"),
        (Some(DnsStrategy::Ipv4Only), "仅 IPv4"),
        (Some(DnsStrategy::Ipv6Only), "仅 IPv6"),
    ]
}
fn dns_server_types() -> [(DnsServerType, &'static str); 7] {
    [
        (DnsServerType::Udp, "UDP"),
        (DnsServerType::Tls, "TLS"),
        (DnsServerType::Https, "HTTPS"),
        (DnsServerType::Quic, "QUIC"),
        (DnsServerType::H3, "H3"),
        (DnsServerType::Local, "Local"),
        (DnsServerType::Fakeip, "FakeIP"),
    ]
}
fn dns_match_types() -> [(DnsMatchType, &'static str); 6] {
    [
        (DnsMatchType::Domain, "Domain"),
        (DnsMatchType::DomainSuffix, "Suffix"),
        (DnsMatchType::DomainKeyword, "Keyword"),
        (DnsMatchType::RuleSet, "Rule set"),
        (DnsMatchType::QueryType, "Query type"),
        (DnsMatchType::ClashMode, "Clash mode"),
    ]
}
fn dns_actions() -> [(DnsRuleAction, &'static str); 3] {
    [
        (DnsRuleAction::Route, "Route"),
        (DnsRuleAction::Predefined, "Predefined"),
        (DnsRuleAction::Reject, "Reject"),
    ]
}
