//! Quantumult X plaintext grammar: https://github.com/crossutility/Quantumult-X/blob/master/sample.conf
//! and https://github.com/crossutility/Quantumult-X/blob/master/rewrite.md.

use lanpath_mitm::{HookMode, Phase, RewriteKind, RewriteRule, ScriptRule};
use lanpath_script::{ScriptDialect, ScriptKind, TaskScript};

use super::surge::{cron_expression, script_options};
use super::utils::*;

pub(super) fn parse_qx_rewrite(
    cfg: &mut super::ImportedConfig,
    hook_index: &mut usize,
    dialect: ScriptDialect,
    line: &str,
) {
    if line.split_whitespace().any(|word| word == "url-and-header") {
        cfg.warn(
            "rewrite",
            line,
            "url-and-header conditional matching unsupported; not activated",
        );
        return;
    }
    let (src, rest) = take_word(line);
    let (mut action, mut args) = take_word(rest);
    if action == "url" {
        (action, args) = take_word(args);
    }
    let Some(pattern) = compile_pattern(src, cfg, "rewrite", line) else {
        return;
    };
    let kind = match action {
        "url-302" | "url-307" | "302" | "307" => {
            if args.is_empty() || args.split_whitespace().count() != 1 {
                cfg.warn(
                    "rewrite",
                    line,
                    "missing or invalid redirect target; not activated",
                );
                return;
            }
            RewriteKind::Redirect {
                status: if action.ends_with("307") { 307 } else { 302 },
                target: args.into(),
            }
        }
        "reject" | "reject-200" | "reject-img" | "reject-dict" | "reject-array" | "reject-json"
            if args.is_empty() =>
        {
            match action {
                "reject-200" => RewriteKind::reject_200(),
                "reject-img" => RewriteKind::reject_img(),
                "reject-dict" | "reject-json" => RewriteKind::reject_json("{}"),
                "reject-array" => RewriteKind::reject_json("[]"),
                _ => RewriteKind::reject(),
            }
        }
        "script-request-header"
        | "script-request-body"
        | "script-response-header"
        | "script-response-body"
        | "script-echo-response"
        | "script-analyze-echo-response" => {
            parse_qx_script_hook(cfg, hook_index, dialect, line, pattern, action, args);
            return;
        }
        "echo-response" => {
            cfg.warn(
                "rewrite",
                line,
                "echo-response file source unsupported; not activated",
            );
            return;
        }
        "jsonjq-request-body" | "jsonjq-response-body" => {
            cfg.warn("rewrite", line, "jq engine unsupported; not activated");
            return;
        }
        "request-header"
        | "response-header"
        | "request-body"
        | "response-body"
        | "url-request-header"
        | "url-response-header"
        | "url-request-body"
        | "url-response-body" => {
            let Some((src, replacement)) = regex_replacement(action, args) else {
                cfg.warn(
                    "rewrite",
                    line,
                    "missing regex or replacement; not activated",
                );
                return;
            };
            let Some(regex) = compile_pattern(src, cfg, "rewrite", line) else {
                return;
            };
            let phase = if action.contains("request") {
                Phase::Request
            } else {
                Phase::Response
            };
            if action.contains("header") {
                RewriteKind::HeaderBlockRewrite {
                    phase,
                    block_pattern: regex,
                    replacement: replacement.into(),
                }
            } else {
                RewriteKind::BodyRewrite {
                    phase,
                    body_pattern: Some(regex),
                    replacement: replacement.into(),
                }
            }
        }
        _ => {
            cfg.warn(
                "rewrite",
                line,
                "unsupported rewrite action or syntax; not activated",
            );
            return;
        }
    };
    cfg.rewrites.push(RewriteRule { pattern, kind });
}

fn parse_qx_script_hook(
    cfg: &mut super::ImportedConfig,
    hook_index: &mut usize,
    dialect: ScriptDialect,
    line: &str,
    pattern: lanpath_mitm::Pattern,
    action: &str,
    args: &str,
) {
    let (first, rest) = take_word(args);
    let (path, attached_params) = first.split_once(',').unwrap_or((first, ""));
    // The rejected source must not consume a rule slot or a corresponding URL slot.
    if !is_remote_url(path) {
        cfg.warn(
            "rewrite",
            line,
            "missing or local script source unsupported; not activated",
        );
        return;
    }
    let Some(params) = parse_kv_params(&format!(
        "{attached_params},{}",
        split_tokens_keep_quoted(rest).join(",")
    )) else {
        cfg.warn(
            "rewrite",
            line,
            "malformed or duplicate script option; not activated",
        );
        return;
    };
    let Some(options) = script_options(cfg, "rewrite", line, &params) else {
        return;
    };
    let kind = if action.contains("response") && !action.contains("echo") {
        ScriptKind::HttpResponse
    } else {
        ScriptKind::HttpRequest
    };
    let requires_body = action.ends_with("-body") || action == "script-analyze-echo-response";
    let mode = if action.contains("echo-response") {
        HookMode::Echo
    } else {
        HookMode::Modify
    };
    let max_size = match params.get("max-size") {
        None => 131072,
        Some(value) => match value.parse::<usize>() {
            Ok(value) => value,
            Err(_) => {
                cfg.warn("rewrite", line, "invalid body size; not activated");
                return;
            }
        },
    };
    let name = format!("hook-{hook_index}");
    let rule = ScriptRule {
        name: name.clone(),
        kind,
        pattern,
        requires_body,
        max_size,
        source: String::new(),
        argument: params.get("argument").cloned(),
        dialect,
        mode,
        enabled: options.enabled,
        timeout_ms: options.timeout_ms,
        binary: parse_bool(params.get("binary-body-mode")),
        full_headers: parse_bool(params.get("full-header-mode")),
        source_id: path.into(),
        module_id: None,
    };
    cfg.script_urls.push((name, path.into()));
    cfg.scripts.push(rule);
    *hook_index += 1;
}

fn take_word(value: &str) -> (&str, &str) {
    let value = value.trim_start();
    match value.find(char::is_whitespace) {
        Some(index) => (&value[..index], value[index..].trim()),
        None => (value, ""),
    }
}

fn regex_replacement<'a>(action: &str, args: &'a str) -> Option<(&'a str, &'a str)> {
    if !action.starts_with("url-") {
        // Locate the repeated keyword without normalizing regex/replacement whitespace.
        for (start, _) in args.match_indices(action) {
            let end = start + action.len();
            if start > 0
                && args[..start].ends_with(char::is_whitespace)
                && (end == args.len() || args[end..].starts_with(char::is_whitespace))
            {
                let regex = args[..start].trim_end();
                return Some((regex, args[end..].trim_start()));
            }
        }
    }
    let (regex, replacement) = take_word(args);
    (!regex.is_empty() && !replacement.is_empty()).then_some((regex, replacement))
}

pub(super) fn parse_qx_task(cfg: &mut super::ImportedConfig, dialect: ScriptDialect, line: &str) {
    let (declaration, params) = line.split_once(',').unwrap_or((line, ""));
    let mut words: Vec<_> = declaration.split_whitespace().collect();
    let Some(path) = words.pop().filter(|path| is_remote_url(path)) else {
        cfg.warn(
            "task",
            line,
            "local source or unsupported task trigger; not activated",
        );
        return;
    };
    let Some(cron_expr) = cron_expression(&words.join(" ")) else {
        cfg.warn("task", line, "unsupported cron expression; not activated");
        return;
    };
    let Some(params) = parse_kv_params(params) else {
        cfg.warn(
            "task",
            line,
            "malformed or duplicate task option; not activated",
        );
        return;
    };
    let Some(options) = script_options(cfg, "task", line, &params) else {
        return;
    };
    let name = params
        .get("tag")
        .filter(|name| !name.is_empty())
        .cloned()
        .unwrap_or_else(|| derive_name_from_url(path));
    cfg.task_scripts.push((
        TaskScript {
            name,
            cron_expr,
            source: String::new(),
            dialect,
            enabled: options.enabled,
            source_id: path.into(),
            module_id: None,
            argument: params.get("argument").cloned(),
            timeout_ms: options.timeout_ms,
        },
        path.into(),
    ));
}
