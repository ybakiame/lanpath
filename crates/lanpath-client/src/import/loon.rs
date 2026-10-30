//! Loon dialect parsing (`[Script]`, `[Rewrite]`, `[Argument]`).

use lanpath_mitm::{Phase, RewriteKind};
use lanpath_script::ScriptKind;

use super::loon_current::{emit, parse_loon_current, parse_options, string_literal};

use super::utils::*;
use super::{ArgKind, ArgSpec, ConfigMeta};

/// Legacy contract: https://nsloon.app/en/docs/Script/
/// Current entries are parsed separately; never discard an unsupported condition.
pub(super) fn parse_loon_script(cfg: &mut super::ImportedConfig, line: &str) {
    let first = line.split_whitespace().next().unwrap_or_default();
    if matches!(first, "request" | "response")
        || (first == "cron" && line.contains(" then script("))
    {
        parse_loon_current(cfg, line);
        return;
    }
    if parse_legacy_script(cfg, line).is_none() {
        cfg.warn(
            "script",
            "",
            "unsupported or invalid legacy Loon entry; not activated",
        );
    }
}

fn parse_legacy_script(cfg: &mut super::ImportedConfig, line: &str) -> Option<()> {
    let (trigger, rest) = line.split_once(char::is_whitespace)?;
    let rest = rest.trim_start();
    let (kind, pattern, cron, params) = match trigger {
        "http-request" | "http-response" => {
            let end = rest.find(char::is_whitespace)?;
            let token = &rest[..end];
            // Preserve the existing comma-attached option form without splitting regex
            // quantifier commas (e.g. {1,2}).
            let inline = token
                .find(",requires-body=")
                .or_else(|| token.find(",require-body="));
            let (pattern, params) = if let Some(index) = inline {
                (
                    &token[..index],
                    format!("{},{}", &token[index + 1..], rest[end..].trim()),
                )
            } else {
                (token, rest[end..].trim().to_owned())
            };
            let pattern =
                lanpath_mitm::Pattern::new(&format!("(?i){}", pattern.replace("\\/", "/"))).ok()?;
            let kind = if trigger == "http-request" {
                ScriptKind::HttpRequest
            } else {
                ScriptKind::HttpResponse
            };
            (kind, Some(pattern), None, params)
        }
        "cron" => {
            let (cron, rest) = string_literal(rest)?;
            (ScriptKind::Cron, None, Some(cron), rest.trim().to_owned())
        }
        _ => return None,
    };
    let mut options = parse_options(&params, true)?;
    let url = options.remove("script-path")?;
    // Loon passes String arguments unchanged; percent decoding alters user data.
    let argument = options.remove("argument");
    emit(cfg, kind, pattern, cron, url, argument, options, 10_000)
}

/// Loon `[Argument]` section line parsing: `Key = input/select,"default","opt2",...,tag=...,desc=...`.
///
/// `input` followed by a single quoted default value; `select` followed by first quoted value as
/// default, rest as options; `tag=` as separate field, `desc=` goes to description. Results merged
/// into [`ConfigMeta::arguments`] by key (when `#!arguments=` already declared,补齐 fields).
pub(super) fn parse_loon_argument(cfg: &mut super::ImportedConfig, line: &str) {
    let Some(eq) = line.find('=') else {
        cfg.warn(
            "argument",
            line,
            "unrecognized line (expected 'Key = kind,...')",
        );
        return;
    };
    let key = line[..eq].trim();
    if key.is_empty() {
        cfg.warn("argument", line, "empty argument key");
        return;
    }
    let segments = split_kv_segments(line[eq + 1..].trim());
    let Some(kind_src) = segments.first() else {
        cfg.warn("argument", line, "missing argument kind");
        return;
    };
    let kind = match kind_src.trim().to_ascii_lowercase().as_str() {
        "input" => ArgKind::Input,
        "select" => ArgKind::Select,
        _ => {
            cfg.warn(
                "argument",
                "",
                "unsupported argument kind; declaration not imported",
            );
            return;
        }
    };
    // Remaining segments: quoted values (default / options) and tag= / desc= parameters.
    let mut values: Vec<String> = Vec::new();
    let mut tag: Option<String> = None;
    let mut desc: Option<String> = None;
    for seg in &segments[1..] {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        // Quoted values checked before key=value, to avoid `=` inside values (e.g., `"a=b"`) being misjudged.
        if seg.starts_with('"') {
            values.push(strip_quotes(seg).to_string());
            continue;
        }
        if let Some((k, v)) = seg.split_once('=') {
            match k.trim().to_ascii_lowercase().as_str() {
                "tag" => tag = Some(strip_quotes(v.trim()).to_string()),
                "desc" => desc = Some(strip_quotes(v.trim()).to_string()),
                _ => {} // other key=value (e.g., enable) ignored
            }
        }
    }
    let (default_value, options) = match kind {
        ArgKind::Select => {
            let mut it = values.into_iter();
            (it.next().unwrap_or_default(), it.collect())
        }
        ArgKind::Input => (values.into_iter().next().unwrap_or_default(), Vec::new()),
    };
    merge_argument_spec(
        &mut cfg.meta,
        ArgSpec {
            key: key.to_string(),
            default_value,
            description: desc,
            kind,
            options,
            tag,
        },
    );
}

/// Merge Loon `[Argument]` section parsed parameter declaration into [`ConfigMeta`] by key.
///
/// When `#!arguments=` (or `#!arguments-desc=`) already declared the same key,补齐 new fields;
/// otherwise append the whole spec.
pub(super) fn merge_argument_spec(meta: &mut ConfigMeta, spec: ArgSpec) {
    if let Some(existing) = meta.arguments.iter_mut().find(|a| a.key == spec.key) {
        existing.kind = spec.kind;
        if !spec.default_value.is_empty() {
            existing.default_value = spec.default_value;
        }
        if spec.description.is_some() {
            existing.description = spec.description;
        }
        if !spec.options.is_empty() {
            existing.options = spec.options;
        }
        if spec.tag.is_some() {
            existing.tag = spec.tag;
        }
    } else {
        meta.arguments.push(spec);
    }
}

/// Loon `[Rewrite]` section line parsing:
/// `pattern reject[-200|-img|-dict|-array]` / `pattern 302|307 target` /
/// `pattern header-del|header-replace|header-add Name [value]` /
/// `pattern header-replace-regex Name <regex> <repl>`。
///
/// Loon `[Rewrite]` 的 header 操作作用于请求阶段。
pub(super) fn parse_loon_rewrite(cfg: &mut super::ImportedConfig, line: &str) {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 2 {
        cfg.warn("rewrite", line, "unrecognized line");
        return;
    }
    let Some(pattern) = compile_pattern(tokens[0], cfg, "rewrite", line) else {
        return;
    };
    let kind = match tokens[1] {
        "reject" => RewriteKind::reject(),
        "reject-200" => RewriteKind::reject_200(),
        "reject-img" => RewriteKind::reject_img(),
        "reject-dict" | "reject-json" => RewriteKind::reject_json("{}"),
        "reject-array" => RewriteKind::reject_json("[]"),
        mode @ ("302" | "307") => {
            let Some(target) = tokens.get(2) else {
                cfg.warn("rewrite", line, "missing redirect target");
                return;
            };
            RewriteKind::Redirect {
                status: if mode == "307" { 307 } else { 302 },
                target: (*target).to_string(),
            }
        }
        "header-del" => {
            let Some(name) = tokens.get(2) else {
                cfg.warn("rewrite", line, "header-del missing header name");
                return;
            };
            RewriteKind::HeaderRemove {
                phase: Phase::Request,
                name: (*name).to_string(),
            }
        }
        "header-replace" | "header-add" => {
            let Some(name) = tokens.get(2) else {
                cfg.warn("rewrite", line, "missing header name");
                return;
            };
            if tokens[1] == "header-add" {
                RewriteKind::HeaderAppend {
                    phase: Phase::Request,
                    name: (*name).to_string(),
                    value: tokens[3..].join(" "),
                }
            } else {
                RewriteKind::HeaderReplace {
                    phase: Phase::Request,
                    name: (*name).to_string(),
                    value: tokens[3..].join(" "),
                }
            }
        }
        "header-replace-regex" => {
            if tokens.len() < 5 {
                cfg.warn(
                    "rewrite",
                    line,
                    "header-replace-regex missing name/regex/replacement",
                );
                return;
            }
            let Some(value_pattern) = compile_pattern(tokens[3], cfg, "rewrite", line) else {
                return;
            };
            RewriteKind::HeaderValueRewrite {
                phase: Phase::Request,
                name: tokens[2].to_string(),
                value_pattern,
                replacement: tokens[4..].join(" "),
            }
        }
        _ => {
            cfg.warn("rewrite", "", "unsupported rewrite action; not activated");
            return;
        }
    };
    cfg.rewrites
        .push(lanpath_mitm::RewriteRule { pattern, kind });
}
