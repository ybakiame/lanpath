use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};

use super::ConfigSlicesView;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum FormField {
    DnsFinal,
    DnsServerTag,
    DnsServerName,
    DnsServerAddress,
    DnsServerPort,
    DnsServerInet4,
    DnsServerInet6,
    DnsServerDetour,
    DnsServerResolver,
    DnsRuleTarget,
    DnsRuleServer,
    DnsRuleRcode,
    OutName,
    OutServer,
    OutPort,
    OutCredential,
    OutExtra,
    OutAlterId,
    OutTlsName,
    OutAlpn,
    OutTransportKind,
    OutTransportPath,
    OutTransportHost,
    OutMembers,
    OutDefault,
    OutUrl,
    OutInterval,
    OutTolerance,
    OutUp,
    OutDown,
    OutObfsType,
    OutObfsPassword,
    RouteFinal,
    RouteResolver,
    CachePath,
    CacheId,
}

pub(super) struct FormInputs {
    pub(super) dns_final: Entity<InputState>,
    pub(super) dns_server_tag: Entity<InputState>,
    pub(super) dns_server_name: Entity<InputState>,
    pub(super) dns_server_address: Entity<InputState>,
    pub(super) dns_server_port: Entity<InputState>,
    pub(super) dns_server_inet4: Entity<InputState>,
    pub(super) dns_server_inet6: Entity<InputState>,
    pub(super) dns_server_detour: Entity<InputState>,
    pub(super) dns_server_resolver: Entity<InputState>,
    pub(super) dns_rule_target: Entity<InputState>,
    pub(super) dns_rule_server: Entity<InputState>,
    pub(super) dns_rule_rcode: Entity<InputState>,
    pub(super) out_name: Entity<InputState>,
    pub(super) out_server: Entity<InputState>,
    pub(super) out_port: Entity<InputState>,
    pub(super) out_credential: Entity<InputState>,
    pub(super) out_extra: Entity<InputState>,
    pub(super) out_alter_id: Entity<InputState>,
    pub(super) out_tls_name: Entity<InputState>,
    pub(super) out_alpn: Entity<InputState>,
    pub(super) out_transport_kind: Entity<InputState>,
    pub(super) out_transport_path: Entity<InputState>,
    pub(super) out_transport_host: Entity<InputState>,
    pub(super) out_members: Entity<InputState>,
    pub(super) out_default: Entity<InputState>,
    pub(super) out_url: Entity<InputState>,
    pub(super) out_interval: Entity<InputState>,
    pub(super) out_tolerance: Entity<InputState>,
    pub(super) out_up: Entity<InputState>,
    pub(super) out_down: Entity<InputState>,
    pub(super) out_obfs_type: Entity<InputState>,
    pub(super) out_obfs_password: Entity<InputState>,
    pub(super) route_final: Entity<InputState>,
    pub(super) route_resolver: Entity<InputState>,
    pub(super) cache_path: Entity<InputState>,
    pub(super) cache_id: Entity<InputState>,
}

impl FormInputs {
    pub(super) fn new(
        window: &mut Window,
        cx: &mut Context<ConfigSlicesView>,
    ) -> (Self, Vec<Subscription>) {
        macro_rules! input {
            ($placeholder:literal) => {
                cx.new(|cx| InputState::new(window, cx).placeholder($placeholder))
            };
        }
        let inputs = Self {
            dns_final: input!("DNS final tag"),
            dns_server_tag: input!("唯一 tag"),
            dns_server_name: input!("显示名称"),
            dns_server_address: input!("服务器地址"),
            dns_server_port: input!("端口；留空使用默认"),
            dns_server_inet4: input!("FakeIP IPv4 CIDR"),
            dns_server_inet6: input!("FakeIP IPv6 CIDR（可选）"),
            dns_server_detour: input!("上游出站 tag（可选）"),
            dns_server_resolver: input!("域名解析服务器 tag（可选）"),
            dns_rule_target: input!("匹配目标；多值用逗号分隔"),
            dns_rule_server: input!("目标 DNS 服务器 tag"),
            dns_rule_rcode: input!("响应码；默认 NOERROR"),
            out_name: input!("名称；生成 slice-* tag"),
            out_server: input!("服务器地址"),
            out_port: input!("服务器端口"),
            out_credential: input!("UUID 或密码"),
            out_extra: input!("flow / security / method"),
            out_alter_id: input!("VMess alter ID"),
            out_tls_name: input!("TLS server name"),
            out_alpn: input!("ALPN；逗号分隔"),
            out_transport_kind: input!("传输类型：tcp/ws/grpc/http/httpupgrade"),
            out_transport_path: input!("路径或 gRPC service name"),
            out_transport_host: input!("Host"),
            out_members: input!("成员 tag；逗号分隔"),
            out_default: input!("Selector 默认成员 tag"),
            out_url: input!("URLTest URL"),
            out_interval: input!("URLTest 间隔，例如 5m"),
            out_tolerance: input!("容差毫秒；0 使用核心默认"),
            out_up: input!("Hysteria2 上行 Mbps；0 使用 BBR"),
            out_down: input!("Hysteria2 下行 Mbps；0 使用 BBR"),
            out_obfs_type: input!("Hysteria2 混淆类型"),
            out_obfs_password: input!("Hysteria2 混淆密码"),
            route_final: input!("默认出站 tag；留空保持模板"),
            route_resolver: input!("默认域名解析服务器 tag"),
            cache_path: input!("缓存路径；留空使用 cache.db"),
            cache_id: input!("Cache ID；留空不分 store"),
        };
        let mut subscriptions = Vec::new();
        macro_rules! watch {
            ($field:ident, $input:ident) => {{
                let entity = inputs.$input.clone();
                subscriptions.push(cx.subscribe(
                    &inputs.$input,
                    move |this, _, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) && !this.syncing {
                            let value = entity.read(cx).value().to_string();
                            this.update_field(FormField::$field, value, cx);
                        }
                    },
                ));
            }};
        }
        watch!(DnsFinal, dns_final);
        watch!(DnsServerTag, dns_server_tag);
        watch!(DnsServerName, dns_server_name);
        watch!(DnsServerAddress, dns_server_address);
        watch!(DnsServerPort, dns_server_port);
        watch!(DnsServerInet4, dns_server_inet4);
        watch!(DnsServerInet6, dns_server_inet6);
        watch!(DnsServerDetour, dns_server_detour);
        watch!(DnsServerResolver, dns_server_resolver);
        watch!(DnsRuleTarget, dns_rule_target);
        watch!(DnsRuleServer, dns_rule_server);
        watch!(DnsRuleRcode, dns_rule_rcode);
        watch!(OutName, out_name);
        watch!(OutServer, out_server);
        watch!(OutPort, out_port);
        watch!(OutCredential, out_credential);
        watch!(OutExtra, out_extra);
        watch!(OutAlterId, out_alter_id);
        watch!(OutTlsName, out_tls_name);
        watch!(OutAlpn, out_alpn);
        watch!(OutTransportKind, out_transport_kind);
        watch!(OutTransportPath, out_transport_path);
        watch!(OutTransportHost, out_transport_host);
        watch!(OutMembers, out_members);
        watch!(OutDefault, out_default);
        watch!(OutUrl, out_url);
        watch!(OutInterval, out_interval);
        watch!(OutTolerance, out_tolerance);
        watch!(OutUp, out_up);
        watch!(OutDown, out_down);
        watch!(OutObfsType, out_obfs_type);
        watch!(OutObfsPassword, out_obfs_password);
        watch!(RouteFinal, route_final);
        watch!(RouteResolver, route_resolver);
        watch!(CachePath, cache_path);
        watch!(CacheId, cache_id);
        (inputs, subscriptions)
    }

    pub(super) fn set_locked(&self, locked: bool, cx: &mut Context<ConfigSlicesView>) {
        for input in [
            &self.dns_final,
            &self.dns_server_tag,
            &self.dns_server_name,
            &self.dns_server_address,
            &self.dns_server_port,
            &self.dns_server_inet4,
            &self.dns_server_inet6,
            &self.dns_server_detour,
            &self.dns_server_resolver,
            &self.dns_rule_target,
            &self.dns_rule_server,
            &self.dns_rule_rcode,
            &self.out_name,
            &self.out_server,
            &self.out_port,
            &self.out_credential,
            &self.out_extra,
            &self.out_alter_id,
            &self.out_tls_name,
            &self.out_alpn,
            &self.out_transport_kind,
            &self.out_transport_path,
            &self.out_transport_host,
            &self.out_members,
            &self.out_default,
            &self.out_url,
            &self.out_interval,
            &self.out_tolerance,
            &self.out_up,
            &self.out_down,
            &self.out_obfs_type,
            &self.out_obfs_password,
            &self.route_final,
            &self.route_resolver,
            &self.cache_path,
            &self.cache_id,
        ] {
            input.update(cx, |state, cx| state.set_readonly(locked, cx));
        }
    }
}
