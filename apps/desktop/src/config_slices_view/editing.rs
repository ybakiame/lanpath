use gpui_kit::{
    Context, Entity, Window,
    component::{WindowExt as _, input::InputState},
};
use lanpath_client::config_slices::{
    CustomOutbound, DnsMatchType, DnsMode, DnsRule, DnsRuleAction, DnsServer, DnsServerType,
    DnsStrategy, OutboundProtocol, VlessOutbound,
};
use uuid::Uuid;

use super::{ConfigSlicesView, OutboundKind, inputs::FormField};

impl ConfigSlicesView {
    pub(super) fn update_field(&mut self, field: FormField, value: String, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        if matches!(
            field,
            FormField::OutName
                | FormField::OutServer
                | FormField::OutPort
                | FormField::OutCredential
                | FormField::OutExtra
                | FormField::OutAlterId
                | FormField::OutTlsName
                | FormField::OutAlpn
                | FormField::OutTransportKind
                | FormField::OutTransportPath
                | FormField::OutTransportHost
                | FormField::OutMembers
                | FormField::OutDefault
                | FormField::OutUrl
                | FormField::OutInterval
                | FormField::OutTolerance
                | FormField::OutUp
                | FormField::OutDown
                | FormField::OutObfsType
                | FormField::OutObfsPassword
        ) {
            self.update_outbound_field(field, value);
            self.finish_edit(cx);
            return;
        }
        let dns_server_index = self.selected_dns_server;
        let dns_rule_index = self.selected_dns_rule;
        let Some(slices) = self.draft.as_mut() else {
            return;
        };
        match field {
            FormField::DnsFinal => slices.dns.final_tag = value,
            FormField::DnsServerTag => {
                if let Some(server) =
                    dns_server_index.and_then(|index| slices.dns.servers.get_mut(index))
                {
                    let old = std::mem::replace(&mut server.tag, value.clone());
                    if slices.dns.final_tag == old {
                        slices.dns.final_tag.clone_from(&value);
                    }
                    for rule in &mut slices.dns.rules {
                        if rule.server_tag == old {
                            rule.server_tag.clone_from(&value);
                        }
                    }
                }
            }
            FormField::DnsServerName => {
                with_server(slices, dns_server_index, |server| server.name = value)
            }
            FormField::DnsServerAddress => {
                with_server(slices, dns_server_index, |server| server.server = value)
            }
            FormField::DnsServerPort => match parse_optional_u16(&value) {
                Ok(port) => {
                    self.input_errors.remove(&field);
                    with_server(slices, dns_server_index, |server| server.server_port = port);
                }
                Err(()) => {
                    self.input_errors.insert(field);
                }
            },
            FormField::DnsServerInet4 => with_server(slices, dns_server_index, |server| {
                server.inet4_range = value
            }),
            FormField::DnsServerInet6 => with_server(slices, dns_server_index, |server| {
                server.inet6_range = value
            }),
            FormField::DnsServerDetour => {
                with_server(slices, dns_server_index, |server| server.detour = value)
            }
            FormField::DnsServerResolver => with_server(slices, dns_server_index, |server| {
                server.domain_resolver = value
            }),
            FormField::DnsRuleTarget => {
                with_rule(slices, dns_rule_index, |rule| rule.target = value)
            }
            FormField::DnsRuleServer => {
                with_rule(slices, dns_rule_index, |rule| rule.server_tag = value)
            }
            FormField::DnsRuleRcode => with_rule(slices, dns_rule_index, |rule| rule.rcode = value),
            FormField::RouteFinal => slices.route.final_tag = value,
            FormField::RouteResolver => slices.route.resolver.server = value,
            FormField::CachePath => slices.experimental.cache_file.path = value,
            FormField::CacheId => slices.experimental.cache_file.cache_id = value,
            _ => {}
        }
        self.finish_edit(cx);
    }

    pub(super) fn set_dns_mode(&mut self, mode: DnsMode, cx: &mut Context<Self>) {
        let Some(slices) = self.draft.as_mut() else {
            return;
        };
        slices.dns = match mode {
            DnsMode::FollowSystem => self.builtin_dns.clone(),
            DnsMode::Takeover => {
                let mut dns = slices.dns.clone();
                dns.mode = DnsMode::Takeover;
                dns
            }
        };
        self.selected_dns_server = (!slices.dns.servers.is_empty()).then_some(0);
        self.selected_dns_rule = (!slices.dns.rules.is_empty()).then_some(0);
        self.finish_edit(cx);
    }

    pub(super) fn set_dns_strategy(&mut self, strategy: DnsStrategy, cx: &mut Context<Self>) {
        if let Some(slices) = self.draft.as_mut() {
            slices.dns.strategy = strategy;
        }
        self.finish_edit(cx);
    }

    pub(super) fn toggle_reverse_mapping(&mut self, cx: &mut Context<Self>) {
        if let Some(slices) = self.draft.as_mut() {
            slices.dns.reverse_mapping = !slices.dns.reverse_mapping;
        }
        self.finish_edit(cx);
    }

    pub(super) fn select_dns_server(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.reject_invalid_input(cx) {
            return;
        }
        self.selected_dns_server = Some(index);
        self.sync_inputs(window, cx);
        cx.notify();
    }

    pub(super) fn add_dns_server(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() || self.reject_invalid_input(cx) {
            return;
        }
        let Some(slices) = self.draft.as_mut() else {
            return;
        };
        let mut sequence = 1;
        while slices
            .dns
            .servers
            .iter()
            .any(|server| server.tag == format!("dns-{sequence}"))
        {
            sequence += 1;
        }
        slices.dns.servers.push(DnsServer {
            tag: format!("dns-{sequence}"),
            name: format!("DNS {sequence}"),
            ..DnsServer::default()
        });
        self.selected_dns_server = Some(slices.dns.servers.len() - 1);
        self.sync_inputs(window, cx);
        self.finish_edit(cx);
    }

    pub(super) fn remove_dns_server(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        let Some(index) = self.selected_dns_server else {
            return;
        };
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .title("删除 DNS 服务器？")
                .description("引用该 tag 的 final 和规则需要同时修正后才能保存。")
                .confirm()
                .ok_text("删除")
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let _ =
                        view.update(cx, |this, cx| this.remove_dns_server_now(index, window, cx));
                    true
                })
        });
    }

    fn remove_dns_server_now(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(slices) = self.draft.as_mut() else {
            return;
        };
        if index >= slices.dns.servers.len() {
            return;
        }
        slices.dns.servers.remove(index);
        self.selected_dns_server =
            (!slices.dns.servers.is_empty()).then_some(index.min(slices.dns.servers.len() - 1));
        self.sync_inputs(window, cx);
        self.finish_edit(cx);
    }

    pub(super) fn set_dns_server_type(&mut self, kind: DnsServerType, cx: &mut Context<Self>) {
        if let Some(server) = self.current_dns_server_mut() {
            server.server_type = kind;
        }
        self.finish_edit(cx);
    }

    pub(super) fn toggle_dns_server(&mut self, cx: &mut Context<Self>) {
        if let Some(server) = self.current_dns_server_mut() {
            server.enabled = !server.enabled;
        }
        self.finish_edit(cx);
    }

    pub(super) fn select_dns_rule(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.reject_invalid_input(cx) {
            return;
        }
        self.selected_dns_rule = Some(index);
        self.sync_inputs(window, cx);
        cx.notify();
    }

    pub(super) fn add_dns_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() || self.reject_invalid_input(cx) {
            return;
        }
        let Some(slices) = self.draft.as_mut() else {
            return;
        };
        let server_tag = slices
            .dns
            .servers
            .iter()
            .find(|server| server.enabled)
            .map(|server| server.tag.clone())
            .unwrap_or_default();
        slices.dns.rules.push(DnsRule {
            id: Uuid::new_v4().to_string(),
            enabled: true,
            match_type: DnsMatchType::Domain,
            target: String::new(),
            server_tag,
            action: DnsRuleAction::Route,
            rcode: String::new(),
        });
        self.selected_dns_rule = Some(slices.dns.rules.len() - 1);
        self.sync_inputs(window, cx);
        self.finish_edit(cx);
    }

    pub(super) fn remove_dns_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        let Some(index) = self.selected_dns_rule else {
            return;
        };
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .title("删除 DNS 规则？")
                .description("该操作会从当前草稿移除所选规则。")
                .confirm()
                .ok_text("删除")
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| this.remove_dns_rule_now(index, window, cx));
                    true
                })
        });
    }

    fn remove_dns_rule_now(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(slices) = self.draft.as_mut() else {
            return;
        };
        if index >= slices.dns.rules.len() {
            return;
        }
        slices.dns.rules.remove(index);
        self.selected_dns_rule =
            (!slices.dns.rules.is_empty()).then_some(index.min(slices.dns.rules.len() - 1));
        self.sync_inputs(window, cx);
        self.finish_edit(cx);
    }

    pub(super) fn set_dns_rule_match(&mut self, kind: DnsMatchType, cx: &mut Context<Self>) {
        if let Some(rule) = self.current_dns_rule_mut() {
            rule.match_type = kind;
        }
        self.finish_edit(cx);
    }

    pub(super) fn set_dns_rule_action(&mut self, action: DnsRuleAction, cx: &mut Context<Self>) {
        if let Some(rule) = self.current_dns_rule_mut() {
            rule.action = action;
        }
        self.finish_edit(cx);
    }

    pub(super) fn toggle_dns_rule(&mut self, cx: &mut Context<Self>) {
        if let Some(rule) = self.current_dns_rule_mut() {
            rule.enabled = !rule.enabled;
        }
        self.finish_edit(cx);
    }

    pub(super) fn set_route_strategy(
        &mut self,
        strategy: Option<DnsStrategy>,
        cx: &mut Context<Self>,
    ) {
        if let Some(slices) = self.draft.as_mut() {
            slices.route.resolver.strategy = strategy;
        }
        self.finish_edit(cx);
    }

    pub(super) fn toggle_cache_enabled(&mut self, cx: &mut Context<Self>) {
        if let Some(slices) = self.draft.as_mut() {
            slices.experimental.cache_file.enabled = !slices.experimental.cache_file.enabled;
        }
        self.finish_edit(cx);
    }

    pub(super) fn toggle_cache_fakeip(&mut self, cx: &mut Context<Self>) {
        if let Some(slices) = self.draft.as_mut() {
            slices.experimental.cache_file.store_fakeip =
                !slices.experimental.cache_file.store_fakeip;
        }
        self.finish_edit(cx);
    }

    pub(super) fn add_outbound(
        &mut self,
        kind: OutboundKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.locked || self.busy() || self.reject_invalid_input(cx) {
            return;
        }
        let Some(slices) = self.draft.as_mut() else {
            return;
        };
        let mut sequence = 1;
        let name = loop {
            let candidate = format!("节点 {sequence}");
            let tag = lanpath_client::config_slices::outbound_tag(&candidate);
            if slices.outbounds.items.iter().all(|item| item.tag() != tag) {
                break candidate;
            }
            sequence += 1;
        };
        let item = CustomOutbound {
            id: Uuid::new_v4().to_string(),
            name,
            enabled: true,
            builtin: false,
            protocol: protocol_for(kind),
        };
        self.selected_outbound = Some(item.id.clone());
        slices.outbounds.items.push(item);
        self.sync_inputs(window, cx);
        self.finish_edit(cx);
    }

    pub(super) fn current_dns_server_mut(&mut self) -> Option<&mut DnsServer> {
        let index = self.selected_dns_server?;
        self.draft.as_mut()?.dns.servers.get_mut(index)
    }

    pub(super) fn current_dns_rule_mut(&mut self) -> Option<&mut DnsRule> {
        let index = self.selected_dns_rule?;
        self.draft.as_mut()?.dns.rules.get_mut(index)
    }

    pub(super) fn finish_edit(&mut self, cx: &mut Context<Self>) {
        self.dirty = self.draft != self.saved || !self.input_errors.is_empty();
        self.notice = None;
        self.error =
            (!self.input_errors.is_empty()).then(|| "数值字段格式无效；修正后再继续".to_string());
        cx.notify();
    }
}
fn with_server(
    slices: &mut lanpath_client::config_slices::ConfigSlices,
    index: Option<usize>,
    edit: impl FnOnce(&mut DnsServer),
) {
    if let Some(server) = index.and_then(|index| slices.dns.servers.get_mut(index)) {
        edit(server);
    }
}

fn with_rule(
    slices: &mut lanpath_client::config_slices::ConfigSlices,
    index: Option<usize>,
    edit: impl FnOnce(&mut DnsRule),
) {
    if let Some(rule) = index.and_then(|index| slices.dns.rules.get_mut(index)) {
        edit(rule);
    }
}

fn parse_optional_u16(value: &str) -> Result<Option<u16>, ()> {
    let value = value.trim();
    if value.is_empty() {
        Ok(None)
    } else {
        value.parse().map(Some).map_err(|_| ())
    }
}

fn protocol_for(kind: OutboundKind) -> OutboundProtocol {
    use lanpath_client::config_slices::{
        Hysteria2Outbound, SelectorOutbound, ShadowsocksOutbound, TrojanOutbound, UrlTestOutbound,
        VmessOutbound,
    };
    match kind {
        OutboundKind::Vless => OutboundProtocol::Vless(VlessOutbound::default()),
        OutboundKind::Vmess => OutboundProtocol::Vmess(VmessOutbound::default()),
        OutboundKind::Shadowsocks => OutboundProtocol::Shadowsocks(ShadowsocksOutbound::default()),
        OutboundKind::Trojan => OutboundProtocol::Trojan(TrojanOutbound::default()),
        OutboundKind::Hysteria2 => OutboundProtocol::Hysteria2(Hysteria2Outbound::default()),
        OutboundKind::Selector => OutboundProtocol::Selector(SelectorOutbound::default()),
        OutboundKind::UrlTest => OutboundProtocol::UrlTest(UrlTestOutbound::default()),
    }
}

pub(super) fn set_input(
    input: &Entity<InputState>,
    value: impl Into<String>,
    window: &mut Window,
    cx: &mut Context<ConfigSlicesView>,
) {
    input.update(cx, |state, cx| state.set_value(value.into(), window, cx));
}
