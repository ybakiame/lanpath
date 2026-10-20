use gpui_kit::{Context, Window};
use lanpath_client::config_slices::OutboundProtocol;

use super::{
    ConfigSlicesView,
    editing::set_input,
    outbound_editing::{tls, transport},
};

#[derive(Default)]
struct OutboundForm {
    name: String,
    server: String,
    port: String,
    credential: String,
    extra: String,
    alter_id: String,
    tls_name: String,
    alpn: String,
    transport_kind: String,
    transport_path: String,
    transport_host: String,
    members: String,
    default_member: String,
    url: String,
    interval: String,
    tolerance: String,
    up: String,
    down: String,
    obfs_type: String,
    obfs_password: String,
}

impl ConfigSlicesView {
    pub(super) fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(slices) = self.draft.as_ref() else {
            return;
        };
        let server = self
            .selected_dns_server
            .and_then(|index| slices.dns.servers.get(index));
        let rule = self
            .selected_dns_rule
            .and_then(|index| slices.dns.rules.get(index));
        let outbound = self
            .selected_outbound
            .as_deref()
            .and_then(|id| slices.outbounds.items.iter().find(|item| item.id == id));
        let outbound = outbound.map(outbound_form).unwrap_or_default();
        let dns_final = slices.dns.final_tag.clone();
        let route_final = slices.route.final_tag.clone();
        let route_resolver = slices.route.resolver.server.clone();
        let cache_path = slices.experimental.cache_file.path.clone();
        let cache_id = slices.experimental.cache_file.cache_id.clone();
        let server_values = server
            .map(|server| {
                (
                    server.tag.clone(),
                    server.name.clone(),
                    server.server.clone(),
                    server
                        .server_port
                        .map(|port| port.to_string())
                        .unwrap_or_default(),
                    server.inet4_range.clone(),
                    server.inet6_range.clone(),
                    server.detour.clone(),
                    server.domain_resolver.clone(),
                )
            })
            .unwrap_or_default();
        let rule_values = rule
            .map(|rule| {
                (
                    rule.target.clone(),
                    rule.server_tag.clone(),
                    rule.rcode.clone(),
                )
            })
            .unwrap_or_default();

        self.syncing = true;
        macro_rules! set {
            ($input:ident, $value:expr) => {
                set_input(&self.inputs.$input, $value, window, cx);
            };
        }
        set!(dns_final, dns_final);
        set!(dns_server_tag, server_values.0);
        set!(dns_server_name, server_values.1);
        set!(dns_server_address, server_values.2);
        set!(dns_server_port, server_values.3);
        set!(dns_server_inet4, server_values.4);
        set!(dns_server_inet6, server_values.5);
        set!(dns_server_detour, server_values.6);
        set!(dns_server_resolver, server_values.7);
        set!(dns_rule_target, rule_values.0);
        set!(dns_rule_server, rule_values.1);
        set!(dns_rule_rcode, rule_values.2);
        set!(out_name, outbound.name);
        set!(out_server, outbound.server);
        set!(out_port, outbound.port);
        set!(out_credential, outbound.credential);
        set!(out_extra, outbound.extra);
        set!(out_alter_id, outbound.alter_id);
        set!(out_tls_name, outbound.tls_name);
        set!(out_alpn, outbound.alpn);
        set!(out_transport_kind, outbound.transport_kind);
        set!(out_transport_path, outbound.transport_path);
        set!(out_transport_host, outbound.transport_host);
        set!(out_members, outbound.members);
        set!(out_default, outbound.default_member);
        set!(out_url, outbound.url);
        set!(out_interval, outbound.interval);
        set!(out_tolerance, outbound.tolerance);
        set!(out_up, outbound.up);
        set!(out_down, outbound.down);
        set!(out_obfs_type, outbound.obfs_type);
        set!(out_obfs_password, outbound.obfs_password);
        set!(route_final, route_final);
        set!(route_resolver, route_resolver);
        set!(cache_path, cache_path);
        set!(cache_id, cache_id);
        self.syncing = false;
    }
}

fn outbound_form(outbound: &lanpath_client::config_slices::CustomOutbound) -> OutboundForm {
    let mut form = OutboundForm {
        name: outbound.name.clone(),
        ..OutboundForm::default()
    };
    match &outbound.protocol {
        OutboundProtocol::Vless(node) => {
            form.server.clone_from(&node.server);
            form.port = node.server_port.to_string();
            form.credential.clone_from(&node.uuid);
            form.extra.clone_from(&node.flow);
        }
        OutboundProtocol::Vmess(node) => {
            form.server.clone_from(&node.server);
            form.port = node.server_port.to_string();
            form.credential.clone_from(&node.uuid);
            form.extra.clone_from(&node.security);
            form.alter_id = node.alter_id.to_string();
        }
        OutboundProtocol::Shadowsocks(node) => {
            form.server.clone_from(&node.server);
            form.port = node.server_port.to_string();
            form.credential.clone_from(&node.password);
            form.extra.clone_from(&node.method);
        }
        OutboundProtocol::Trojan(node) => {
            form.server.clone_from(&node.server);
            form.port = node.server_port.to_string();
            form.credential.clone_from(&node.password);
        }
        OutboundProtocol::Hysteria2(node) => {
            form.server.clone_from(&node.server);
            form.port = node.server_port.to_string();
            form.credential.clone_from(&node.password);
            form.up = node.up_mbps.to_string();
            form.down = node.down_mbps.to_string();
            form.obfs_type.clone_from(&node.obfs.obfs_type);
            form.obfs_password.clone_from(&node.obfs.password);
        }
        OutboundProtocol::Selector(group) => {
            form.members = group.outbounds.join(", ");
            form.default_member.clone_from(&group.default);
        }
        OutboundProtocol::UrlTest(group) => {
            form.members = group.outbounds.join(", ");
            form.url.clone_from(&group.url);
            form.interval.clone_from(&group.interval);
            form.tolerance = group.tolerance.to_string();
        }
    }
    if let Some(tls) = tls(&outbound.protocol) {
        form.tls_name.clone_from(&tls.server_name);
        form.alpn = tls.alpn.join(", ");
    }
    if let Some(transport) = transport(&outbound.protocol) {
        form.transport_kind.clone_from(&transport.kind);
        form.transport_path.clone_from(&transport.path);
        form.transport_host.clone_from(&transport.host);
    }
    form
}
