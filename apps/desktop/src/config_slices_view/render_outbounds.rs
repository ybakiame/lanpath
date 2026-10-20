use gpui_kit::component::{
    Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{Context, IntoElement, ParentElement as _, Styled as _, div};
use lanpath_client::config_slices::OutboundProtocol;

use super::{
    ConfigSlicesView, OutboundKind,
    outbound_editing::{outbound_kind, tls, transport},
    render::{field, section_title},
};

impl ConfigSlicesView {
    pub(super) fn render_outbounds(&self, disabled: bool, cx: &Context<Self>) -> impl IntoElement {
        let Some(slices) = self.draft.as_ref() else {
            return div().child("出站配置不可用");
        };
        let outbound = self
            .selected_outbound
            .as_deref()
            .and_then(|id| self.outbound(id));
        let selected_kind = outbound.map(|item| outbound_kind(&item.protocol));
        div()
            .flex()
            .flex_col()
            .gap_3()
            .pb_4()
            .child(section_title(
                "出站与分组",
                "自定义节点生成 slice-* tag；内置 proxy/auto 等分组可调整但不可删除。",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .children(slices.outbounds.items.iter().map(|item| {
                        let id = item.id.clone();
                        Button::new(format!("outbound-{}", item.id))
                            .label(format!(
                                "{}{}",
                                if item.builtin {
                                    "内置 · "
                                } else if !item.enabled {
                                    "停用 · "
                                } else {
                                    ""
                                },
                                item.name
                            ))
                            .selected(self.selected_outbound.as_deref() == Some(item.id.as_str()))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_outbound(id.clone(), window, cx)
                            }))
                    }))
                    .child(
                        Button::new("outbound-remove")
                            .label("删除")
                            .danger()
                            .disabled(disabled || outbound.is_none_or(|item| item.builtin))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.remove_outbound(window, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .children(outbound_kinds().into_iter().map(|(kind, label)| {
                        Button::new(format!("outbound-add-{label}"))
                            .label(format!("新增 {label}"))
                            .disabled(disabled)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.add_outbound(kind, window, cx)
                            }))
                    })),
            )
            .children(outbound.map(|outbound| {
                let protocol_locked = disabled || outbound.builtin;
                let is_group = outbound.protocol.is_group();
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("outbound-enabled")
                                    .label("启用")
                                    .selected(outbound.enabled)
                                    .disabled(disabled)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_outbound_enabled(cx)
                                    })),
                            )
                            .children(outbound.builtin.then(|| {
                                div().text_sm().child("内置分组名称、协议和成员由模板维护")
                            })),
                    )
                    .child(field("名称", &self.inputs.out_name, protocol_locked))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .children(outbound_kinds().into_iter().map(|(kind, label)| {
                                Button::new(format!("outbound-kind-{label}"))
                                    .label(label)
                                    .selected(selected_kind == Some(kind))
                                    .disabled(protocol_locked)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.set_outbound_kind(kind, window, cx)
                                    }))
                            })),
                    )
                    .children((!is_group).then(|| self.render_node_fields(outbound, disabled, cx)))
                    .children(is_group.then(|| self.render_group_fields(outbound, disabled, cx)))
            }))
    }

    fn render_node_fields(
        &self,
        outbound: &lanpath_client::config_slices::CustomOutbound,
        disabled: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let kind = outbound_kind(&outbound.protocol);
        let credential = match kind {
            OutboundKind::Vless | OutboundKind::Vmess => "UUID",
            _ => "密码",
        };
        let extra = match kind {
            OutboundKind::Vless => Some("VLESS flow"),
            OutboundKind::Vmess => Some("VMess security"),
            OutboundKind::Shadowsocks => Some("加密方法"),
            _ => None,
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(field("服务器", &self.inputs.out_server, disabled))
                    .child(field("端口", &self.inputs.out_port, disabled)),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(field(credential, &self.inputs.out_credential, disabled))
                    .children(extra.map(|label| {
                        field(label, &self.inputs.out_extra, disabled).into_any_element()
                    }))
                    .children((kind == OutboundKind::Vmess).then(|| {
                        field("Alter ID", &self.inputs.out_alter_id, disabled).into_any_element()
                    })),
            )
            .children(tls(&outbound.protocol).map(|tls| {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("outbound-tls")
                                    .label("TLS")
                                    .selected(tls.enabled)
                                    .disabled(disabled)
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.toggle_outbound_tls(cx)),
                                    ),
                            )
                            .child(
                                Button::new("outbound-insecure")
                                    .label("跳过证书验证")
                                    .selected(tls.insecure)
                                    .disabled(disabled || !tls.enabled)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_outbound_insecure(cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(field(
                                "TLS server name",
                                &self.inputs.out_tls_name,
                                disabled || !tls.enabled,
                            ))
                            .child(field(
                                "ALPN",
                                &self.inputs.out_alpn,
                                disabled || !tls.enabled,
                            )),
                    )
            }))
            .children(transport(&outbound.protocol).map(|_| {
                div()
                    .flex()
                    .gap_2()
                    .child(field("传输类型", &self.inputs.out_transport_kind, disabled))
                    .child(field(
                        "路径 / service",
                        &self.inputs.out_transport_path,
                        disabled,
                    ))
                    .child(field("Host", &self.inputs.out_transport_host, disabled))
            }))
            .children((kind == OutboundKind::Hysteria2).then(|| {
                let OutboundProtocol::Hysteria2(node) = &outbound.protocol else {
                    unreachable!()
                };
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(field("上行 Mbps", &self.inputs.out_up, disabled))
                            .child(field("下行 Mbps", &self.inputs.out_down, disabled)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(field("混淆类型", &self.inputs.out_obfs_type, disabled))
                            .child(field(
                                "混淆密码",
                                &self.inputs.out_obfs_password,
                                disabled || node.obfs.obfs_type.is_empty(),
                            )),
                    )
            }))
    }

    fn render_group_fields(
        &self,
        outbound: &lanpath_client::config_slices::CustomOutbound,
        disabled: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let member_locked = disabled || outbound.builtin;
        let interrupt = match &outbound.protocol {
            OutboundProtocol::Selector(group) => group.interrupt_exist_connections,
            OutboundProtocol::UrlTest(group) => group.interrupt_exist_connections,
            _ => false,
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(field(
                "成员 tags（逗号分隔）",
                &self.inputs.out_members,
                member_locked,
            ))
            .child(
                Button::new("outbound-interrupt")
                    .label("切换时中断已有连接")
                    .selected(interrupt)
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_outbound_interrupt(cx))),
            )
            .children(
                matches!(outbound.protocol, OutboundProtocol::Selector(_)).then(|| {
                    field("默认成员 tag", &self.inputs.out_default, disabled).into_any_element()
                }),
            )
            .children(
                matches!(outbound.protocol, OutboundProtocol::UrlTest(_)).then(|| {
                    div()
                        .flex()
                        .gap_2()
                        .child(field("测速 URL", &self.inputs.out_url, disabled))
                        .child(field("间隔", &self.inputs.out_interval, disabled))
                        .child(field("容差 ms", &self.inputs.out_tolerance, disabled))
                        .into_any_element()
                }),
            )
    }
}

fn outbound_kinds() -> [(OutboundKind, &'static str); 7] {
    [
        (OutboundKind::Vless, "VLESS"),
        (OutboundKind::Vmess, "VMess"),
        (OutboundKind::Shadowsocks, "Shadowsocks"),
        (OutboundKind::Trojan, "Trojan"),
        (OutboundKind::Hysteria2, "Hysteria2"),
        (OutboundKind::Selector, "Selector"),
        (OutboundKind::UrlTest, "URLTest"),
    ]
}
