//! 分享链接解析：ss / vmess / vless / trojan / hysteria2 / tuic / anytls。
//!
//! 按行解析，每条链接产出 sing-box outbound 节点。
//! 单行解析失败跳过并记 warning；非分享链接行（无 `://` 或未知 scheme）静默跳过。

mod helpers;

pub(crate) use helpers::b64_decode;
use helpers::{
    as_u64, build_vmess_transport, fallback_name, parse_host_port, parse_query, percent_decode,
    port_of, split_fragment, split_query, truthy,
};
use lanpath_common::{LanpathError, LanpathResult};
use serde_json::{Map, Value, json};

/// 单个解析出的分享链接节点。
#[derive(Debug, Clone)]
pub struct ShareNode {
    /// 节点名称（分享链接 `#name`，缺省为 `host:port`）。
    pub name: String,
    /// sing-box outbound（含 `tag`）。
    pub outbound_singbox: Value,
}

/// 分享链接解析结果：节点列表 + 每行失败时的 warning。
#[derive(Debug, Clone, Default)]
pub struct ShareLinkParseResult {
    pub nodes: Vec<ShareNode>,
    pub warnings: Vec<String>,
}

/// 按行解析分享链接文本。单行失败跳过并记 warning；
/// 非分享链接行（无 `://` 或未知 scheme）静默跳过。
pub fn parse_share_links(content: &str) -> ShareLinkParseResult {
    let mut result = ShareLinkParseResult::default();
    for (idx, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match parse_share_link(line) {
            Ok(Some(node)) => result.nodes.push(node),
            Ok(None) => {}
            Err(e) => result.warnings.push(format!("line {}: {e}", idx + 1)),
        }
    }
    result
}

/// 解析单条分享链接。返回 `Ok(None)` 表示该行不是本模块支持的分享链接。
fn parse_share_link(line: &str) -> LanpathResult<Option<ShareNode>> {
    let (scheme, rest) = match line.split_once("://") {
        Some(s) => s,
        None => return Ok(None),
    };
    let outbound = match scheme {
        "ss" => parse_ss(rest),
        "vmess" => parse_vmess(rest),
        "vless" => parse_vless(rest),
        "trojan" => parse_trojan(rest),
        "hysteria2" | "hy2" => parse_hysteria2(rest),
        "tuic" => parse_tuic(rest),
        "anytls" => parse_anytls(rest),
        _ => return Ok(None),
    };
    let outbound = outbound
        .ok_or_else(|| LanpathError::Client(format!("failed to parse {scheme}:// share link")))?;
    let name = outbound["tag"].as_str().unwrap_or_default().to_string();
    Ok(Some(ShareNode {
        name,
        outbound_singbox: outbound,
    }))
}

/// `ss://`（SIP002：base64(method:password)@host:port，或旧式整体 base64）。
fn parse_ss(rest: &str) -> Option<Value> {
    let (body, frag) = split_fragment(rest);
    let (authority, _query) = split_query(body);
    let name = percent_decode(frag);

    // 旧式整体 base64：method:password@host:port。含 '@' 时直接按明文处理。
    let decoded_legacy: String;
    let userinfo_hostport: &str = if authority.contains('@') {
        authority
    } else {
        decoded_legacy = b64_decode(authority)?;
        decoded_legacy.as_str()
    };

    let (userinfo, hostport) = userinfo_hostport.split_once('@')?;
    let (server, port) = parse_host_port(hostport)?;

    // SIP002 的 userinfo 是 base64(method:password)；含 ':' 时按明文处理。
    let decoded_cred: String;
    let cred: &str = if userinfo.contains(':') {
        userinfo
    } else {
        match b64_decode(userinfo) {
            Some(d) => {
                decoded_cred = d;
                decoded_cred.as_str()
            }
            None => userinfo,
        }
    };
    let (method, password) = cred.split_once(':')?;

    let mut o = json!({
        "type": "shadowsocks",
        "server": server,
        "server_port": port,
        "method": method,
        "password": password,
    });
    o["tag"] = json!(fallback_name(&name, &server, port));
    Some(o)
}

/// `vmess://` base64 JSON（v/ps/add/port/id/aid/net/type/host/path/tls/sni/fp）。
fn parse_vmess(rest: &str) -> Option<Value> {
    let (body, _frag) = split_fragment(rest);
    let body = body.trim();
    let json_str = if body.starts_with('{') {
        body.to_string()
    } else {
        b64_decode(body)?
    };
    let v: Value = serde_json::from_str(&json_str).ok()?;

    let add = v.get("add").and_then(Value::as_str)?.to_string();
    let port = port_of(v.get("port"))?;
    let id = v.get("id").and_then(Value::as_str)?.to_string();
    let name = v
        .get("ps")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| format!("{add}:{port}"));
    let net = v.get("net").and_then(Value::as_str).unwrap_or("tcp");
    let host = v.get("host").and_then(Value::as_str).unwrap_or_default();
    let path = v.get("path").and_then(Value::as_str).unwrap_or_default();
    let tls_enabled = v
        .get("tls")
        .and_then(Value::as_str)
        .is_some_and(|s| s == "tls" || s == "1");
    let sni = v
        .get("sni")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let fp = v
        .get("fp")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let scy = v
        .get("scy")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let aid = as_u64(v.get("aid")).unwrap_or(0);

    let mut o = json!({
        "type": "vmess",
        "server": add,
        "server_port": port,
        "uuid": id,
        "alter_id": aid,
        "security": scy.unwrap_or("auto"),
    });
    o["tag"] = json!(name);
    if tls_enabled {
        let mut tls = Map::new();
        tls.insert(String::from("enabled"), json!(true));
        let server_name = sni
            .filter(|s| !s.is_empty())
            .or_else(|| (!host.is_empty()).then_some(host));
        if let Some(sn) = server_name {
            tls.insert(String::from("server_name"), json!(sn));
        }
        if let Some(f) = fp {
            tls.insert(
                String::from("utls"),
                json!({ "enabled": true, "fingerprint": f }),
            );
        }
        o["tls"] = Value::Object(tls);
    }
    if let Some(t) = build_vmess_transport(net, host, path) {
        o["transport"] = t;
    }
    Some(o)
}

/// `vless://` uuid@host:port 带 query（security/flow/sni/fp/pbk/sid/type/path/serviceName）。
fn parse_vless(rest: &str) -> Option<Value> {
    let (body, frag) = split_fragment(rest);
    let (authority, query) = split_query(body);
    let name = percent_decode(frag);
    let (uuid, hostport) = authority.split_once('@')?;
    let (server, port) = parse_host_port(hostport)?;
    let params = parse_query(query);

    let security = params.get("security").map(String::as_str).unwrap_or("");
    let tls_enabled = security == "tls" || security == "reality";
    let flow = params.get("flow").filter(|s| !s.is_empty());
    let sni = params.get("sni").filter(|s| !s.is_empty());
    let fp = params.get("fp").filter(|s| !s.is_empty());
    let pbk = params.get("pbk").filter(|s| !s.is_empty());
    let sid = params.get("sid").filter(|s| !s.is_empty());
    let net = params.get("type").map(String::as_str).unwrap_or("tcp");
    let path = params.get("path").filter(|s| !s.is_empty());
    let service_name = params.get("serviceName").filter(|s| !s.is_empty());
    let ws_host = params.get("host").filter(|s| !s.is_empty());

    let mut o = json!({
        "type": "vless",
        "server": server,
        "server_port": port,
        "uuid": uuid,
        "packet_encoding": "xudp",
    });
    o["tag"] = json!(fallback_name(&name, &server, port));
    if let Some(f) = flow {
        o["flow"] = json!(f);
    }
    if tls_enabled {
        let mut tls = Map::new();
        tls.insert(String::from("enabled"), json!(true));
        if let Some(s) = sni {
            tls.insert(String::from("server_name"), json!(s));
        }
        if let Some(f) = fp {
            tls.insert(
                String::from("utls"),
                json!({ "enabled": true, "fingerprint": f }),
            );
        }
        if security == "reality" {
            let mut r = Map::new();
            r.insert(String::from("enabled"), json!(true));
            if let Some(p) = pbk {
                r.insert(String::from("public_key"), json!(p));
            }
            if let Some(s) = sid {
                r.insert(String::from("short_id"), json!(s));
            }
            tls.insert(String::from("reality"), Value::Object(r));
        }
        o["tls"] = Value::Object(tls);
    }
    match net {
        "ws" => {
            let mut t = Map::new();
            t.insert(String::from("type"), json!("ws"));
            if let Some(p) = path {
                t.insert(String::from("path"), json!(p));
            }
            if let Some(h) = ws_host {
                t.insert(String::from("headers"), json!({ "Host": h }));
            }
            o["transport"] = Value::Object(t);
        }
        "grpc" => {
            let mut t = Map::new();
            t.insert(String::from("type"), json!("grpc"));
            if let Some(sn) = service_name {
                t.insert(String::from("service_name"), json!(sn));
            }
            o["transport"] = Value::Object(t);
        }
        "http" | "h2" => {
            let mut t = Map::new();
            t.insert(String::from("type"), json!("http"));
            if let Some(p) = path {
                t.insert(String::from("path"), json!(p));
            }
            o["transport"] = Value::Object(t);
        }
        _ => {}
    }
    Some(o)
}

/// `trojan://` password@host:port?sni&allowInsecure#name。
fn parse_trojan(rest: &str) -> Option<Value> {
    let (body, frag) = split_fragment(rest);
    let (authority, query) = split_query(body);
    let name = percent_decode(frag);
    let (password, hostport) = authority.split_once('@')?;
    let (server, port) = parse_host_port(hostport)?;
    let params = parse_query(query);
    let sni = params.get("sni").filter(|s| !s.is_empty());
    let allow_insecure = truthy(
        params
            .get("allowInsecure")
            .or_else(|| params.get("insecure")),
    );
    // 无条件带 TLS：无 sni 时 server_name 回退 host（否则 sing-box 报 TLS required）。
    let server_name = sni.cloned().unwrap_or_else(|| server.clone());

    let mut o = json!({
        "type": "trojan",
        "server": server,
        "server_port": port,
        "password": password,
        "tls": { "enabled": true, "server_name": server_name },
    });
    o["tag"] = json!(fallback_name(&name, &server, port));
    if allow_insecure {
        o["tls"]["insecure"] = json!(true);
    }
    Some(o)
}

/// `hysteria2://` / `hy2://` password@host:port?sni&insecure#name。
fn parse_hysteria2(rest: &str) -> Option<Value> {
    let (body, frag) = split_fragment(rest);
    let (authority, query) = split_query(body);
    let name = percent_decode(frag);
    let (password, hostport) = authority.split_once('@')?;
    let (server, port) = parse_host_port(hostport)?;
    let params = parse_query(query);
    let sni = params.get("sni").filter(|s| !s.is_empty());
    let insecure = truthy(params.get("insecure"));
    // 无条件带 TLS：无 sni 时 server_name 回退 host。
    let server_name = sni.cloned().unwrap_or_else(|| server.clone());

    let mut o = json!({
        "type": "hysteria2",
        "server": server,
        "server_port": port,
        "password": password,
        "tls": { "enabled": true, "server_name": server_name },
    });
    o["tag"] = json!(fallback_name(&name, &server, port));
    if insecure {
        o["tls"]["insecure"] = json!(true);
    }
    Some(o)
}

/// `tuic://` uuid:password@host:port?sni&congestion_control#name。
fn parse_tuic(rest: &str) -> Option<Value> {
    let (body, frag) = split_fragment(rest);
    let (authority, query) = split_query(body);
    let name = percent_decode(frag);
    let (uuid_password, hostport) = authority.split_once('@')?;
    let (uuid, password) = uuid_password.split_once(':')?;
    let (server, port) = parse_host_port(hostport)?;
    let params = parse_query(query);
    let sni = params.get("sni").filter(|s| !s.is_empty());
    let cc = params.get("congestion_control").filter(|s| !s.is_empty());
    // 无条件带 TLS：无 sni 时 server_name 回退 host。
    let server_name = sni.cloned().unwrap_or_else(|| server.clone());

    let mut o = json!({
        "type": "tuic",
        "server": server,
        "server_port": port,
        "uuid": uuid,
        "password": password,
        "tls": { "enabled": true, "server_name": server_name },
    });
    o["tag"] = json!(fallback_name(&name, &server, port));
    if let Some(c) = cc {
        o["congestion_control"] = json!(c);
    }
    Some(o)
}

/// `anytls://` password@host:port?sni#name。
fn parse_anytls(rest: &str) -> Option<Value> {
    let (body, frag) = split_fragment(rest);
    let (authority, query) = split_query(body);
    let name = percent_decode(frag);
    let (password, hostport) = authority.split_once('@')?;
    let (server, port) = parse_host_port(hostport)?;
    let params = parse_query(query);
    let sni = params.get("sni").filter(|s| !s.is_empty());
    // 无条件带 TLS：无 sni 时 server_name 回退 host。
    let server_name = sni.cloned().unwrap_or_else(|| server.clone());

    let mut o = json!({
        "type": "anytls",
        "server": server,
        "server_port": port,
        "password": password,
        "tls": { "enabled": true, "server_name": server_name },
    });
    o["tag"] = json!(fallback_name(&name, &server, port));
    Some(o)
}

#[cfg(test)]
mod tests;
