use super::*;
use base64::Engine as _;

const UUID: &str = "12345678-1234-1234-1234-123456789012";

fn b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s)
}

/// 解析单条链接并返回 sing-box outbound（测试便捷入口）。
fn parse_one(line: &str) -> Value {
    let result = parse_share_links(line);
    assert!(
        result.warnings.is_empty(),
        "unexpected warnings: {:?}",
        result.warnings
    );
    assert_eq!(result.nodes.len(), 1, "expected exactly one node");
    result.nodes[0].outbound_singbox.clone()
}

// ---------- ① 每协议至少一条样例链接 ----------

#[test]
fn parses_ss_sip002() {
    let link = format!(
        "ss://{}@example.com:8388#ss-node",
        b64("aes-256-gcm:password")
    );
    let o = parse_one(&link);
    assert_eq!(o["type"], "shadowsocks");
    assert_eq!(o["server"], "example.com");
    assert_eq!(o["server_port"], 8388);
    assert_eq!(o["method"], "aes-256-gcm");
    assert_eq!(o["password"], "password");
    assert_eq!(o["tag"], "ss-node");
}

#[test]
fn parses_ss_legacy_whole_base64() {
    let link = format!(
        "ss://{}#legacy-node",
        b64("aes-256-gcm:password@example.com:8388")
    );
    let o = parse_one(&link);
    assert_eq!(o["type"], "shadowsocks");
    assert_eq!(o["server"], "example.com");
    assert_eq!(o["server_port"], 8388);
    assert_eq!(o["method"], "aes-256-gcm");
    assert_eq!(o["password"], "password");
    assert_eq!(o["tag"], "legacy-node");
}

#[test]
fn parses_ss_plaintext_userinfo() {
    let o = parse_one("ss://aes-256-gcm:password@example.com:8388#plain");
    assert_eq!(o["server"], "example.com");
    assert_eq!(o["server_port"], 8388);
    assert_eq!(o["method"], "aes-256-gcm");
    assert_eq!(o["password"], "password");
}

#[test]
fn parses_vmess() {
    let v = json!({
        "v": "2", "ps": "vmess-node", "add": "example.com", "port": "443",
        "id": UUID, "aid": "0", "net": "ws", "type": "none",
        "host": "cdn.example.com", "path": "/ws", "tls": "tls",
        "sni": "example.com", "fp": "chrome"
    });
    let o = parse_one(&format!("vmess://{}", b64(&v.to_string())));
    assert_eq!(o["type"], "vmess");
    assert_eq!(o["server"], "example.com");
    assert_eq!(o["server_port"], 443);
    assert_eq!(o["uuid"], UUID);
    assert_eq!(o["tag"], "vmess-node");
    assert_eq!(o["tls"]["enabled"], true);
    assert_eq!(o["tls"]["server_name"], "example.com");
    assert_eq!(o["tls"]["utls"]["fingerprint"], "chrome");
    assert_eq!(o["transport"]["type"], "ws");
    assert_eq!(o["transport"]["path"], "/ws");
    assert_eq!(o["transport"]["headers"]["Host"], "cdn.example.com");
}

#[test]
fn parses_vmess_plain_json() {
    let v = json!({
        "add": "example.com", "port": 443, "id": UUID, "ps": "vmess-plain",
        "net": "tcp", "tls": "tls", "sni": "example.com"
    });
    let o = parse_one(&format!("vmess://{v}"));
    assert_eq!(o["server"], "example.com");
    assert_eq!(o["server_port"], 443);
    assert_eq!(o["tls"]["server_name"], "example.com");
}

#[test]
fn parses_vless_reality() {
    let link = format!(
        "vless://{UUID}@example.com:443?security=reality&sni=example.com&fp=chrome&pbk=REALITY_PK&sid=abcd1234&type=tcp#vless-reality"
    );
    let o = parse_one(&link);
    assert_eq!(o["type"], "vless");
    assert_eq!(o["server"], "example.com");
    assert_eq!(o["server_port"], 443);
    assert_eq!(o["uuid"], UUID);
    assert_eq!(o["tag"], "vless-reality");
    assert_eq!(o["tls"]["enabled"], true);
    assert_eq!(o["tls"]["server_name"], "example.com");
    assert_eq!(o["tls"]["utls"]["fingerprint"], "chrome");
    assert_eq!(o["tls"]["reality"]["public_key"], "REALITY_PK");
    assert_eq!(o["tls"]["reality"]["short_id"], "abcd1234");
}

#[test]
fn parses_vless_ws() {
    let link = format!(
        "vless://{UUID}@example.com:443?security=tls&sni=example.com&type=ws&path=%2Fws&host=cdn.example.com#vless-ws"
    );
    let o = parse_one(&link);
    assert_eq!(o["server"], "example.com");
    assert_eq!(o["tls"]["server_name"], "example.com");
    assert_eq!(o["transport"]["type"], "ws");
    assert_eq!(o["transport"]["path"], "/ws");
    assert_eq!(o["transport"]["headers"]["Host"], "cdn.example.com");
}

#[test]
fn parses_trojan() {
    let o = parse_one(
        "trojan://trojanpass@example.com:443?sni=example.com&allowInsecure=1#trojan-node",
    );
    assert_eq!(o["type"], "trojan");
    assert_eq!(o["server"], "example.com");
    assert_eq!(o["server_port"], 443);
    assert_eq!(o["password"], "trojanpass");
    assert_eq!(o["tls"]["enabled"], true);
    assert_eq!(o["tls"]["server_name"], "example.com");
    assert_eq!(o["tls"]["insecure"], true);
}

#[test]
fn parses_hysteria2() {
    let o = parse_one("hysteria2://hypass@example.com:443?sni=example.com&insecure=1#hy2-node");
    assert_eq!(o["type"], "hysteria2");
    assert_eq!(o["server"], "example.com");
    assert_eq!(o["server_port"], 443);
    assert_eq!(o["password"], "hypass");
    assert_eq!(o["tls"]["server_name"], "example.com");
    assert_eq!(o["tls"]["insecure"], true);
}

#[test]
fn parses_hy2_alias() {
    let o = parse_one("hy2://hypass@example.com:443?sni=example.com#hy2-alias");
    assert_eq!(o["type"], "hysteria2");
    assert_eq!(o["tag"], "hy2-alias");
}

#[test]
fn parses_tuic() {
    let o = parse_one(&format!(
        "tuic://{UUID}:tuicpass@example.com:443?sni=example.com&congestion_control=bbr#tuic-node"
    ));
    assert_eq!(o["type"], "tuic");
    assert_eq!(o["server"], "example.com");
    assert_eq!(o["server_port"], 443);
    assert_eq!(o["uuid"], UUID);
    assert_eq!(o["password"], "tuicpass");
    assert_eq!(o["congestion_control"], "bbr");
    assert_eq!(o["tls"]["server_name"], "example.com");
}

#[test]
fn parses_anytls() {
    let o = parse_one("anytls://anypass@example.com:443?sni=example.com#anytls-node");
    assert_eq!(o["type"], "anytls");
    assert_eq!(o["server"], "example.com");
    assert_eq!(o["server_port"], 443);
    assert_eq!(o["password"], "anypass");
    assert_eq!(o["tls"]["server_name"], "example.com");
}

// ---------- 项 2：TLS 默认值（无 sni 时 server_name 回退 host） ----------

#[test]
fn tls_defaults_fill_server_name_from_host_when_sni_absent() {
    let links = vec![
        "trojan://pw@example.com:443#t".to_string(),
        "hysteria2://hpw@example.com:443#h".to_string(),
        format!("tuic://{UUID}:tupw@example.com:443#u"),
        "anytls://apw@example.com:443#a".to_string(),
    ];
    for link in &links {
        let o = parse_one(link);
        assert_eq!(o["tls"]["enabled"], true, "link: {link}");
        assert_eq!(o["tls"]["server_name"], "example.com", "link: {link}");
    }
}

#[test]
fn tls_defaults_prefer_sni_over_host_when_present() {
    let o = parse_one("trojan://pw@example.com:443?sni=tls.example.com#t");
    assert_eq!(o["tls"]["enabled"], true);
    assert_eq!(o["tls"]["server_name"], "tls.example.com");
}

#[test]
fn tls_defaults_keep_insecure_flag_per_link() {
    let o = parse_one("hysteria2://hpw@example.com:443?insecure=1#h");
    assert_eq!(o["tls"]["enabled"], true);
    assert_eq!(o["tls"]["server_name"], "example.com");
    assert_eq!(o["tls"]["insecure"], true);

    let o = parse_one("hysteria2://hpw@example.com:443#h");
    assert!(o["tls"].get("insecure").is_none());
}

/// vless/vmess：仅 security=tls/reality 时带 tls 块，security=none 不带。
#[test]
fn vless_security_none_has_no_tls_block() {
    let link = format!("vless://{UUID}@example.com:443?security=none#plain");
    let o = parse_one(&link);
    assert!(o.get("tls").is_none());
}

// ---------- 项 2：真实 sing-box 二进制 check ----------

/// 探测可用的 sing-box 二进制：环境变量 `PROXY_PANEL_SINGBOX_BIN` → 用户目录
/// `~/.proxy-panel-client/cores/sing-box/1.14.0-beta.4/sing-box` → workspace
/// `target/test-cores/sing-box`（cargo 测试 cwd 为 crate 根目录）。
fn find_singbox_binary() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("PROXY_PANEL_SINGBOX_BIN") {
        let p = std::path::PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let p = std::path::PathBuf::from(home)
            .join(".proxy-panel-client/cores/sing-box/1.14.0-beta.4/sing-box");
        if p.is_file() {
            return Some(p);
        }
    }
    for p in [
        "../../target/test-cores/sing-box",
        "target/test-cores/sing-box",
    ] {
        let p = std::path::PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// 用真实 sing-box 二进制校验含 trojan/hy2/tuic/anytls（均无 sni 参数）节点的
/// 完整配置通过 `sing-box check`；无本地二进制时跳过（不失败）。
#[test]
fn singbox_check_accepts_tls_default_nodes_without_sni() {
    let Some(bin) = find_singbox_binary() else {
        eprintln!("no sing-box binary found, skipping real-core check");
        return;
    };
    let links = format!(
        "trojan://pw@example.com:443#t\n\
         hysteria2://hpw@example.com:443#h\n\
         tuic://{UUID}:tupw@example.com:443#u\n\
         anytls://apw@example.com:443#a\n"
    );
    let result = parse_share_links(&links);
    assert!(
        result.warnings.is_empty(),
        "unexpected warnings: {:?}",
        result.warnings
    );
    let mut outbounds: Vec<Value> = result
        .nodes
        .iter()
        .map(|n| n.outbound_singbox.clone())
        .collect();
    outbounds.push(json!({ "type": "direct", "tag": "direct" }));
    let config = json!({
        "log": { "level": "error" },
        "inbounds": [{ "type": "mixed", "listen": "127.0.0.1", "listen_port": 17890 }],
        "outbounds": outbounds,
        "route": { "final": "direct" },
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("check.json");
    std::fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
    let output = std::process::Command::new(&bin)
        .args(["check", "-c", path.to_str().unwrap()])
        .output()
        .expect("failed to run sing-box");
    assert!(
        output.status.success(),
        "sing-box check failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

// ---------- 双核心产出（已移除，仅保留 sing-box） ----------

#[test]
fn each_node_produces_singbox_outbound() {
    let link = format!(
        "vless://{UUID}@example.com:443?security=reality&sni=example.com&fp=chrome&pbk=PK&sid=ab#dual"
    );
    let result = parse_share_links(&link);
    assert_eq!(result.nodes.len(), 1);
    let node = &result.nodes[0];
    assert_eq!(node.name, "dual");
    assert_eq!(node.outbound_singbox["type"], "vless");
    assert_eq!(node.outbound_singbox["tag"], "dual");
}

// ---------- 失败行 warning / 跳过 ----------

#[test]
fn malformed_line_produces_warning_and_is_skipped() {
    let content = format!(
        "vless://{UUID}@example.com:443?security=tls&sni=example.com#ok\n\
         trojan://broken-link-without-at\n\
         vless://{UUID}@example.com:443?security=tls&sni=example.com#also-ok"
    );
    let result = parse_share_links(&content);
    assert_eq!(result.nodes.len(), 2);
    assert_eq!(result.warnings.len(), 1);
    assert!(result.warnings[0].contains("trojan"));
}

#[test]
fn non_share_link_and_unknown_scheme_lines_are_skipped_silently() {
    let content = "random text line\n\
                   socks5://user:pass@example.com:1080\n\
                   vless://{UUID}@example.com:443?sni=example.com#real\n";
    let result = parse_share_links(content);
    assert_eq!(result.nodes.len(), 1);
    assert_eq!(result.nodes[0].name, "real");
    assert!(result.warnings.is_empty());
}

#[test]
fn parses_ipv6_host() {
    let link = format!("vless://{UUID}@[::1]:443?security=tls&sni=example.com#v6");
    let o = parse_one(&link);
    assert_eq!(o["server"], "::1");
    assert_eq!(o["server_port"], 443);
}

#[test]
fn percent_decodes_fragment_names() {
    let o = parse_one("anytls://pw@example.com:443?sni=example.com#My%20Node%201");
    assert_eq!(o["tag"], "My Node 1");
}
