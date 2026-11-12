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
        for name in ["sing-box.pid", "mihomo.pid"] {
            let _ = std::fs::write(dir.join(name), b"bogus pid record");
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
            for name in ["sing-box.pid", "mihomo.pid"] {
                let _ = std::fs::remove_file(dir.join(name));
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
            spawn_listener(host, port, false, 0);
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
        spawn_listener(host, port, true, api_delay_ms);
    }
}

fn bind_mihomo_yaml(text: &str, skip_api: bool, api_delay_ms: u64) {
    for line in text.lines() {
        if let Some(port) = line_value(line, "mixed-port")
            .and_then(|value| value.parse::<u16>().ok())
        {
            spawn_listener("127.0.0.1", port, false, 0);
        }
        if !skip_api
            && let Some(controller) = line_value(line, "external-controller")
            && let Some((host, port)) = controller.rsplit_once(':')
        {
            let port: u16 = port.parse().expect("api port");
            spawn_listener(host, port, true, api_delay_ms);
        }
    }
}

fn spawn_listener(host: &str, port: u16, http: bool, delay_ms: u64) {
    let host = host.to_owned();
    std::thread::spawn(move || {
        if delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(delay_ms));
        }
        let listener = TcpListener::bind((host.as_str(), port)).expect("fixture binds its listener");
        for connection in listener.incoming() {
            let Ok(mut stream) = connection else { continue };
            if http {
                let mut buffer = [0_u8; 4096];
                let _ = stream.read(&mut buffer);
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 20\r\nConnection: close\r\n\r\n{\"version\":\"fixture\"}",
                );
            }
        }
    });
}
