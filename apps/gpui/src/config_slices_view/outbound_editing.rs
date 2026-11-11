use gpui_kit::{Context, Window, component::WindowExt as _};
use lanpath_client::config_slices::{
    CustomOutbound, Hysteria2Outbound, OutboundProtocol, OutboundTls, OutboundTransport,
    SelectorOutbound, ShadowsocksOutbound, TrojanOutbound, UrlTestOutbound, VlessOutbound,
    VmessOutbound,
};

use super::{ConfigSlicesView, OutboundKind, inputs::FormField};

impl ConfigSlicesView {
    pub(super) fn update_outbound_field(&mut self, field: FormField, value: String) {
        let Some(id) = self.selected_outbound.clone() else {
            return;
        };
        if matches!(
            field,
            FormField::OutPort
                | FormField::OutAlterId
                | FormField::OutTolerance
                | FormField::OutUp
                | FormField::OutDown
        ) {
            if let Ok(number) = value.trim().parse::<u32>() {
                if (matches!(
                    field,
                    FormField::OutPort | FormField::OutAlterId | FormField::OutTolerance
                ) && number > u16::MAX.into())
                    || value.trim().is_empty()
                {
                    self.input_errors.insert(field);
                    return;
                }
                self.input_errors.remove(&field);
                if let Some(outbound) = self.outbound_mut(&id) {
                    set_numeric(&mut outbound.protocol, field, number);
                }
            } else {
                self.input_errors.insert(field);
            }
            return;
        }
        let Some(outbound) = self.outbound_mut(&id) else {
            return;
        };
        match field {
            FormField::OutName if !outbound.builtin => outbound.name = value,
            FormField::OutServer => set_server(&mut outbound.protocol, value),
            FormField::OutCredential => set_credential(&mut outbound.protocol, value),
            FormField::OutExtra => set_extra(&mut outbound.protocol, value),
            FormField::OutTlsName => {
                if let Some(tls) = tls_mut(&mut outbound.protocol) {
                    tls.server_name = value;
                }
            }
            FormField::OutAlpn => {
                if let Some(tls) = tls_mut(&mut outbound.protocol) {
                    tls.alpn = csv(&value);
                }
            }
            FormField::OutTransportKind => {
                if let Some(transport) = transport_mut(&mut outbound.protocol) {
                    transport.kind = value;
                }
            }
            FormField::OutTransportPath => {
                if let Some(transport) = transport_mut(&mut outbound.protocol) {
                    transport.path = value;
                }
            }
            FormField::OutTransportHost => {
                if let Some(transport) = transport_mut(&mut outbound.protocol) {
                    transport.host = value;
                }
            }
            FormField::OutMembers => match &mut outbound.protocol {
                OutboundProtocol::Selector(group) => group.outbounds = csv(&value),
                OutboundProtocol::UrlTest(group) => group.outbounds = csv(&value),
                _ => {}
            },
            FormField::OutDefault => {
                if let OutboundProtocol::Selector(group) = &mut outbound.protocol {
                    group.default = value;
                }
            }
            FormField::OutUrl => {
                if let OutboundProtocol::UrlTest(group) = &mut outbound.protocol {
                    group.url = value;
                }
            }
            FormField::OutInterval => {
                if let OutboundProtocol::UrlTest(group) = &mut outbound.protocol {
                    group.interval = value;
                }
            }
            FormField::OutObfsType => {
                if let OutboundProtocol::Hysteria2(node) = &mut outbound.protocol {
                    node.obfs.obfs_type = value;
                }
            }
            FormField::OutObfsPassword => {
                if let OutboundProtocol::Hysteria2(node) = &mut outbound.protocol {
                    node.obfs.password = value;
                }
            }
            _ => {}
        }
    }

    pub(super) fn select_outbound(
        &mut self,
        id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.reject_invalid_input(cx) {
            return;
        }
        self.selected_outbound = Some(id);
        self.sync_inputs(window, cx);
        cx.notify();
    }

    pub(super) fn remove_outbound(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        let Some(id) = self.selected_outbound.clone() else {
            return;
        };
        if self.outbound(&id).is_some_and(|item| item.builtin) {
            self.error = Some("内置出站组不可删除".into());
            cx.notify();
            return;
        }
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            let id = id.clone();
            dialog
                .title("删除自定义出站？")
                .description("引用该出站 tag 的分组和默认路由需要同时修正后才能保存。")
                .confirm()
                .ok_text("删除")
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let id = id.clone();
                    let _ = view.update(cx, |this, cx| this.remove_outbound_now(&id, window, cx));
                    true
                })
        });
    }

    fn remove_outbound_now(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(slices) = self.draft.as_mut() else {
            return;
        };
        slices
            .outbounds
            .items
            .retain(|item| item.id != id || item.builtin);
        self.selected_outbound = slices.outbounds.items.first().map(|item| item.id.clone());
        self.sync_inputs(window, cx);
        self.finish_edit(cx);
    }

    pub(super) fn set_outbound_kind(
        &mut self,
        kind: OutboundKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.selected_outbound.clone() else {
            return;
        };
        let Some(outbound) = self.outbound_mut(&id) else {
            return;
        };
        if outbound.builtin {
            return;
        }
        outbound.protocol = protocol_for(kind);
        self.input_errors.clear();
        self.sync_inputs(window, cx);
        self.finish_edit(cx);
    }

    pub(super) fn toggle_outbound_enabled(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected_outbound.clone() else {
            return;
        };
        if let Some(outbound) = self.outbound_mut(&id) {
            outbound.enabled = !outbound.enabled;
        }
        self.finish_edit(cx);
    }

    pub(super) fn toggle_outbound_tls(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected_outbound.clone() else {
            return;
        };
        if let Some(outbound) = self.outbound_mut(&id)
            && let Some(tls) = tls_mut(&mut outbound.protocol)
        {
            tls.enabled = !tls.enabled;
        }
        self.finish_edit(cx);
    }

    pub(super) fn toggle_outbound_insecure(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected_outbound.clone() else {
            return;
        };
        if let Some(outbound) = self.outbound_mut(&id)
            && let Some(tls) = tls_mut(&mut outbound.protocol)
        {
            tls.insecure = !tls.insecure;
        }
        self.finish_edit(cx);
    }

    pub(super) fn toggle_outbound_interrupt(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected_outbound.clone() else {
            return;
        };
        if let Some(outbound) = self.outbound_mut(&id) {
            match &mut outbound.protocol {
                OutboundProtocol::Selector(group) => {
                    group.interrupt_exist_connections = !group.interrupt_exist_connections
                }
                OutboundProtocol::UrlTest(group) => {
                    group.interrupt_exist_connections = !group.interrupt_exist_connections
                }
                _ => {}
            }
        }
        self.finish_edit(cx);
    }

    pub(super) fn outbound(&self, id: &str) -> Option<&CustomOutbound> {
        self.draft
            .as_ref()?
            .outbounds
            .items
            .iter()
            .find(|item| item.id == id)
    }

    fn outbound_mut(&mut self, id: &str) -> Option<&mut CustomOutbound> {
        self.draft
            .as_mut()?
            .outbounds
            .items
            .iter_mut()
            .find(|item| item.id == id)
    }
}

pub(super) fn outbound_kind(protocol: &OutboundProtocol) -> OutboundKind {
    match protocol {
        OutboundProtocol::Vless(_) => OutboundKind::Vless,
        OutboundProtocol::Vmess(_) => OutboundKind::Vmess,
        OutboundProtocol::Shadowsocks(_) => OutboundKind::Shadowsocks,
        OutboundProtocol::Trojan(_) => OutboundKind::Trojan,
        OutboundProtocol::Hysteria2(_) => OutboundKind::Hysteria2,
        OutboundProtocol::Selector(_) => OutboundKind::Selector,
        OutboundProtocol::UrlTest(_) => OutboundKind::UrlTest,
    }
}

fn set_numeric(protocol: &mut OutboundProtocol, field: FormField, value: u32) {
    match field {
        FormField::OutPort => match protocol {
            OutboundProtocol::Vless(node) => node.server_port = value as u16,
            OutboundProtocol::Vmess(node) => node.server_port = value as u16,
            OutboundProtocol::Shadowsocks(node) => node.server_port = value as u16,
            OutboundProtocol::Trojan(node) => node.server_port = value as u16,
            OutboundProtocol::Hysteria2(node) => node.server_port = value as u16,
            _ => {}
        },
        FormField::OutAlterId => {
            if let OutboundProtocol::Vmess(node) = protocol {
                node.alter_id = value as u16;
            }
        }
        FormField::OutTolerance => {
            if let OutboundProtocol::UrlTest(group) = protocol {
                group.tolerance = value as u16;
            }
        }
        FormField::OutUp => {
            if let OutboundProtocol::Hysteria2(node) = protocol {
                node.up_mbps = value;
            }
        }
        FormField::OutDown => {
            if let OutboundProtocol::Hysteria2(node) = protocol {
                node.down_mbps = value;
            }
        }
        _ => {}
    }
}

fn set_server(protocol: &mut OutboundProtocol, value: String) {
    match protocol {
        OutboundProtocol::Vless(node) => node.server = value,
        OutboundProtocol::Vmess(node) => node.server = value,
        OutboundProtocol::Shadowsocks(node) => node.server = value,
        OutboundProtocol::Trojan(node) => node.server = value,
        OutboundProtocol::Hysteria2(node) => node.server = value,
        _ => {}
    }
}

fn set_credential(protocol: &mut OutboundProtocol, value: String) {
    match protocol {
        OutboundProtocol::Vless(node) => node.uuid = value,
        OutboundProtocol::Vmess(node) => node.uuid = value,
        OutboundProtocol::Shadowsocks(node) => node.password = value,
        OutboundProtocol::Trojan(node) => node.password = value,
        OutboundProtocol::Hysteria2(node) => node.password = value,
        _ => {}
    }
}

fn set_extra(protocol: &mut OutboundProtocol, value: String) {
    match protocol {
        OutboundProtocol::Vless(node) => node.flow = value,
        OutboundProtocol::Vmess(node) => node.security = value,
        OutboundProtocol::Shadowsocks(node) => node.method = value,
        _ => {}
    }
}

pub(super) fn tls(protocol: &OutboundProtocol) -> Option<&OutboundTls> {
    match protocol {
        OutboundProtocol::Vless(node) => Some(&node.tls),
        OutboundProtocol::Vmess(node) => Some(&node.tls),
        OutboundProtocol::Trojan(node) => Some(&node.tls),
        OutboundProtocol::Hysteria2(node) => Some(&node.tls),
        _ => None,
    }
}
fn tls_mut(protocol: &mut OutboundProtocol) -> Option<&mut OutboundTls> {
    match protocol {
        OutboundProtocol::Vless(node) => Some(&mut node.tls),
        OutboundProtocol::Vmess(node) => Some(&mut node.tls),
        OutboundProtocol::Trojan(node) => Some(&mut node.tls),
        OutboundProtocol::Hysteria2(node) => Some(&mut node.tls),
        _ => None,
    }
}
pub(super) fn transport(protocol: &OutboundProtocol) -> Option<&OutboundTransport> {
    match protocol {
        OutboundProtocol::Vless(node) => Some(&node.transport),
        OutboundProtocol::Vmess(node) => Some(&node.transport),
        OutboundProtocol::Trojan(node) => Some(&node.transport),
        _ => None,
    }
}
fn transport_mut(protocol: &mut OutboundProtocol) -> Option<&mut OutboundTransport> {
    match protocol {
        OutboundProtocol::Vless(node) => Some(&mut node.transport),
        OutboundProtocol::Vmess(node) => Some(&mut node.transport),
        OutboundProtocol::Trojan(node) => Some(&mut node.transport),
        _ => None,
    }
}

fn csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn protocol_for(kind: OutboundKind) -> OutboundProtocol {
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
