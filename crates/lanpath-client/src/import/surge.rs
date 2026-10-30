//! Public plaintext Surge grammar. Specifications: https://manual.nssurge.com/scripting/overview.html
//! and https://manual.nssurge.com/http/{url-rewrite,header-rewrite,body-rewrite,map-local}.html.

use lanpath_mitm::{HookMode, Phase, RewriteKind, RewriteRule, ScriptRule};
use lanpath_script::{ScriptDialect, ScriptKind, TaskScript};
use std::collections::HashMap;

use super::utils::*;

pub(super) fn parse_surge_script(
    cfg: &mut super::ImportedConfig,
    dialect: ScriptDialect,
    line: &str,
) {
    let first = line.split_whitespace().next().unwrap_or_default();
    let (name, params) = if matches!(first, "http-request" | "http-response" | "cron") {
        let parts = split_tokens_keep_quoted(line);
        let kind = parts.first().map(String::as_str).unwrap_or_default();
        let value = parts
            .get(1)
            .map(|value| strip_quotes(value))
            .unwrap_or_default();
        let Some(mut params) = parse_kv_params(&parts.get(2..).unwrap_or_default().join(" "))
        else {
            cfg.warn(
                "script",
                line,
                "malformed or duplicate script option; not activated",
            );
            return;
        };
        params.insert("type".into(), kind.into());
        params.insert(
            if kind == "cron" { "cronexp" } else { "pattern" }.into(),
            value.into(),
        );
        let name = params
            .get("script-path")
            .map(|url| derive_name_from_url(url))
            .unwrap_or_default();
        (name, params)
    } else if let Some((name, rest)) = line.split_once('=') {
        let Some(params) = parse_kv_params(rest) else {
            cfg.warn(
                "script",
                line,
                "malformed or duplicate script option; not activated",
            );
            return;
        };
        (name.trim().to_string(), params)
    } else {
        cfg.warn(
            "script",
            line,
            "unsupported script declaration; not activated",
        );
        return;
    };
    if name.is_empty() {
        cfg.warn("script", line, "missing script name; not activated");
        return;
    }
    let Some(options) = script_options(cfg, "script", line, &params) else {
        return;
    };
    let Some(path) = params.get("script-path").filter(|path| is_remote_url(path)) else {
        cfg.warn(
            "script",
            line,
            "missing or local script source unsupported; not activated",
        );
        return;
    };
    match params.get("type").map(String::as_str) {
        Some("http-request" | "http-response") => {
            let Some(src) = params.get("pattern") else {
                cfg.warn("script", line, "missing URL pattern; not activated");
                return;
            };
            let Some(pattern) = compile_pattern(src, cfg, "script", line) else {
                return;
            };
            let kind = if params["type"] == "http-request" {
                ScriptKind::HttpRequest
            } else {
                ScriptKind::HttpResponse
            };
            let max_size = match params.get("max-size").map(String::as_str) {
                None => 10 * 1024 * 1024,
                Some("-1") if kind == ScriptKind::HttpRequest => 32 * 1024 * 1024,
                Some("-1") => usize::MAX,
                Some(value) => match value.parse::<usize>() {
                    Ok(size) => size,
                    Err(_) => {
                        cfg.warn("script", line, "invalid body size; not activated");
                        return;
                    }
                },
            };
            if dialect == ScriptDialect::Surge {
                if params.get("max-size").is_some_and(|value| value == "-1")
                    && kind == ScriptKind::HttpResponse
                {
                    cfg.warn("script", line, "response max-size=-1 uses Lanpath's bounded 32 MiB inspection envelope; larger responses skip the script and pass through unchanged");
                } else if max_size > lanpath_mitm::SURGE_BODY_ENVELOPE {
                    cfg.warn("script", line, "body limit exceeds the supported 32 MiB inspection envelope; not activated");
                    return;
                }
            }
            let rule = ScriptRule {
                name: name.clone(),
                kind,
                pattern,
                requires_body: parse_bool(
                    params
                        .get("requires-body")
                        .or_else(|| params.get("require-body")),
                ),
                max_size,
                source: String::new(),
                argument: params.get("argument").cloned(),
                dialect,
                mode: HookMode::Modify,
                enabled: options.enabled,
                timeout_ms: Some(options.timeout_ms.unwrap_or(5000)),
                binary: parse_bool(params.get("binary-body-mode")),
                full_headers: parse_bool(params.get("full-header-mode")),
                source_id: path.clone(),
                module_id: None,
            };
            // Reject unsupported sources before adding either side of the source/rule pair.
            cfg.script_urls.push((name, path.clone()));
            cfg.scripts.push(rule);
        }
        Some("cron") => {
            let Some(cron_expr) = params
                .get("cronexp")
                .and_then(|value| cron_expression(value))
            else {
                cfg.warn("script", line, "unsupported cron expression; not activated");
                return;
            };
            cfg.task_scripts.push((
                TaskScript {
                    name,
                    cron_expr,
                    source: String::new(),
                    dialect,
                    enabled: options.enabled,
                    source_id: path.clone(),
                    module_id: None,
                    argument: params.get("argument").cloned(),
                    timeout_ms: Some(options.timeout_ms.unwrap_or(5000)),
                },
                path.clone(),
            ));
        }
        _ => cfg.warn("script", line, "unsupported script type; not activated"),
    }
}

pub(super) struct ScriptOptions {
    pub enabled: bool,
    pub timeout_ms: Option<u64>,
}

pub(super) fn script_options(
    cfg: &mut super::ImportedConfig,
    section: &str,
    line: &str,
    params: &HashMap<String, String>,
) -> Option<ScriptOptions> {
    if params.get("engine").is_some_and(|engine| engine != "auto")
        || params
            .get("debug")
            .is_some_and(|value| parse_bool(Some(value)))
        || params
            .get("wake-system")
            .is_some_and(|value| parse_bool(Some(value)))
        || params.keys().any(|key| {
            !matches!(
                key.as_str(),
                "type"
                    | "script-path"
                    | "pattern"
                    | "cronexp"
                    | "argument"
                    | "enabled"
                    | "timeout"
                    | "requires-body"
                    | "require-body"
                    | "binary-body-mode"
                    | "full-header-mode"
                    | "max-size"
                    | "engine"
                    | "debug"
                    | "wake-system"
                    | "script-update-interval"
                    | "tag"
                    | "img-url"
            )
        })
    {
        cfg.warn(
            section,
            line,
            "unsupported script engine or option; not activated",
        );
        return None;
    }
    if params.contains_key("engine") {
        cfg.warn(
            section,
            line,
            "QuickJS host-API envelope only; vendor engine auto selection is not reproduced",
        );
    }
    if params.contains_key("script-update-interval") {
        cfg.warn(section, line, "per-script update interval is not scheduled independently; resource refresh policy applies");
    }
    for key in [
        "enabled",
        "requires-body",
        "require-body",
        "binary-body-mode",
        "full-header-mode",
        "debug",
        "wake-system",
    ] {
        if params.get(key).is_some_and(|value| {
            !matches!(value.as_str(), "true" | "false" | "1" | "0" | "yes" | "no")
        }) {
            cfg.warn(section, line, "invalid Boolean option; not activated");
            return None;
        }
    }
    let timeout_ms = if let Some(value) = params.get("timeout") {
        let Ok(seconds) = value.parse::<f64>() else {
            cfg.warn(section, line, "invalid script timeout; not activated");
            return None;
        };
        if !seconds.is_finite() || seconds <= 0.0 || seconds * 1000.0 >= u64::MAX as f64 {
            cfg.warn(section, line, "invalid script timeout; not activated");
            return None;
        }
        Some((seconds * 1000.0).ceil() as u64)
    } else {
        None
    };
    Some(ScriptOptions {
        enabled: params
            .get("enabled")
            .is_none_or(|value| parse_bool(Some(value))),
        timeout_ms,
    })
}

pub(super) fn cron_expression(value: &str) -> Option<String> {
    let fields: Vec<_> = value.split_whitespace().collect();
    let expression = match fields.len() {
        5 => format!("0 {}", fields.join(" ")),
        6 => fields.join(" "),
        _ => return None,
    };
    expression
        .parse::<cron::Schedule>()
        .ok()
        .map(|_| expression)
}

pub(super) fn parse_surge_url_rewrite(cfg: &mut super::ImportedConfig, line: &str) {
    let tokens: Vec<_> = line.split_whitespace().collect();
    if !(2..=3).contains(&tokens.len()) {
        cfg.warn(
            "url rewrite",
            line,
            "unsupported URL rewrite declaration; not activated",
        );
        return;
    }
    let Some(pattern) = compile_pattern(tokens[0], cfg, "url rewrite", line) else {
        return;
    };
    let kind = match tokens.get(2).copied().unwrap_or("header") {
        "header" => RewriteKind::UrlRewrite {
            target: tokens[1].into(),
        },
        "302" | "307" => RewriteKind::Redirect {
            status: if tokens[2] == "307" { 307 } else { 302 },
            target: tokens[1].into(),
        },
        "reject" => RewriteKind::reject(),
        _ => {
            cfg.warn(
                "url rewrite",
                line,
                "unsupported URL rewrite mode; not activated",
            );
            return;
        }
    };
    cfg.rewrites.push(RewriteRule { pattern, kind });
}

pub(super) fn parse_surge_header_rewrite(cfg: &mut super::ImportedConfig, line: &str) {
    let tokens: Vec<_> = line.split_whitespace().collect();
    let (phase, tokens) = match tokens.first().copied() {
        Some("http-request") => (Phase::Request, &tokens[1..]),
        Some("http-response") => (Phase::Response, &tokens[1..]),
        _ => (Phase::Request, tokens.as_slice()),
    };
    if tokens.len() < 3 {
        cfg.warn(
            "header rewrite",
            line,
            "incomplete header rewrite; not activated",
        );
        return;
    }
    let Some(pattern) = compile_pattern(tokens[0], cfg, "header rewrite", line) else {
        return;
    };
    let name = tokens[2].to_string();
    let kind = match tokens[1] {
        "header-add" if tokens.len() >= 4 => RewriteKind::HeaderAppend {
            phase,
            name,
            value: tokens[3..].join(" "),
        },
        "header-replace" if tokens.len() >= 4 => RewriteKind::HeaderReplace {
            phase,
            name,
            value: tokens[3..].join(" "),
        },
        "header-del" if tokens.len() == 3 => RewriteKind::HeaderRemove { phase, name },
        "header-replace-regex" if tokens.len() >= 5 => {
            let Some(value_pattern) = compile_pattern(tokens[3], cfg, "header rewrite", line)
            else {
                return;
            };
            RewriteKind::HeaderValueRewrite {
                phase,
                name,
                value_pattern,
                replacement: tokens[4..].join(" "),
            }
        }
        _ => {
            cfg.warn(
                "header rewrite",
                line,
                "unsupported or incomplete header action; not activated",
            );
            return;
        }
    };
    cfg.rewrites.push(RewriteRule { pattern, kind });
}

pub(super) fn parse_surge_body_rewrite(cfg: &mut super::ImportedConfig, line: &str) {
    let tokens = split_tokens_keep_quoted(line);
    if tokens.len() < 4 || !tokens.len().is_multiple_of(2) {
        cfg.warn(
            "body rewrite",
            line,
            "unsupported or incomplete body rewrite; not activated",
        );
        return;
    }
    let phase = match tokens[0].as_str() {
        "http-request" => Phase::Request,
        "http-response" => Phase::Response,
        _ => {
            cfg.warn(
                "body rewrite",
                line,
                "unsupported body rewrite phase or engine; not activated",
            );
            return;
        }
    };
    if tokens[2] == "jq" {
        cfg.warn("body rewrite", line, "jq engine unsupported; not activated");
        return;
    }
    let Some(pattern) = compile_pattern(&tokens[1], cfg, "body rewrite", line) else {
        return;
    };
    let mut pairs = Vec::with_capacity((tokens.len() - 2) / 2);
    for pair in tokens[2..].as_chunks::<2>().0 {
        let source = format!("(?m){}", strip_quotes(&pair[0]));
        let Some(regex) = compile_pattern(&source, cfg, "body rewrite", line) else {
            return;
        };
        pairs.push((regex, strip_quotes(&pair[1]).to_string()));
    }
    cfg.rewrites.push(RewriteRule {
        pattern,
        kind: RewriteKind::BodyRewritePairs { phase, pairs },
    });
}
