//! Stash plaintext YAML, with remote providers and a QuickJS-only host API envelope.
//! https://stash.wiki/en/script/manage-script
//! https://stash.wiki/en/script/rewrite-requests
//! https://stash.wiki/en/http-engine/rewrite
//! https://stash.wiki/en/http-engine/mitm

use lanpath_mitm::{HookMode, Pattern, RewriteKind, RewriteRule, ScriptRule};
use lanpath_script::{ScriptDialect, ScriptKind};
use serde_yaml::{Mapping, Value};

use super::ImportedConfig;
use super::loon_current::{max_size, seconds_ms};
use super::utils::is_remote_url;

pub(super) fn parse_stash(content: &str) -> ImportedConfig {
    let mut cfg = ImportedConfig::default();
    let Ok(Value::Mapping(root)) = serde_yaml::from_str::<Value>(content) else {
        warn(&mut cfg, "invalid Stash YAML; no entries activated");
        return cfg;
    };
    let providers = get(&root, "script-providers").and_then(Value::as_mapping);
    let Some(http) = get(&root, "http") else {
        return cfg;
    };
    let Some(http) = http.as_mapping() else {
        warn(&mut cfg, "invalid http mapping; no entries activated");
        return cfg;
    };
    for key in http.keys() {
        if !matches!(key.as_str(), Some("script" | "mitm" | "url-rewrite")) {
            warn(
                &mut cfg,
                "unsupported http directive; directive not activated",
            );
        }
    }
    if let Some(value) = get(http, "mitm") {
        if let Some(entries) = value.as_sequence() {
            for entry in entries {
                let Some(host) = entry.as_str().filter(|host| valid_hostname(host)) else {
                    warn(&mut cfg, "invalid MITM hostname; entry not activated");
                    continue;
                };
                // Stash's unqualified MITM names cover TLS port 443 only.
                cfg.hostnames.push(if host.contains(':') {
                    host.to_owned()
                } else {
                    format!("{host}:443")
                });
            }
        } else {
            warn(&mut cfg, "invalid MITM sequence; directive not activated");
        }
    }
    if let Some(value) = get(http, "url-rewrite") {
        if let Some(entries) = value.as_sequence() {
            for entry in entries {
                if entry
                    .as_str()
                    .and_then(|entry| parse_url_rewrite(&mut cfg, entry))
                    .is_none()
                {
                    warn(&mut cfg, "unsupported URL rewrite; entry not activated");
                }
            }
        } else {
            warn(
                &mut cfg,
                "invalid URL rewrite sequence; directive not activated",
            );
        }
    }
    if let Some(value) = get(http, "script") {
        if let Some(entries) = value.as_sequence() {
            for entry in entries {
                if parse_script(&mut cfg, entry, providers).is_none() {
                    warn(
                        &mut cfg,
                        "unsupported script or provider; entry not activated",
                    );
                }
            }
        } else {
            warn(&mut cfg, "invalid script sequence; directive not activated");
        }
    }
    if !cfg.scripts.is_empty() {
        warn(
            &mut cfg,
            "scripts use the QuickJS Stash host-API envelope, not WebKit or JavaScriptCore; body capture remains bounded by the local transport limit",
        );
    }
    cfg
}

fn get<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a Value> {
    mapping.get(Value::String(key.to_owned()))
}

fn warn(cfg: &mut ImportedConfig, message: &str) {
    cfg.warn("stash", "", message);
}

fn valid_hostname(host: &str) -> bool {
    if host.is_empty()
        || host.contains(['/', '@', '?', '#', '\\'])
        || host.chars().any(char::is_whitespace)
    {
        return false;
    }
    let host = host.strip_prefix('-').unwrap_or(host);
    let (name, port) = host
        .rsplit_once(':')
        .map_or((host, None), |(name, port)| (name, Some(port)));
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '*' | '?'))
        && port.is_none_or(|port| port == "*" || port.parse::<u16>().is_ok_and(|port| port > 0))
}

fn boolean(mapping: &Mapping, key: &str, default: bool) -> Option<bool> {
    match get(mapping, key) {
        Some(value) => value.as_bool(),
        None => Some(default),
    }
}

fn parse_script(
    cfg: &mut ImportedConfig,
    entry: &Value,
    providers: Option<&Mapping>,
) -> Option<()> {
    let entry = entry.as_mapping()?;
    if entry.keys().any(|key| {
        !matches!(
            key.as_str(),
            Some(
                "name"
                    | "match"
                    | "type"
                    | "require-body"
                    | "timeout"
                    | "argument"
                    | "engine"
                    | "binary-mode"
                    | "max-size"
                    | "enabled"
                    | "enable"
            )
        )
    }) {
        return None;
    }
    let name = get(entry, "name")?.as_str()?;
    if name.is_empty() {
        return None;
    }
    let provider = get(providers?, name)?.as_mapping()?;
    // Auth headers, embedded cache payloads and local source precedence cannot be
    // approximated as a plain remote download. Reject before pushing either pair.
    if provider
        .keys()
        .any(|key| !matches!(key.as_str(), Some("url" | "interval")))
    {
        return None;
    }
    let url = get(provider, "url")?.as_str()?;
    if !is_remote_url(url) || reqwest::Url::parse(url).ok()?.host_str().is_none() {
        return None;
    }
    let kind = match get(entry, "type")?.as_str()? {
        "request" => ScriptKind::HttpRequest,
        "response" => ScriptKind::HttpResponse,
        _ => return None,
    };
    let pattern = Pattern::new(get(entry, "match")?.as_str()?).ok()?;
    if get(entry, "engine").is_some_and(|value| value.as_str() != Some("auto")) {
        return None;
    }
    let requires_body = boolean(entry, "require-body", false)?;
    let binary = boolean(entry, "binary-mode", false)?;
    let full_headers = false;
    if get(entry, "enable").is_some() && get(entry, "enabled").is_some() {
        return None;
    }
    let enabled = boolean(entry, "enable", true)? && boolean(entry, "enabled", true)?;
    let timeout_ms = match get(entry, "timeout") {
        Some(value) => seconds_ms(&value.as_f64()?.to_string())?,
        None => 10_000,
    };
    let max_size_value = match get(entry, "max-size") {
        Some(value) => Some(value.as_i64()?.to_string()),
        None => None,
    };
    let max_size = max_size(max_size_value.as_ref(), 131_072)?;
    let argument = match get(entry, "argument") {
        Some(value) => Some(value.as_str()?.to_owned()),
        None => None,
    };
    if let Some(interval) = get(provider, "interval") {
        interval.as_u64()?;
        warn(
            cfg,
            "provider interval is not a per-provider download schedule; resource refresh policy applies",
        );
    }
    cfg.scripts.push(ScriptRule {
        name: name.to_owned(),
        kind,
        pattern,
        requires_body,
        max_size,
        source: String::new(),
        argument,
        dialect: ScriptDialect::Stash,
        mode: HookMode::Modify,
        enabled,
        timeout_ms: Some(timeout_ms),
        binary,
        full_headers,
        source_id: url.to_owned(),
        module_id: None,
    });
    cfg.script_urls.push((name.to_owned(), url.to_owned()));
    Some(())
}

fn parse_url_rewrite(cfg: &mut ImportedConfig, line: &str) -> Option<()> {
    let tokens = line.split_whitespace().collect::<Vec<_>>();
    if tokens.len() != 3 {
        return None;
    }
    let pattern = Pattern::new(tokens[0]).ok()?;
    let kind = match tokens[2] {
        "transparent" => RewriteKind::UrlRewrite {
            target: tokens[1].to_owned(),
        },
        "302" | "307" => RewriteKind::Redirect {
            status: tokens[2].parse().ok()?,
            target: tokens[1].to_owned(),
        },
        "reject" => RewriteKind::reject(),
        "reject-200" => RewriteKind::reject_200(),
        "reject-img" => RewriteKind::reject_img(),
        "reject-dict" => RewriteKind::reject_json("{}"),
        "reject-array" => RewriteKind::reject_json("[]"),
        _ => return None,
    };
    cfg.rewrites.push(RewriteRule { pattern, kind });
    Some(())
}

#[cfg(test)]
#[path = "tests/stash.rs"]
mod tests;
