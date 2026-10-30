//! Loon 3.5.1 (983+) simple URL-guard grammar.
//! Contract: https://nsloon.app/en/docs/Script/script_v2/
//! Compound conditions and plugin objects are deliberately not activated.

use std::collections::HashMap;

use lanpath_mitm::{HookMode, Pattern, ScriptRule};
use lanpath_script::{ScriptDialect, ScriptKind, TaskScript};

use super::ImportedConfig;
use super::utils::{derive_name_from_url, is_remote_url};

type Options = HashMap<String, String>;

pub(super) fn parse_loon_current(cfg: &mut ImportedConfig, line: &str) {
    if parse_entry(cfg, line).is_none() {
        cfg.warn(
            "script",
            "",
            "unsupported or invalid Loon entry; not activated",
        );
    }
}

fn parse_entry(cfg: &mut ImportedConfig, line: &str) -> Option<()> {
    let (trigger, rest) = line.split_once(char::is_whitespace)?;
    let rest = rest.trim_start();
    let (kind, pattern, cron, rest) = match trigger {
        "request" | "response" => {
            let condition = rest.strip_prefix("if ")?.trim_start();
            let regex_src = condition
                .strip_prefix("${url}")?
                .trim_start()
                .strip_prefix("~=")?
                .trim_start();
            let (pattern, rest) = regex_literal(regex_src)?;
            let rest = rest.trim_start().strip_prefix("then ")?;
            let kind = if trigger == "request" {
                ScriptKind::HttpRequest
            } else {
                ScriptKind::HttpResponse
            };
            (kind, Some(pattern), None, rest)
        }
        "cron" => {
            let (cron, rest) = string_literal(rest)?;
            (
                ScriptKind::Cron,
                None,
                Some(cron),
                rest.trim_start().strip_prefix("then ")?,
            )
        }
        _ => return None,
    };
    let rest = rest.trim_start().strip_prefix("script(")?.trim_start();
    let (url, rest) = string_literal(rest)?;
    if url.is_empty() || url.contains("${") {
        return None;
    }
    let rest = rest.trim_start();
    let (argument, rest) = if let Some(rest) = rest.strip_prefix(',') {
        let (argument, rest) = string_literal(rest.trim_start())?;
        (Some(argument), rest.trim_start())
    } else {
        (None, rest)
    };
    let rest = rest.strip_prefix(')')?.trim();
    let options = if rest.is_empty() {
        Options::new()
    } else {
        parse_options(rest.strip_prefix("with ")?, false)?
    };
    if options.keys().any(|key| {
        !matches!(
            key.as_str(),
            "tag"
                | "img_url"
                | "enable"
                | "timeout"
                | "debug"
                | "requires_body"
                | "binary_body_mode"
        )
    }) {
        return None;
    }
    emit(cfg, kind, pattern, cron, url, argument, options, 20_000)
}

/// Consume exactly one regex literal, leaving all condition suffixes unconsumed.
fn regex_literal(input: &str) -> Option<(Pattern, &str)> {
    let body = input.strip_prefix('/')?;
    let mut escaped = false;
    let mut in_class = false;
    let mut end = None;
    for (index, ch) in body.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '[' => in_class = true,
            ']' => in_class = false,
            '/' if !in_class => {
                end = Some(index);
                break;
            }
            _ => {}
        }
    }
    let end = end?;
    let rest = &body[end + 1..];
    let flags_end = rest
        .find(|ch: char| !ch.is_ascii_alphabetic())
        .unwrap_or(rest.len());
    let flags = &rest[..flags_end];
    let mut seen = String::new();
    for flag in flags.chars() {
        if !matches!(flag, 'i' | 'm' | 's') || seen.contains(flag) {
            return None;
        }
        seen.push(flag);
    }
    let source = body[..end].replace("\\/", "/");
    let source = if flags.is_empty() {
        source
    } else {
        format!("(?{flags}){source}")
    };
    let pattern = Pattern::new(&source).ok()?;
    Some((pattern, &rest[flags_end..]))
}

/// JSON-escaped quoted strings or Loon's literal backtick raw strings.
pub(super) fn string_literal(input: &str) -> Option<(String, &str)> {
    let quote = input.chars().next()?;
    if !matches!(quote, '"' | '`') {
        return None;
    }
    let mut escaped = false;
    for (index, ch) in input[1..].char_indices() {
        if quote == '"' && escaped {
            escaped = false;
            continue;
        }
        if quote == '"' && ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            let end = index + 2;
            let value = if quote == '`' {
                input[1..end - 1].to_owned()
            } else {
                serde_json::from_str::<String>(&input[..end]).ok()?
            };
            // Ordinary strings may reference plugin parameters. No unbound substitution.
            if quote == '"' && value.contains("${") {
                return None;
            }
            return Some((value, &input[end..]));
        }
    }
    None
}

/// Strict fields: malformed/duplicate/unknown fields must never silently change scope.
pub(super) fn parse_options(input: &str, legacy: bool) -> Option<Options> {
    if legacy {
        let mut options = Options::new();
        for segment in super::utils::split_kv_segments(input) {
            let (key, raw) = segment.trim().split_once('=')?;
            let key = match key.trim() {
                "require-body" | "requires-body" => "requires_body",
                "binary-body-mode" => "binary_body_mode",
                "max-size" => "max_size",
                "full-headers" | "full-header-mode" => "full_headers",
                "img-url" => "img_url",
                key => key,
            };
            if key.is_empty() || key.chars().any(char::is_whitespace) {
                return None;
            }
            let raw = raw.trim();
            let value = if raw.starts_with(['"', '`']) {
                let (value, rest) = string_literal(raw)?;
                if !rest.trim().is_empty() {
                    return None;
                }
                value
            } else {
                raw.to_owned()
            };
            if options.insert(key.to_owned(), value).is_some() {
                return None;
            }
        }
        return Some(options);
    }
    let mut options = Options::new();
    let mut rest = input.trim();
    while !rest.is_empty() {
        let (key, tail) = rest.split_once('=')?;
        let key = key.trim();
        if key.is_empty() || key.chars().any(char::is_whitespace) {
            return None;
        }
        let key = key.to_owned();
        let tail = tail.trim_start();
        let (value, remaining) = if tail.starts_with(['"', '`']) {
            string_literal(tail)?
        } else {
            let end = tail.find(',').unwrap_or(tail.len());
            let value = tail[..end].trim();
            if value.is_empty() || value.contains(['{', '}', '`', '"']) {
                return None;
            }
            (value.to_owned(), &tail[end..])
        };
        if options.insert(key, value).is_some() {
            return None;
        }
        let remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }
        rest = remaining.strip_prefix(',')?.trim_start();
        if rest.is_empty() {
            return None;
        }
    }
    Some(options)
}

pub(super) fn bool_option(options: &Options, key: &str, default: bool) -> Option<bool> {
    match options.get(key).map(String::as_str) {
        None => Some(default),
        Some("true" | "1") => Some(true),
        Some("false" | "0") => Some(false),
        _ => None,
    }
}

pub(super) fn seconds_ms(value: &str) -> Option<u64> {
    let seconds = value.parse::<f64>().ok()?;
    let milliseconds = seconds * 1000.0;
    if !seconds.is_finite() || milliseconds < 1.0 || milliseconds >= u64::MAX as f64 {
        return None;
    }
    Some(milliseconds.ceil() as u64)
}

pub(super) fn max_size(value: Option<&String>, default: usize) -> Option<usize> {
    match value.map(String::as_str) {
        None => Some(default),
        Some("-1" | "0") => Some(usize::MAX),
        Some(value) => value.parse().ok(),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit(
    cfg: &mut ImportedConfig,
    kind: ScriptKind,
    pattern: Option<Pattern>,
    cron: Option<String>,
    url: String,
    argument: Option<String>,
    options: Options,
    default_http_timeout: u64,
) -> Option<()> {
    if !is_remote_url(&url) || reqwest::Url::parse(&url).ok()?.host_str().is_none() {
        return None;
    }
    if options.keys().any(|key| {
        !matches!(
            key.as_str(),
            "tag"
                | "img_url"
                | "enable"
                | "timeout"
                | "debug"
                | "requires_body"
                | "binary_body_mode"
                | "max_size"
                | "full_headers"
                | "engine"
        )
    }) {
        return None;
    }
    if options.get("engine").is_some_and(|engine| engine != "auto") {
        return None;
    }
    let http = matches!(kind, ScriptKind::HttpRequest | ScriptKind::HttpResponse);
    if !http
        && [
            "requires_body",
            "binary_body_mode",
            "max_size",
            "full_headers",
        ]
        .iter()
        .any(|key| options.contains_key(*key))
    {
        return None;
    }
    let enabled = bool_option(&options, "enable", true)?;
    let requires_body = bool_option(&options, "requires_body", false)?;
    let binary = bool_option(&options, "binary_body_mode", false)?;
    let full_headers = bool_option(&options, "full_headers", false)?;
    let _debug = bool_option(&options, "debug", false)?;
    let timeout_ms = match options.get("timeout") {
        Some(value) => seconds_ms(value)?,
        None => {
            if http {
                default_http_timeout
            } else {
                300_000
            }
        }
    };
    let max_size = max_size(options.get("max_size"), 131_072)?;
    let name = options
        .get("tag")
        .filter(|name| !name.is_empty())
        .cloned()
        .unwrap_or_else(|| derive_name_from_url(&url));
    if http {
        let pattern = pattern?;
        cfg.scripts.push(ScriptRule {
            name: name.clone(),
            kind,
            pattern,
            requires_body,
            max_size,
            source: String::new(),
            argument,
            dialect: ScriptDialect::Loon,
            mode: HookMode::Modify,
            enabled,
            timeout_ms: Some(timeout_ms),
            binary,
            full_headers,
            source_id: url.clone(),
            module_id: None,
        });
        cfg.script_urls.push((name, url));
    } else {
        let fields = cron?
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let cron_expr = match fields.len() {
            5 => format!("0 {}", fields.join(" ")),
            6 => fields.join(" "),
            _ => return None,
        };
        // Use the scheduler's parser rather than activating malformed schedules.
        if cron_expr.parse::<cron::Schedule>().is_err() {
            return None;
        }
        cfg.task_scripts.push((
            TaskScript {
                name,
                cron_expr,
                source: String::new(),
                dialect: ScriptDialect::Loon,
                enabled,
                source_id: url.clone(),
                module_id: None,
                argument,
                timeout_ms: Some(timeout_ms),
            },
            url,
        ));
    }
    if options.contains_key("engine") {
        cfg.warn(
            "script",
            "",
            "QuickJS host-API envelope only; vendor engine auto selection is not reproduced",
        );
    }
    if options.contains_key("debug") || options.contains_key("img_url") {
        cfg.warn("script", "", "display/debug metadata has no runtime effect");
    }
    Ok::<(), ()>(()).ok()
}

#[cfg(test)]
#[path = "tests/loon_current.rs"]
mod tests;
