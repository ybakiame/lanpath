//! 连接列表行卡片：展示目标主机/IP/进程、命中规则与出站链、上下行流量，
//! 以及「直连 / 代理 / 拦截 / 自定义」一键写入规则与单条断开操作。

use lanpath_client::ConnectionView;
use nsh_base::theme;
use nsh_core::prelude::*;

use super::state::{ConnTab, ConnectionsState};
use crate::Page;
use crate::connections::{format_bytes, rule_seed_for};
use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::feedback::{BadgeVariant, badge};
use crate::ui::icon::{Icon, Tone, icon_sized};
use crate::ui::layout::{SurfaceLevel, surface};

pub(super) fn connection_row(
    conn: ConnectionView,
    state: ConnectionsState,
    page: Signal<Page>,
    rule_seed: Signal<String>,
) -> AnyElement {
    let is_active = state.tab.get() == ConnTab::Active;
    let id = conn.id.clone();
    let host = if conn.host.is_empty() {
        t!("connections.unknown_host")
    } else {
        conn.host.clone()
    };
    let network = conn.network.to_ascii_uppercase();
    let process = conn.process.clone();
    let dest_ip = conn.destination_ip.clone();
    let show_dest_ip = !dest_ip.is_empty() && dest_ip != host && !host.starts_with(&dest_ip);
    let rule_label = format_hit_rule(&conn.rule, &conn.rule_payload);
    let chain_label = if conn.chain.is_empty() {
        conn.outbound.clone()
    } else {
        conn.chain.clone()
    };
    let outbound_variant = outbound_badge_variant(&conn.outbound);
    let traffic_up = format_bytes(conn.upload);
    let traffic_down = format_bytes(conn.download);

    let can_add_rule = !conn.target.trim().is_empty() || !conn.process.trim().is_empty();
    let conn_direct = conn.clone();
    let conn_proxy = conn.clone();
    let conn_block = conn.clone();
    let conn_custom = conn.clone();
    let st_direct = state.clone();
    let st_proxy = state.clone();
    let st_block = state.clone();
    let st_close = state;
    let close_id = id.clone();

    let mut header_left = view()
        .flex_1()
        .min_w(px(0.0))
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_2()
        .child(
            text(host)
                .text_size(px(13.5))
                .font_weight(FontWeight::SEMIBOLD)
                .selectable(true),
        );
    if !network.is_empty() {
        let net_text = network.clone();
        header_left = header_left.child(badge(BadgeVariant::Outline, move || net_text.clone()));
    }
    if !process.is_empty() {
        let proc_text = process.clone();
        header_left = header_left.child(badge(BadgeVariant::Default, move || proc_text.clone()));
    }

    let mut header_right = view()
        .flex_row()
        .items_center()
        .gap_3()
        .flex_none()
        .child(
            text(format!("↑ {traffic_up}"))
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            text(format!("↓ {traffic_down}"))
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || theme().foreground),
        );
    if is_active {
        header_right = header_right.child(
            button(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .test_id(format!("connections-close-{id}"))
                .prefix(|| icon_sized(Icon::Close, Tone::Danger, 13.0))
                .label(|| t!("connections.close_one"))
                .on_press(move || st_close.close_one(close_id.clone())),
        );
    }

    let mut meta_row = view().flex_row().flex_wrap().items_center().gap_2().child(
        view()
            .flex_row()
            .items_center()
            .gap_1()
            .px_2()
            .py(px(2.0))
            .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
            .bg(move || theme().muted.opacity(0.6))
            .child(icon_sized(Icon::Rules, Tone::Primary, 12.0))
            .child(
                text(rule_label)
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .selectable(true),
            ),
    );
    if !chain_label.is_empty() {
        let chain_text = chain_label.clone();
        meta_row = meta_row.child(badge(outbound_variant, move || chain_text.clone()));
    }
    if show_dest_ip {
        meta_row = meta_row.child(
            text(dest_ip)
                .text_size(px(11.5))
                .text_color(move || theme().muted_foreground)
                .selectable(true),
        );
    }

    let mut card = surface(SurfaceLevel::Card)
        .test_id(&format!("connection-row-{id}"))
        .p_3()
        .flex_col()
        .gap_2()
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_2()
                .child(header_left)
                .child(header_right),
        )
        .child(meta_row);

    if can_add_rule {
        let direct_id = format!("connections-rule-direct-{id}");
        let proxy_id = format!("connections-rule-proxy-{id}");
        let block_id = format!("connections-rule-block-{id}");
        let custom_id = format!("connections-rule-custom-{id}");
        card = card.child(
            view()
                .pt_1()
                .border_t(px(1.0))
                .border_color(move || theme().border.opacity(0.6))
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    text(|| t!("connections.quick_rule"))
                        .text_size(px(11.5))
                        .text_color(move || theme().muted_foreground),
                )
                .child(
                    button(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .test_id(direct_id)
                        .label(|| t!("connections.add_direct"))
                        .on_press(move || st_direct.quick_add_rule(&conn_direct, "direct")),
                )
                .child(
                    button(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .test_id(proxy_id)
                        .label(|| t!("connections.add_proxy"))
                        .on_press(move || st_proxy.quick_add_rule(&conn_proxy, "proxy")),
                )
                .child(
                    button(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .test_id(block_id)
                        .label(|| t!("connections.add_block"))
                        .on_press(move || st_block.quick_add_rule(&conn_block, "block")),
                )
                .child(
                    button(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .test_id(custom_id)
                        .prefix(|| icon_sized(Icon::Plus, Tone::Primary, 12.0))
                        .label(|| t!("connections.add_custom"))
                        .on_press(move || {
                            rule_seed.set(rule_seed_for(&conn_custom));
                            page.set(Page::Rules);
                        }),
                ),
        );
    }

    card.into_element()
}

fn format_hit_rule(rule: &str, payload: &str) -> String {
    let r = rule.trim();
    let p = payload.trim();
    match (r.is_empty(), p.is_empty()) {
        (true, true) => t!("connections.no_rule"),
        (false, true) => r.to_owned(),
        (true, false) => p.to_owned(),
        (false, false) => format!("{r} · {p}"),
    }
}

fn outbound_badge_variant(outbound: &str) -> BadgeVariant {
    let lower = outbound.trim().to_ascii_lowercase();
    if lower == "direct" {
        BadgeVariant::Success
    } else if lower == "block" || lower == "reject" {
        BadgeVariant::Destructive
    } else {
        BadgeVariant::Default
    }
}
