//! Audited client-only protocol subset. In particular, no bridge, WireGuard,
//! SSH, Tor, Tailcat or Naive external runtime/extra-argument capabilities.
//! Reference: https://sing-box.sagernet.org/configuration/outbound/
use std::path::Path;

use lanpath_common::LanpathResult;
use serde_json::{Map, Value};

use super::policy::{array, fields, object};
use super::resources::{TLS_INPUT_PATHS, denied, read_paths};

pub(super) const DIAL: &str = "detour bind_interface inet4_bind_address inet6_bind_address bind_address_no_port connect_timeout tcp_fast_open tcp_multi_path disable_tcp_keep_alive tcp_keep_alive tcp_keep_alive_interval udp_fragment domain_resolver domain_strategy network_strategy network_type fallback_network_type fallback_delay";

pub(super) fn check(root: &Path, value: &Value, location: &str) -> LanpathResult<()> {
    let obj = object(value, location)?;
    let protocol = obj.get("type").and_then(Value::as_str).ok_or_else(|| {
        denied(
            &format!("{location}.type"),
            "expected an explicitly supported outbound type",
        )
    })?;
    let extra = match protocol {
        "direct" => "override_address override_port proxy_protocol",
        "block" | "dns" => "",
        "socks" => "server server_port version username password network udp_over_tcp",
        "http" => "server server_port username password path headers tls",
        "shadowsocks" => "server server_port method password network udp_over_tcp multiplex",
        "vmess" => {
            "server server_port uuid security alter_id global_padding authenticated_length packet_encoding network tls transport multiplex"
        }
        "vless" => "server server_port uuid flow packet_encoding network tls transport multiplex",
        "trojan" => "server server_port password network tls transport multiplex",
        "hysteria" => {
            "server server_port server_ports up up_mbps down down_mbps obfs auth auth_str recv_window_conn recv_window disable_mtu_discovery hop_interval network tls"
        }
        "hysteria2" => {
            "server server_port server_ports hop_interval up_mbps down_mbps obfs password network tls brutal_debug"
        }
        "tuic" => {
            "server server_port uuid password congestion_control udp_relay_mode udp_over_stream zero_rtt_handshake heartbeat network tls"
        }
        "shadowtls" => "server server_port version password tls",
        "anytls" => {
            "server server_port password idle_session_check_interval idle_session_timeout min_idle_session tls"
        }
        "snell" => "server server_port version psk network udp_over_tcp multiplex",
        "selector" => "outbounds default interrupt_exist_connections",
        "urltest" => "outbounds url interval tolerance idle_timeout interrupt_exist_connections",
        _ => {
            return Err(denied(
                &format!("{location}.type"),
                "unsupported outbound capability; use direct, block, dns, socks, http, shadowsocks, vmess, vless, trojan, hysteria, hysteria2, tuic, shadowtls, anytls, snell, selector or urltest",
            ));
        }
    };
    fields(obj, location, &["type tag", DIAL, extra])?;
    shared(root, obj, location)?;
    if let Some(value) = obj.get("transport") {
        let loc = format!("{location}.transport");
        let transport = object(value, &loc)?;
        if !matches!(
            transport.get("type").and_then(Value::as_str),
            Some("http" | "ws" | "quic" | "grpc" | "httpupgrade")
        ) {
            return Err(denied(
                &format!("{loc}.type"),
                "unsupported V2Ray transport",
            ));
        }
        fields(
            transport,
            &loc,
            &[
                "type host path method headers idle_timeout ping_timeout max_early_data early_data_header_name service_name permit_without_stream",
            ],
        )?;
    }
    if let Some(value) = obj.get("multiplex") {
        let loc = format!("{location}.multiplex");
        let multiplex = object(value, &loc)?;
        fields(
            multiplex,
            &loc,
            &["enabled protocol max_connections min_streams max_streams padding brutal"],
        )?;
        if let Some(brutal) = multiplex.get("brutal") {
            fields(
                object(brutal, &format!("{loc}.brutal"))?,
                &format!("{loc}.brutal"),
                &["enabled up_mbps down_mbps"],
            )?;
        }
    }
    if let Some(value) = obj.get("udp_over_tcp")
        && !value.is_boolean()
    {
        fields(
            object(value, &format!("{location}.udp_over_tcp"))?,
            &format!("{location}.udp_over_tcp"),
            &["enabled version"],
        )?;
    }
    if let Some(value) = obj.get("obfs")
        && value.is_object()
    {
        fields(
            object(value, &format!("{location}.obfs"))?,
            &format!("{location}.obfs"),
            &["type password"],
        )?;
    }
    Ok(())
}

pub(super) fn shared(root: &Path, obj: &Map<String, Value>, location: &str) -> LanpathResult<()> {
    if let Some(value) = obj.get("tls") {
        tls(root, value, &format!("{location}.tls"))?;
    }
    if let Some(value) = obj.get("domain_resolver") {
        resolver(value, &format!("{location}.domain_resolver"))?;
    }
    Ok(())
}

pub(super) fn resolver(value: &Value, location: &str) -> LanpathResult<()> {
    if value.is_string() {
        return Ok(());
    }
    fields(
        object(value, location)?,
        location,
        &["server strategy disable_cache rewrite_ttl client_subnet timeout"],
    )
}

fn tls(root: &Path, value: &Value, location: &str) -> LanpathResult<()> {
    let tls = object(value, location)?;
    fields(
        tls,
        location,
        &[
            "enabled engine disable_sni server_name insecure alpn min_version max_version cipher_suites curve_preferences certificate certificate_path certificate_sha256 certificate_public_key_sha256 client_certificate client_certificate_path client_key client_key_path fragment fragment_fallback_delay record_fragment kernel_tx kernel_rx handshake_timeout ech utls reality",
        ],
    )?;
    for &name in TLS_INPUT_PATHS {
        if let Some(value) = tls.get(name) {
            read_paths(root, value, &format!("{location}.{name}"))?;
        }
    }
    for (name, allowed) in [
        (
            "ech",
            "enabled config config_path query_server_name pq_signature_schemes_enabled dynamic_record_sizing_disabled",
        ),
        ("utls", "enabled fingerprint"),
        ("reality", "enabled public_key short_id"),
    ] {
        if let Some(value) = tls.get(name) {
            let loc = format!("{location}.{name}");
            let nested = object(value, &loc)?;
            fields(nested, &loc, &[allowed])?;
            if let Some(path) = nested.get("config_path") {
                read_paths(root, path, &format!("{loc}.config_path"))?;
            }
        }
    }
    Ok(())
}

pub(super) fn http_client(root: &Path, value: &Value, location: &str) -> LanpathResult<()> {
    if value.is_string() {
        return Ok(());
    }
    let obj = object(value, location)?;
    fields(
        obj,
        location,
        &[
            "tag engine version disable_version_fallback headers tls",
            DIAL,
        ],
    )?;
    shared(root, obj, location)
}

pub(super) fn dns(root: &Path, value: &Value) -> LanpathResult<()> {
    let obj = object(value, "dns")?;
    fields(
        obj,
        "dns",
        &[
            "servers rules final strategy disable_cache disable_expire independent_cache cache_capacity optimistic timeout reverse_mapping client_subnet fakeip",
        ],
    )?;
    if let Some(servers) = obj.get("servers") {
        for (index, server) in array(servers, "dns.servers")?.iter().enumerate() {
            let loc = format!("dns.servers[{index}]");
            let server = object(server, &loc)?;
            let kind = server.get("type").and_then(Value::as_str);
            if !matches!(
                kind,
                Some(
                    "local" | "hosts" | "udp" | "tcp" | "tls" | "https" | "quic" | "h3" | "fakeip"
                )
            ) {
                return Err(denied(
                    &format!("{loc}.type"),
                    "use local, hosts, udp, tcp, tls, https, quic, h3 or fakeip DNS; DHCP/mDNS/endpoint/system-service DNS is unsupported",
                ));
            }
            fields(
                server,
                &loc,
                &[
                    "type tag server server_port path headers tls http_client predefined inet4_range inet6_range",
                    DIAL,
                ],
            )?;
            shared(root, server, &loc)?;
            if let Some(client) = server.get("http_client") {
                http_client(root, client, &format!("{loc}.http_client"))?;
            }
            if kind == Some("hosts") {
                let paths = server.get("path").ok_or_else(|| denied(&format!("{loc}.path"), "explicit root-confined hosts paths are required; implicit system hosts files are unsupported"))?;
                if paths.as_array().is_some_and(Vec::is_empty) {
                    return Err(denied(
                        &format!("{loc}.path"),
                        "implicit system hosts files are unsupported; specify at least one file",
                    ));
                }
                read_paths(root, paths, &format!("{loc}.path"))?;
            }
        }
    }
    Ok(())
}
