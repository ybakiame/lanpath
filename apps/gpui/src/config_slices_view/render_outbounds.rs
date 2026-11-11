use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    switch::Switch,
};
use gpui_kit::{
    Context, IntoElement, ParentElement as _, Styled as _, Window, div, prelude::FluentBuilder as _,
};
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
                Button::new("outbound-add")
                    .label("新增出站…")
                    .disabled(disabled)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.open_new_outbound(window, cx)),
                    ),
            )
            .child(div().flex().flex_col().min_w_0().gap_2().children(
                slices.outbounds.items.iter().map(|item| {
                    let id = item.id.clone();
                    let selected = self.selected_outbound.as_deref() == Some(item.id.as_str());
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(if selected {
                            cx.theme().primary
                        } else {
                            cx.theme().border
                        })
                        .bg(cx.theme().group_box)
                        .child(
                            Button::new(format!("outbound-{}", item.id))
                                .accessibility_label(item.name.clone())
                                .ghost()
                                .w_full()
                                .h_auto()
                                .p_0()
                                .justify_start()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.select_outbound(id.clone(), window, cx)
                                }))
                                .child(
                                    div()
                                        .w_full()
                                        .flex()
                                        .flex_col()
                                        .items_start()
                                        .gap_1()
                                        .child(item.name.clone())
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(format!(
                                                    "{}{}",
                                                    if item.builtin {
                                                        "内置分组"
                                                    } else {
                                                        "自定义节点"
                                                    },
                                                    if item.enabled { "" } else { " · 已停用" }
                                                )),
                                        ),
                                ),
                        )
                        .when(selected, |element| {
                            element.child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_2()
                                    .pt_2()
                                    .border_t_1()
                                    .border_color(cx.theme().border)
                                    .child(
                                        Button::new("outbound-edit")
                                            .small()
                                            .icon(IconName::SquarePen)
                                            .label("编辑")
                                            .disabled(disabled)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.open_outbound_editor(window, cx)
                                            })),
                                    )
                                    .child(
                                        Button::new("outbound-toggle")
                                            .small()
                                            .ghost()
                                            .label(if item.enabled { "停用" } else { "启用" })
                                            .disabled(disabled)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.toggle_outbound_enabled(cx)
                                            })),
                                    )
                                    .child(
                                        Button::new("outbound-remove")
                                            .small()
                                            .danger()
                                            .outline()
                                            .label("删除")
                                            .disabled(disabled || item.builtin)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.remove_outbound(window, cx)
                                            })),
                                    ),
                            )
                        })
                }),
            ))
    }

    pub(super) fn render_node_fields(
        &self,
        outbound: &lanpath_client::config_slices::CustomOutbound,
        disabled: bool,
        window: &mut Window,
        view: &gpui_kit::Entity<Self>,
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
                            .flex_wrap()
                            .gap_2()
                            .child(
                                Switch::new("outbound-tls")
                                    .label("启用 TLS")
                                    .checked(tls.enabled)
                                    .disabled(disabled)
                                    .on_click(window.listener_for(view, |this, _, _, cx| {
                                        this.toggle_outbound_tls(cx)
                                    })),
                            )
                            .child(
                                Switch::new("outbound-insecure")
                                    .label("跳过证书验证")
                                    .checked(tls.insecure)
                                    .disabled(disabled || !tls.enabled)
                                    .on_click(window.listener_for(view, |this, _, _, cx| {
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

    pub(super) fn render_group_fields(
        &self,
        outbound: &lanpath_client::config_slices::CustomOutbound,
        disabled: bool,
        window: &mut Window,
        view: &gpui_kit::Entity<Self>,
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
                Switch::new("outbound-interrupt")
                    .label("切换时中断已有连接")
                    .checked(interrupt)
                    .disabled(disabled)
                    .on_click(
                        window.listener_for(view, |this, _, _, cx| {
                            this.toggle_outbound_interrupt(cx)
                        }),
                    ),
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

pub(super) fn outbound_kinds() -> [(OutboundKind, &'static str); 7] {
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
