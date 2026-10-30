//! Parameter expansion is data-only: an expanded declaration never re-enters the config parser.

use std::collections::HashMap;

pub(crate) const TEMPLATE_WARNING: &str =
    "[mitm] parameterized hostname declaration is unresolved or unsafe; not activated";

/// Resolve only a hostname key/value pair, not arbitrary configuration directives.
/// Unknown parameters (even when supplied by a caller) must have a declaration.
pub(crate) fn resolve_mitm_templates(
    templates: &[String],
    user_values: &HashMap<String, String>,
    defaults: &HashMap<String, String>,
) -> (Vec<String>, Vec<String>) {
    let mut hostnames = Vec::new();
    let mut warnings = Vec::new();
    for template in templates {
        let Some(entries) = resolve_declaration(template, user_values, defaults) else {
            warnings.push(TEMPLATE_WARNING.to_string());
            continue;
        };
        for hostname in entries {
            if !hostnames.contains(&hostname) {
                hostnames.push(hostname);
            }
        }
    }
    (hostnames, warnings)
}

fn expand(
    template: &str,
    user_values: &HashMap<String, String>,
    defaults: &HashMap<String, String>,
) -> Option<String> {
    let mut output = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        output.push_str(&rest[..start]);
        rest = &rest[start..];
        let (open, close) = if rest.starts_with("{{{") {
            (3, "}}}")
        } else {
            (1, "}")
        };
        let end = rest[open..].find(close)? + open;
        let key = &rest[open..end];
        // Requiring a declared key prevents stale or arbitrary user values from
        // turning unknown placeholders into an activation capability.
        let default = defaults.get(key)?;
        let value = user_values.get(key).unwrap_or(default);
        if value.chars().any(char::is_control) || value.contains(['{', '}']) {
            return None;
        }
        output.push_str(value);
        rest = &rest[end + close.len()..];
    }
    output.push_str(rest);
    if output.chars().any(|ch| ch.is_control() && ch != '\t') || output.contains(['{', '}']) {
        return None;
    }
    Some(output)
}

fn resolve_declaration(
    template: &str,
    user_values: &HashMap<String, String>,
    defaults: &HashMap<String, String>,
) -> Option<Vec<String>> {
    // Split BEFORE expansion: injected '=' or section text cannot change grammar.
    let (key, value) = template.split_once('=')?;
    let key = expand(key.trim(), user_values, defaults)?;
    let key = key.trim();
    if key.is_empty() || key.starts_with('#') || key.starts_with(';') || key.starts_with("//") {
        return Some(Vec::new());
    }
    if !key.eq_ignore_ascii_case("hostname") {
        return None;
    }
    let value = expand(value.trim(), user_values, defaults)?;
    let value = value.trim();
    let value = value.strip_prefix("%APPEND%").unwrap_or(value).trim();
    let mut hostnames = Vec::new();
    for entry in value.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let (excluded, hostname) = match entry.strip_prefix(['-', '!']) {
            Some(hostname) => (true, hostname),
            None => (false, entry),
        };
        // Hostname grammar, never config grammar. Whitespace, URLs, security
        // directives, scripts and sections cannot survive this gate.
        if !valid_hostname(hostname) {
            return None;
        }
        hostnames.push(if excluded {
            format!("-{hostname}")
        } else {
            hostname.to_string()
        });
    }
    Some(hostnames)
}

fn valid_hostname(hostname: &str) -> bool {
    if hostname.is_empty()
        || !hostname.bytes().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(ch, b'.' | b'-' | b'_' | b'*' | b':' | b'[' | b']')
        })
    {
        return false;
    }
    if !hostname.contains(['[', ']']) {
        return true;
    }
    // Brackets identify IPv6 authorities, never a section name such as [Script].
    let Some((address, suffix)) = hostname
        .strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
    else {
        return false;
    };
    address.parse::<std::net::Ipv6Addr>().is_ok()
        && (suffix.is_empty()
            || suffix
                .strip_prefix(':')
                .is_some_and(|port| port == "*" || port.parse::<u16>().is_ok()))
}
