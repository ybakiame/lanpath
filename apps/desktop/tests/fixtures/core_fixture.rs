//! Test-only fake proxy core: real process, real listeners, real exit codes.
//!
//! Compiled at test time by `rustc` (see `src/core_fixture.rs` and
//! `tests/common/mod.rs`), linked against the Cargo-built `serde_json` rlib.
//! It emulates the narrow CLI surface the runtime and the config validator
//! use:
//!
//! - `core_fixture version` / `core_fixture -v` — version probes;
//! - `core_fixture check -c <file> -D <dir>` — sing-box config check;
//! - `core_fixture -t -d <dir> -f <file>` — Mihomo config test;
//! - `core_fixture run -c <file> -D <dir>` / `core_fixture -d <dir> -f <file>`
//!   — run until killed, binding the mixed listener(s) and the loopback Clash
//!   API endpoint described by the config, answering `GET /version` with 200.
//!
//! Behavior switches are read from the config text itself (JSON pretty or
//! YAML `key: value` lines — a deliberate text scan so the fixture links no
//! YAML crate): `fixture_fail` (stderr + exit 1), `fixture_sleep_secs`
//! (stall; timeout tests), `fixture_no_listen` (run without binding),
//! `fixture_skip_api` (bind mixed but not the API), `fixture_exit_after_ms`
//! (exit 1 after a delay). A sidecar file `<binary>.behavior` next to the
//! executable supplies the same switches (as JSON) when the caller composes
//! the config itself (the runtime tests); config fields win on conflicts.
//! This is a fixture, not a real sing-box/Mihomo: real-core validation on
//! Windows stays with the main agent.
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

const SWITCHES: &[&str] = &[
    "fixture_fail",
    "fixture_sleep_secs",
    "fixture_no_listen",
    "fixture_skip_api",
    "fixture_exit_after_ms",
    "fixture_write_bogus_pid",
    "fixture_delete_pid_file_after_ms",
    "fixture_api_delay_ms",
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "version") {
        println!("sing-box version {}", reported_version("1.2.3"));
        return;
    }
    if args.iter().any(|arg| arg == "-v") {
        println!("Mihomo Meta v{} linux amd64", reported_version("1.19.29"));
        return;
    }
    let config_path = args
        .windows(2)
        .find(|pair| pair[0] == "-c" || pair[0] == "-f")
        .map(|pair| pair[1].clone());
    let Some(config_path) = config_path else {
        eprintln!("fixture requires -c <file> or -f <file>");
        std::process::exit(2);
    };
    let bytes = std::fs::read(&config_path).expect("fixture reads its config");
    let text = String::from_utf8_lossy(&bytes);
    let mut behavior = text_switches(&text);
    // Sidecar behavior（见模块文档）：config 字段优先。
    if let Some(sidecar) = std::env::current_exe()
        .ok()
        .and_then(|exe| std::fs::read_to_string(exe.with_extension("behavior")).ok())
        .and_then(|sidecar| serde_json::from_str::<BTreeMap<String, serde_json::Value>>(&sidecar).ok())
    {
        for (key, value) in sidecar {
            let value = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            behavior.entry(key).or_insert(value);
        }
    }
    if let Some(message) = behavior.get("fixture_fail") {
        eprintln!("{message}");
        std::process::exit(1);
    }
    if let Some(secs) = behavior
        .get("fixture_sleep_secs")
        .and_then(|value| value.parse::<u64>().ok())
    {
        std::thread::sleep(Duration::from_secs(secs));
    }
    let check_only = args.iter().any(|arg| arg == "check" || arg == "-t");
    if check_only {
        return;
    }
    // 核心工作目录（sing-box `-D` / Mihomo `-d`）：pid 故障注入用。
    let config_dir = args
        .windows(2)
        .find(|pair| pair[0] == "-D" || pair[0] == "-d")
        .map(|pair| std::path::PathBuf::from(&pair[1]));
    // 启动早期写损坏的 pid 记录：引擎写身份记录（create_new）必然失败，
    // 用于驱动“引擎已回收子进程、本层回滚再回收失败”的组合错误路径。
    if behavior.contains_key("fixture_write_bogus_pid")
        && let Some(dir) = &config_dir
    {
        for rel in ["runtime/sing-box/sing-box.pid", "runtime/mihomo/mihomo.pid"] {
            let path = dir.join(rel);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, b"bogus pid record");
        }
    }
    // 延迟删除 pid 身份记录：引擎在 spawn 约 500ms 后写记录，延迟删除
    // 保证发生在写入之后、运行时读回之前（配合 API 延迟把读回推后）。
    if let Some(ms) = behavior
        .get("fixture_delete_pid_file_after_ms")
        .and_then(|value| value.parse::<u64>().ok())
        && let Some(dir) = config_dir
    {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(ms));
            for rel in ["runtime/sing-box/sing-box.pid", "runtime/mihomo/mihomo.pid"] {
                let _ = std::fs::remove_file(dir.join(rel));
            }
        });
    }
    if !behavior.contains_key("fixture_no_listen") {
        let skip_api = behavior.contains_key("fixture_skip_api");
        let api_delay_ms = behavior
            .get("fixture_api_delay_ms")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(json) => bind_singbox_json(&json, skip_api, api_delay_ms),
            Err(_) => bind_mihomo_yaml(&text, skip_api, api_delay_ms),
        }
    }
    if let Some(ms) = behavior
        .get("fixture_exit_after_ms")
        .and_then(|value| value.parse::<u64>().ok())
    {
        std::thread::sleep(Duration::from_millis(ms));
        std::process::exit(1);
    }
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}

/// 从库存版本目录名推导上报版本（`cores/<binary>/<version>/<binary>` 布局；
/// 数字开头才采信，否则用默认值）——共享选择/激活按版本目录与 probe 输出
/// 精确匹配。
fn reported_version(default: &str) -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.parent()
                .and_then(|parent| parent.file_name())
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|name| {
            name.chars()
                .next()
                .is_some_and(|first| first.is_ascii_digit())
        })
        .unwrap_or_else(|| default.to_string())
}

/// 从配置文本扫描行为开关（JSON pretty 的 `"key": value` 与 YAML 的
/// `key: value` 行都能命中；逐行首个匹配生效）。
fn text_switches(text: &str) -> BTreeMap<String, String> {
    let mut switches = BTreeMap::new();
    for line in text.lines() {
        for key in SWITCHES {
            if let Some(value) = line_value(line, key) {
                switches.entry((*key).to_string()).or_insert(value);
            }
        }
    }
    switches
}

/// 匹配一行里的 `key: value`（key 可带引号；值去引号/尾逗号）。
fn line_value(line: &str, key: &str) -> Option<String> {
    let rest = line.trim_start();
    let rest = rest.strip_prefix('"').unwrap_or(rest);
    let rest = rest.strip_prefix(key)?;
    let rest = rest.strip_prefix('"').unwrap_or(rest);
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let value = rest
        .trim_end_matches(',')
        .trim()
        .trim_matches('"')
        .trim();
    Some(value.to_string())
}

fn bind_singbox_json(value: &serde_json::Value, skip_api: bool, api_delay_ms: u64) {
    if let Some(inbounds) = value.get("inbounds").and_then(|v| v.as_array()) {
        for inbound in inbounds {
            if inbound.get("type").and_then(|v| v.as_str()) != Some("mixed") {
                continue;
            }
            let host = inbound
                .get("listen")
                .and_then(|v| v.as_str())
                .unwrap_or("127.0.0.1");
            let port = inbound
                .get("listen_port")
                .and_then(|v| v.as_u64())
                .expect("mixed inbound has listen_port") as u16;
            spawn_listener(host, port, None, 0);
        }
    }
    if skip_api {
        return;
    }
    if let Some(controller) = value
        .pointer("/experimental/clash_api/external_controller")
        .and_then(|v| v.as_str())
        && let Some((host, port)) = controller.rsplit_once(':')
    {
        let port: u16 = port.parse().expect("api port");
        let state = build_clash_state(value);
        spawn_listener(host, port, Some(state), api_delay_ms);
    }
}

fn bind_mihomo_yaml(text: &str, skip_api: bool, api_delay_ms: u64) {
    for line in text.lines() {
        if let Some(port) = line_value(line, "mixed-port")
            .and_then(|value| value.parse::<u16>().ok())
        {
            spawn_listener("127.0.0.1", port, None, 0);
        }
        if !skip_api
            && let Some(controller) = line_value(line, "external-controller")
            && let Some((host, port)) = controller.rsplit_once(':')
        {
            let port: u16 = port.parse().expect("api port");
            let state = build_clash_state(&serde_json::json!({}));
            spawn_listener(host, port, Some(state), api_delay_ms);
        }
    }
}

struct ClashState {
    mode: String,
    proxies: serde_json::Map<String, serde_json::Value>,
    connections: Vec<serde_json::Value>,
}

fn build_clash_state(value: &serde_json::Value) -> ClashState {
    let mode = value
        .pointer("/experimental/clash_api/default_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("rule")
        .to_owned();
    let mut proxies = serde_json::Map::new();
    if let Some(outbounds) = value.get("outbounds").and_then(|v| v.as_array()) {
        for item in outbounds {
            let Some(tag) = item.get("tag").and_then(|v| v.as_str()) else {
                continue;
            };
            let kind = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let clash_type = match kind {
                "selector" => "Selector",
                "urltest" => "URLTest",
                "direct" => "Direct",
                "block" => "Block",
                "shadowsocks" => "Shadowsocks",
                "vmess" => "VMess",
                "vless" => "VLESS",
                "trojan" => "Trojan",
                other => other,
            };
            let mut obj = serde_json::Map::new();
            obj.insert("name".into(), serde_json::json!(tag));
            obj.insert("type".into(), serde_json::json!(clash_type));
            obj.insert("history".into(), serde_json::json!([]));
            obj.insert("udp".into(), serde_json::json!(true));
            if let Some(all) = item.get("outbounds").and_then(|v| v.as_array()) {
                let members: Vec<String> = all
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
                let now = item
                    .get("default")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
                    .or_else(|| members.first().cloned())
                    .unwrap_or_default();
                obj.insert("all".into(), serde_json::json!(members));
                obj.insert("now".into(), serde_json::json!(now));
            }
            proxies.insert(tag.to_owned(), serde_json::Value::Object(obj));
        }
    }
    let connections = vec![
        serde_json::json!({
            "id": "conn-gh-1",
            "metadata": {
                "host": "github.com",
                "network": "tcp",
                "destinationIP": "140.82.121.4",
                "destinationPort": "443",
                "process": "curl"
            },
            "chains": ["HK-01", "proxy"],
            "rule": "DOMAIN-SUFFIX",
            "rulePayload": "github.com",
            "upload": 2048,
            "download": 16384,
            "start": "2026-06-01T12:00:00+00:00"
        }),
        serde_json::json!({
            "id": "conn-dns-2",
            "metadata": {
                "host": "",
                "network": "udp",
                "destinationIP": "114.114.114.114",
                "destinationPort": "53",
                "process": "systemd-resolved"
            },
            "chains": ["direct"],
            "rule": "IP-CIDR",
            "rulePayload": "114.114.114.114/32",
            "upload": 512,
            "download": 1024,
            "start": "2026-06-01T12:00:05+00:00"
        }),
    ];
    ClashState {
        mode,
        proxies,
        connections,
    }
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3])
                && let Ok(val) = u8::from_str_radix(hex, 16)
            {
                out.push(val);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn handle_clash_request(raw: &str, state: &mut ClashState) -> (u16, String) {
    let mut lines = raw.lines();
    let first = lines.next().unwrap_or_default();
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let target = parts.next().unwrap_or("/");
    let path = target.split('?').next().unwrap_or(target);
    let body = raw.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");

    match (method, path) {
        ("GET", "/version") => (200, "{\"version\":\"fixture\"}".to_owned()),
        ("GET", "/configs") => (200, serde_json::json!({ "mode": state.mode }).to_string()),
        ("PATCH", "/configs") => {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(body)
                && let Some(m) = val.get("mode").and_then(|v| v.as_str())
            {
                state.mode = m.to_owned();
            }
            (204, String::new())
        }
        ("GET", "/proxies") => (
            200,
            serde_json::json!({ "proxies": state.proxies }).to_string(),
        ),
        ("PUT", p) if p.starts_with("/proxies/") => {
            let group = percent_decode(p.trim_start_matches("/proxies/"));
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(body)
                && let Some(name) = val.get("name").and_then(|v| v.as_str())
                && let Some(entry) = state.proxies.get_mut(&group).and_then(|v| v.as_object_mut())
            {
                entry.insert("now".into(), serde_json::json!(name));
                return (204, String::new());
            }
            (404, "{\"message\":\"not found\"}".to_owned())
        }
        ("GET", p) if p.starts_with("/proxies/") && p.ends_with("/delay") => {
            (200, "{\"delay\":42}".to_owned())
        }
        ("GET", "/connections") => {
            let upload_total: u64 = state
                .connections
                .iter()
                .filter_map(|c| c.get("upload").and_then(|v| v.as_u64()))
                .sum();
            let download_total: u64 = state
                .connections
                .iter()
                .filter_map(|c| c.get("download").and_then(|v| v.as_u64()))
                .sum();
            (
                200,
                serde_json::json!({
                    "downloadTotal": download_total,
                    "uploadTotal": upload_total,
                    "connections": state.connections
                })
                .to_string(),
            )
        }
        ("DELETE", "/connections") => {
            state.connections.clear();
            (204, String::new())
        }
        ("DELETE", p) if p.starts_with("/connections/") => {
            let id = percent_decode(p.trim_start_matches("/connections/"));
            let before = state.connections.len();
            state
                .connections
                .retain(|c| c.get("id").and_then(|v| v.as_str()) != Some(id.as_str()));
            if state.connections.len() < before {
                (204, String::new())
            } else {
                (404, "{\"message\":\"not found\"}".to_owned())
            }
        }
        _ => (200, "{\"version\":\"fixture\"}".to_owned()),
    }
}

fn spawn_listener(host: &str, port: u16, api_state: Option<ClashState>, delay_ms: u64) {
    let host = host.to_owned();
    std::thread::spawn(move || {
        if delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(delay_ms));
        }
        let mut api_state = api_state;
        let listener = TcpListener::bind((host.as_str(), port)).expect("fixture binds its listener");
        for connection in listener.incoming() {
            let Ok(mut stream) = connection else { continue };
            if let Some(state) = api_state.as_mut() {
                let mut buffer = [0_u8; 8192];
                let n = stream.read(&mut buffer).unwrap_or(0);
                let req = String::from_utf8_lossy(&buffer[..n]);
                let (status, resp_body) = handle_clash_request(&req, state);
                let status_line = match status {
                    204 => "204 No Content",
                    404 => "404 Not Found",
                    _ => "200 OK",
                };
                let resp = format!(
                    "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{resp_body}",
                    resp_body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
            // Let the probing client close first. An eager server close leaves
            // the fixture's fixed port in TIME_WAIT and makes restart tests race.
            let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
            let _ = stream.read(&mut [0_u8; 1]);
        }
    });
}
