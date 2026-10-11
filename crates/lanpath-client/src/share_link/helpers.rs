//! Shared URI decoding and transport helpers.
use base64::Engine as _;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

// ---------- 通用工具 ----------

pub(super) fn split_fragment(s: &str) -> (&str, &str) {
    match s.split_once('#') {
        Some((a, b)) => (a, b),
        None => (s, ""),
    }
}

pub(super) fn split_query(s: &str) -> (&str, &str) {
    match s.split_once('?') {
        Some((a, b)) => (a, b),
        None => (s, ""),
    }
}

pub(super) fn parse_query(s: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if s.is_empty() {
        return map;
    }
    for pair in s.split('&') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        match pair.split_once('=') {
            Some((k, v)) => {
                map.insert(percent_decode(k), percent_decode(v));
            }
            None => {
                map.insert(percent_decode(pair), String::new());
            }
        }
    }
    map
}

/// 解析 `host:port`；IPv6 支持 `[::1]:443`。
pub(super) fn parse_host_port(s: &str) -> Option<(String, u16)> {
    let s = s.trim();
    if let Some(inner) = s.strip_prefix('[') {
        let (host, rest) = inner.split_once(']')?;
        let port = rest.strip_prefix(':')?.trim();
        let port: u16 = port.parse().ok()?;
        return Some((host.to_string(), port));
    }
    let (host, port) = s.rsplit_once(':')?;
    let port: u16 = port.trim().parse().ok()?;
    Some((host.to_string(), port))
}

pub(super) fn fallback_name(name: &str, server: &str, port: u16) -> String {
    if name.is_empty() {
        format!("{server}:{port}")
    } else {
        name.to_string()
    }
}

/// 支持标准与 URL-safe 两种 base64 字母表，容忍缺失 padding。
pub(crate) fn b64_decode(s: &str) -> Option<String> {
    let s = s.trim();
    let mut padded = s.to_string();
    while !padded.len().is_multiple_of(4) {
        padded.push('=');
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(padded.as_bytes())
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(padded.as_bytes()))
        .ok()?;
    String::from_utf8(bytes).ok()
}

pub(super) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex_val(b[i + 1]), hex_val(b[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

pub(super) fn port_of(v: Option<&Value>) -> Option<u16> {
    let v = v?;
    if let Some(n) = v.as_u64() {
        return u16::try_from(n).ok();
    }
    if let Some(n) = v.as_i64() {
        return u16::try_from(n).ok();
    }
    v.as_str()?.parse().ok()
}

pub(super) fn as_u64(v: Option<&Value>) -> Option<u64> {
    let v = v?;
    if let Some(n) = v.as_u64() {
        return Some(n);
    }
    if let Some(n) = v.as_i64() {
        return u64::try_from(n).ok();
    }
    v.as_str()?.parse().ok()
}

/// 把 "1" / "true" / "yes" / "on" 视为 true。
pub(super) fn truthy(v: Option<&String>) -> bool {
    match v.map(String::as_str) {
        Some(s) => matches!(s.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        None => false,
    }
}

/// vmess 的传输层构建（ws / grpc / http，其余视为 tcp 不输出）。
pub(super) fn build_vmess_transport(net: &str, host: &str, path: &str) -> Option<Value> {
    match net {
        "ws" => {
            let mut t = Map::new();
            t.insert(String::from("type"), json!("ws"));
            if !path.is_empty() {
                t.insert(String::from("path"), json!(path));
            }
            if !host.is_empty() {
                t.insert(String::from("headers"), json!({ "Host": host }));
            }
            Some(Value::Object(t))
        }
        "grpc" => {
            let mut t = Map::new();
            t.insert(String::from("type"), json!("grpc"));
            if !path.is_empty() {
                t.insert(String::from("service_name"), json!(path));
            }
            if !host.is_empty() {
                t.insert(String::from("authority"), json!(host));
            }
            Some(Value::Object(t))
        }
        "http" | "h2" | "httpupgrade" => {
            let mut t = Map::new();
            t.insert(String::from("type"), json!("http"));
            if !path.is_empty() {
                t.insert(String::from("path"), json!(path));
            }
            if !host.is_empty() {
                t.insert(String::from("host"), json!(host));
            }
            Some(Value::Object(t))
        }
        _ => None,
    }
}
