//! clash/mihomo proxy → sing-box outbound 单向映射（ClashYaml 订阅转 sing-box）。
//!
//! 覆盖 ss / vmess / vless / trojan / hysteria2 / tuic / anytls 的常见字段
//! （tls / reality / ws / grpc / http 传输等）；未覆盖的协议类型跳过（返回 `None`）。

use serde_json::{Map, Value, json};

/// clash/mihomo proxy → sing-box outbound。未覆盖的 proxy 类型返回 `None`。
pub fn mihomo_to_singbox(p: &Value) -> Option<Value> {
    let name = p.get("name")?.as_str()?;
    let ptype = p.get("type")?.as_str()?;
    let mut o = Map::new();
    o.insert(String::from("tag"), Value::String(name.to_string()));
    o.insert(String::from("server"), p.get("server")?.clone());
    o.insert(String::from("server_port"), p.get("port")?.clone());
    match ptype {
        "ss" | "shadowsocks" => {
            o.insert(String::from("type"), json!("shadowsocks"));
            o.insert(String::from("method"), p.get("cipher")?.clone());
            o.insert(String::from("password"), p.get("password")?.clone());
        }
        "vmess" => {
            o.insert(String::from("type"), json!("vmess"));
            o.insert(String::from("uuid"), p.get("uuid")?.clone());
            o.insert(
                String::from("alter_id"),
                p.get("alterId").cloned().unwrap_or(json!(0)),
            );
            o.insert(
                String::from("security"),
                p.get("cipher").cloned().unwrap_or(json!("auto")),
            );
            tls_from_mihomo(p, &mut o);
            transport_from_mihomo(p, &mut o);
        }
        "vless" => {
            o.insert(String::from("type"), json!("vless"));
            o.insert(String::from("uuid"), p.get("uuid")?.clone());
            if let Some(flow) = p.get("flow").and_then(Value::as_str)
                && !flow.is_empty()
            {
                o.insert(String::from("flow"), json!(flow));
            }
            tls_from_mihomo(p, &mut o);
            transport_from_mihomo(p, &mut o);
        }
        "trojan" => {
            o.insert(String::from("type"), json!("trojan"));
            o.insert(String::from("password"), p.get("password")?.clone());
            tls_from_mihomo(p, &mut o);
            ensure_tls_defaults(&mut o);
        }
        "hysteria2" | "hy2" => {
            o.insert(String::from("type"), json!("hysteria2"));
            o.insert(String::from("password"), p.get("password")?.clone());
            tls_from_mihomo(p, &mut o);
            ensure_tls_defaults(&mut o);
        }
        "tuic" => {
            o.insert(String::from("type"), json!("tuic"));
            o.insert(String::from("uuid"), p.get("uuid")?.clone());
            o.insert(String::from("password"), p.get("password")?.clone());
            if let Some(cc) = p.get("congestion-controller").and_then(Value::as_str)
                && !cc.is_empty()
            {
                o.insert(String::from("congestion_control"), json!(cc));
            }
            tls_from_mihomo(p, &mut o);
            ensure_tls_defaults(&mut o);
        }
        "anytls" => {
            o.insert(String::from("type"), json!("anytls"));
            o.insert(String::from("password"), p.get("password")?.clone());
            tls_from_mihomo(p, &mut o);
            ensure_tls_defaults(&mut o);
        }
        _ => return None,
    }
    Some(Value::Object(o))
}

/// sing-box outbound → clash/mihomo proxy. Unsupported leaf types return `None`.
pub fn singbox_to_mihomo(o: &Value) -> Option<Value> {
    let tag = o.get("tag")?.as_str()?;
    let ptype = o.get("type")?.as_str()?;
    let mut p = Map::new();
    p.insert(String::from("name"), Value::String(tag.to_string()));
    p.insert(String::from("server"), o.get("server")?.clone());
    p.insert(String::from("port"), o.get("server_port")?.clone());
    match ptype {
        "shadowsocks" => {
            p.insert(String::from("type"), json!("ss"));
            p.insert(String::from("cipher"), o.get("method")?.clone());
            p.insert(String::from("password"), o.get("password")?.clone());
        }
        "vmess" => {
            p.insert(String::from("type"), json!("vmess"));
            p.insert(String::from("uuid"), o.get("uuid")?.clone());
            p.insert(
                String::from("alterId"),
                o.get("alter_id").cloned().unwrap_or(json!(0)),
            );
            p.insert(
                String::from("cipher"),
                o.get("security").cloned().unwrap_or(json!("auto")),
            );
            tls_to_mihomo(o, &mut p);
            transport_to_mihomo(o.get("transport"), &mut p);
        }
        "vless" => {
            p.insert(String::from("type"), json!("vless"));
            p.insert(String::from("uuid"), o.get("uuid")?.clone());
            if let Some(flow) = o.get("flow").and_then(Value::as_str)
                && !flow.is_empty()
            {
                p.insert(String::from("flow"), json!(flow));
            }
            tls_to_mihomo(o, &mut p);
            transport_to_mihomo(o.get("transport"), &mut p);
        }
        "trojan" => {
            p.insert(String::from("type"), json!("trojan"));
            p.insert(String::from("password"), o.get("password")?.clone());
            tls_to_mihomo(o, &mut p);
        }
        "hysteria2" => {
            p.insert(String::from("type"), json!("hysteria2"));
            p.insert(String::from("password"), o.get("password")?.clone());
            tls_to_mihomo(o, &mut p);
        }
        "tuic" => {
            p.insert(String::from("type"), json!("tuic"));
            p.insert(String::from("uuid"), o.get("uuid")?.clone());
            p.insert(String::from("password"), o.get("password")?.clone());
            if let Some(cc) = o.get("congestion_control").and_then(Value::as_str)
                && !cc.is_empty()
            {
                p.insert(String::from("congestion-controller"), json!(cc));
            }
            tls_to_mihomo(o, &mut p);
        }
        "anytls" => {
            p.insert(String::from("type"), json!("anytls"));
            p.insert(String::from("password"), o.get("password")?.clone());
            tls_to_mihomo(o, &mut p);
        }
        _ => return None,
    }
    Some(Value::Object(p))
}

fn tls_to_mihomo(o: &Value, p: &mut Map<String, Value>) {
    let tls = match o.get("tls") {
        Some(t) if t.is_object() => t,
        _ => return,
    };
    if tls.get("enabled").and_then(Value::as_bool).unwrap_or(false) {
        p.insert(String::from("tls"), json!(true));
    }
    if let Some(sn) = tls.get("server_name").and_then(Value::as_str)
        && !sn.is_empty()
    {
        p.insert(String::from("servername"), json!(sn));
    }
    if tls.get("insecure").and_then(Value::as_bool) == Some(true) {
        p.insert(String::from("skip-cert-verify"), json!(true));
    }
    if let Some(fp) = tls
        .get("utls")
        .and_then(|u| u.get("fingerprint"))
        .and_then(Value::as_str)
        && !fp.is_empty()
    {
        p.insert(String::from("client-fingerprint"), json!(fp));
    }
    let reality = match tls.get("reality") {
        Some(r) if r.is_object() => r,
        _ => return,
    };
    if reality.get("enabled").and_then(Value::as_bool) != Some(true) {
        return;
    }
    let mut opts = Map::new();
    if let Some(pk) = reality.get("public_key").and_then(Value::as_str)
        && !pk.is_empty()
    {
        opts.insert(String::from("public-key"), json!(pk));
    }
    if let Some(sid) = reality.get("short_id").and_then(Value::as_str)
        && !sid.is_empty()
    {
        opts.insert(String::from("short-id"), json!(sid));
    }
    if !opts.is_empty() {
        p.insert(String::from("reality-opts"), Value::Object(opts));
    }
}

fn transport_to_mihomo(t: Option<&Value>, p: &mut Map<String, Value>) {
    let t = match t {
        Some(t) if t.is_object() => t,
        _ => return,
    };
    let ttype = t.get("type").and_then(Value::as_str).unwrap_or("tcp");
    match ttype {
        "ws" => {
            p.insert(String::from("network"), json!("ws"));
            let mut ws = Map::new();
            if let Some(path) = t.get("path").and_then(Value::as_str)
                && !path.is_empty()
            {
                ws.insert(String::from("path"), json!(path));
            }
            if let Some(hdrs) = t.get("headers").and_then(Value::as_object)
                && let Some(host) = hdrs.get("Host").and_then(Value::as_str)
                && !host.is_empty()
            {
                ws.insert(String::from("headers"), json!({ "Host": host }));
            }
            if !ws.is_empty() {
                p.insert(String::from("ws-opts"), Value::Object(ws));
            }
        }
        "grpc" => {
            p.insert(String::from("network"), json!("grpc"));
            if let Some(sn) = t.get("service_name").and_then(Value::as_str)
                && !sn.is_empty()
            {
                p.insert(
                    String::from("grpc-opts"),
                    json!({ "grpc-service-name": sn }),
                );
            }
        }
        "http" => {
            p.insert(String::from("network"), json!("http"));
            let mut opts = Map::new();
            if let Some(path) = t.get("path").and_then(Value::as_str)
                && !path.is_empty()
            {
                opts.insert(String::from("path"), json!(path));
            }
            if !opts.is_empty() {
                p.insert(String::from("http-opts"), Value::Object(opts));
            }
        }
        _ => {
            p.insert(String::from("network"), json!("tcp"));
        }
    }
}

/// mihomo 侧 tls 字段 → sing-box `tls` 块（server_name / insecure / utls / reality）。
fn tls_from_mihomo(p: &Value, o: &mut Map<String, Value>) {
    let enabled = p.get("tls").and_then(Value::as_bool).unwrap_or(false);
    let mut tls = Map::new();
    tls.insert(String::from("enabled"), json!(enabled));
    // `servername` 与 `sni` 均为 Clash Meta 中常见的 TLS 主机名字段（vless/vmess 用
    // servername，trojan/hy2/tuic/anytls 常用 sni），两者皆取。
    let sn = p
        .get("servername")
        .or_else(|| p.get("sni"))
        .and_then(Value::as_str);
    if let Some(sn) = sn
        && !sn.is_empty()
    {
        tls.insert(String::from("server_name"), json!(sn));
    }
    if p.get("skip-cert-verify").and_then(Value::as_bool) == Some(true) {
        tls.insert(String::from("insecure"), json!(true));
    }
    if let Some(fp) = p.get("client-fingerprint").and_then(Value::as_str)
        && !fp.is_empty()
    {
        tls.insert(
            String::from("utls"),
            json!({ "enabled": true, "fingerprint": fp }),
        );
    }
    if let Some(opts) = p.get("reality-opts").and_then(Value::as_object) {
        let mut r = Map::new();
        r.insert(String::from("enabled"), json!(true));
        if let Some(pk) = opts.get("public-key").and_then(Value::as_str)
            && !pk.is_empty()
        {
            r.insert(String::from("public_key"), json!(pk));
        }
        if let Some(sid) = opts.get("short-id").and_then(Value::as_str)
            && !sid.is_empty()
        {
            r.insert(String::from("short_id"), json!(sid));
        }
        tls.insert(String::from("reality"), Value::Object(r));
    }
    if enabled || tls.len() > 1 {
        o.insert(String::from("tls"), Value::Object(tls));
    }
}

/// 为 TLS 型协议（trojan / hysteria2 / tuic / anytls）的 sing-box outbound 无条件
/// 补全 `tls: {enabled: true, server_name: ...}`：已有 server_name 保留，缺失时回退
/// host；insecure 等已有字段保留。避免 mihomo 侧未标 `tls: true` 时生成的 outbound
/// 缺 TLS 块导致 sing-box `initialize outbound: TLS required` FATAL。
fn ensure_tls_defaults(o: &mut Map<String, Value>) {
    let host = o
        .get("server")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let tls = match o.get_mut("tls") {
        Some(t) if t.is_object() => t
            .as_object_mut()
            .expect("checked is_object in ensure_tls_defaults"),
        _ => {
            o.insert(String::from("tls"), json!({}));
            o.get_mut("tls")
                .and_then(Value::as_object_mut)
                .expect("just inserted tls object")
        }
    };
    tls.insert(String::from("enabled"), json!(true));
    if !tls.contains_key("server_name") && !host.is_empty() {
        tls.insert(String::from("server_name"), json!(host));
    }
}

/// mihomo 侧 network / ws-opts / grpc-opts / http-opts → sing-box `transport` 块。
fn transport_from_mihomo(p: &Value, o: &mut Map<String, Value>) {
    let network = p.get("network").and_then(Value::as_str).unwrap_or("tcp");
    match network {
        "ws" => {
            let mut t = Map::new();
            t.insert(String::from("type"), json!("ws"));
            if let Some(opts) = p.get("ws-opts").and_then(Value::as_object) {
                if let Some(path) = opts.get("path").and_then(Value::as_str)
                    && !path.is_empty()
                {
                    t.insert(String::from("path"), json!(path));
                }
                if let Some(hdrs) = opts.get("headers").and_then(Value::as_object)
                    && let Some(host) = hdrs.get("Host").and_then(Value::as_str)
                    && !host.is_empty()
                {
                    t.insert(String::from("headers"), json!({ "Host": host }));
                }
            }
            o.insert(String::from("transport"), Value::Object(t));
        }
        "grpc" => {
            let mut t = Map::new();
            t.insert(String::from("type"), json!("grpc"));
            if let Some(opts) = p.get("grpc-opts").and_then(Value::as_object)
                && let Some(sn) = opts.get("grpc-service-name").and_then(Value::as_str)
                && !sn.is_empty()
            {
                t.insert(String::from("service_name"), json!(sn));
            }
            o.insert(String::from("transport"), Value::Object(t));
        }
        "http" => {
            let mut t = Map::new();
            t.insert(String::from("type"), json!("http"));
            if let Some(opts) = p.get("http-opts").and_then(Value::as_object)
                && let Some(path) = opts.get("path").and_then(Value::as_str)
                && !path.is_empty()
            {
                t.insert(String::from("path"), json!(path));
            }
            o.insert(String::from("transport"), Value::Object(t));
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "node_convert/tests.rs"]
mod tests;
