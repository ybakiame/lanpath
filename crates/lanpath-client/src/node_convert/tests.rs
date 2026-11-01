use super::*;
use serde_json::json;

#[test]
fn singbox_to_mihomo_preserves_tls_transport_and_reality() {
    let outbound = json!({
        "type": "vless",
        "tag": "vl",
        "server": "edge.example.com",
        "server_port": 443,
        "uuid": "12345678-1234-1234-1234-123456789012",
        "flow": "xtls-rprx-vision",
        "tls": {
            "enabled": true,
            "server_name": "edge.example.com",
            "utls": { "enabled": true, "fingerprint": "chrome" },
            "reality": { "enabled": true, "public_key": "pk", "short_id": "ab" }
        },
        "transport": {
            "type": "ws",
            "path": "/ws",
            "headers": { "Host": "cdn.example.com" }
        }
    });
    let proxy = singbox_to_mihomo(&outbound).unwrap();
    assert_eq!(proxy["type"], "vless");
    assert_eq!(proxy["name"], "vl");
    assert_eq!(proxy["port"], 443);
    assert_eq!(proxy["tls"], true);
    assert_eq!(proxy["servername"], "edge.example.com");
    assert_eq!(proxy["network"], "ws");
    assert_eq!(proxy["ws-opts"]["path"], "/ws");
    assert_eq!(proxy["ws-opts"]["headers"]["Host"], "cdn.example.com");
    assert_eq!(proxy["reality-opts"]["public-key"], "pk");
    assert_eq!(proxy["reality-opts"]["short-id"], "ab");
}

#[test]
fn singbox_unsupported_outbound_is_reported_not_silently_skipped() {
    let outbound = json!({
        "type": "wireguard",
        "tag": "wg",
        "server": "edge.example.com",
        "server_port": 51820
    });
    assert!(singbox_to_mihomo(&outbound).is_none());
}

#[test]
fn mihomo_to_singbox_converts_common_proxies() {
    let proxies = vec![
        json!({
            "name": "ss1", "type": "ss", "server": "s.com", "port": 8388,
            "cipher": "aes-256-gcm", "password": "pw"
        }),
        json!({
            "name": "vm1", "type": "vmess", "server": "s.com", "port": 443,
            "uuid": "12345678-1234-1234-1234-123456789012", "alterId": 0,
            "cipher": "auto", "tls": true, "servername": "s.com", "network": "ws",
            "ws-opts": { "path": "/ws", "headers": { "Host": "cdn.s.com" } }
        }),
        json!({
            "name": "vl1", "type": "vless", "server": "s.com", "port": 443,
            "uuid": "12345678-1234-1234-1234-123456789012",
            "flow": "xtls-rprx-vision", "tls": true, "servername": "s.com",
            "client-fingerprint": "chrome",
            "reality-opts": { "public-key": "pk", "short-id": "ab" }
        }),
        json!({
            "name": "tj1", "type": "trojan", "server": "s.com", "port": 443,
            "password": "pw", "sni": "s.com", "skip-cert-verify": true
        }),
        json!({
            "name": "hy1", "type": "hysteria2", "server": "s.com", "port": 443,
            "password": "pw", "sni": "s.com"
        }),
        json!({
            "name": "tc1", "type": "tuic", "server": "s.com", "port": 443,
            "uuid": "12345678-1234-1234-1234-123456789012", "password": "pw",
            "congestion-controller": "bbr", "sni": "s.com"
        }),
        json!({
            "name": "at1", "type": "anytls", "server": "s.com", "port": 443,
            "password": "pw", "sni": "s.com"
        }),
    ];
    for p in proxies {
        let o = mihomo_to_singbox(&p).unwrap();
        assert_eq!(o["server"], p["server"]);
        assert_eq!(o["server_port"], p["port"]);
        assert_eq!(o["tag"], p["name"]);
    }
}

#[test]
fn mihomo_to_singbox_unsupported_type_is_skipped() {
    let p = json!({ "name": "x", "type": "direct", "server": "s.com", "port": 1 });
    assert!(mihomo_to_singbox(&p).is_none());
}

/// clash→sing-box 路径同样补 TLS 默认值：mihomo 侧未标 `tls: true` 的
/// trojan/hy2/tuic/anytls 仍无条件产出 `tls: {enabled: true, server_name: host}`。
#[test]
fn mihomo_to_singbox_tls_protocols_get_tls_defaults_without_tls_flag() {
    for proxy in [
        json!({ "name": "tj1", "type": "trojan", "server": "s.com", "port": 443, "password": "pw" }),
        json!({ "name": "hy1", "type": "hysteria2", "server": "s.com", "port": 443, "password": "pw", "sni": "hy.s.com" }),
        json!({ "name": "tc1", "type": "tuic", "server": "s.com", "port": 443, "uuid": "12345678-1234-1234-1234-123456789012", "password": "pw" }),
        json!({ "name": "at1", "type": "anytls", "server": "s.com", "port": 443, "password": "pw" }),
    ] {
        let o = mihomo_to_singbox(&proxy).unwrap();
        assert_eq!(o["tls"]["enabled"], true, "proxy: {proxy}");
        let expected_sn = proxy.get("sni").and_then(Value::as_str).unwrap_or("s.com");
        assert_eq!(o["tls"]["server_name"], expected_sn, "proxy: {proxy}");
    }
}

/// mihomo 侧 skip-cert-verify 映射的 insecure 在补全 TLS 默认值时保留。
#[test]
fn mihomo_to_singbox_keeps_insecure_while_adding_tls_defaults() {
    let p = json!({
        "name": "tj1", "type": "trojan", "server": "s.com", "port": 443,
        "password": "pw", "skip-cert-verify": true
    });
    let o = mihomo_to_singbox(&p).unwrap();
    assert_eq!(o["tls"]["enabled"], true);
    assert_eq!(o["tls"]["server_name"], "s.com");
    assert_eq!(o["tls"]["insecure"], true);
}
