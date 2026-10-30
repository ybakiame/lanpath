//! Subscription config composition (sing-box).

use lanpath_common::{LanpathError, LanpathResult};
use serde_json::{Value, json};

use super::MitmChain;

/// Compose subscription config into a locally usable sing-box config.
///
/// Without MITM chain, inbounds are replaced wholesale with a single mixed inbound
/// (only listening on `127.0.0.1`), and `outbounds` / `route` / `dns` / `log` etc. are preserved.
///
/// With MITM chain (`mitm_chain = Some`):
/// - inbounds are two mixed: main inlet `main-in` (`mixed_port`) + return inlet
///   `mitm-return` (`return_port`, MITM-decrypted traffic returns to core normal routing from here)
/// - outbounds append `lanpath-mitm` http outbound (pointing to MITM listen address)
/// - route.rules prepend a whitelist rule: `inbound = [main-in]`, `*.` prefix hostnames go to
///   `domain_suffix`, IP addresses to exact `ip_cidr`, and other entries to `domain`.
///   Panel injection keeps mode/local/IPv6 rules ahead of this whitelist, but its injected
///   RealIP resolve exempts this exact predicate in rule mode. Mock responses can therefore
///   complete without resolving the origin; forwarded traffic resolves on `mitm-return`.
///
/// MITM chain with an empty effective whitelist (no non-exclusion hostname) leaves the config
/// completely untouched: no route rule (a conditionless `inbound = [main-in]` rule would match
/// all traffic), no `lanpath-mitm` outbound, no `mitm-return` inbound. This is a defensive guard —
/// the normal entry `ClientState::start_mitm_chain` already skips the chain in that case.
///
/// Both paths perform sing-box 1.12+ DNS compatibility adaptation: when subscription contains
/// `dns.servers`, ensure `route.default_domain_resolver` exists (pointing to first DNS server tag,
/// auto-generated when missing), otherwise current sing-box rejects config.
pub fn compose_singbox_config(
    sub_config: &Value,
    mixed_port: u16,
    mitm_chain: Option<MitmChain>,
) -> LanpathResult<Value> {
    let mut obj = sub_config.as_object().cloned().ok_or_else(|| {
        LanpathError::Client("subscription config must be a JSON object".to_string())
    })?;

    ensure_domain_resolver(&mut obj);

    let Some(chain) = mitm_chain else {
        let mixed_inbound = json!({
            "type": "mixed",
            "tag": "mixed-in",
            "listen": "127.0.0.1",
            "listen_port": mixed_port,
        });
        obj.insert("inbounds".to_string(), Value::Array(vec![mixed_inbound]));
        return Ok(Value::Object(obj));
    };

    // Exclusions are predicates on the MITM route, not direct-outbound rules.
    // Failing this predicate leaves the original subscription routing untouched.
    let mut included = Vec::new();
    let mut excluded = Vec::new();
    for hostname in &chain.hostnames {
        if let Some(pattern) = hostname
            .strip_prefix('-')
            .or_else(|| hostname.strip_prefix('!'))
        {
            excluded.push(hostname_rule(pattern)?);
        } else {
            included.push(hostname_rule(hostname)?);
        }
    }
    if included.is_empty() {
        return Ok(Value::Object(obj));
    }

    let main_in = json!({
        "type": "mixed",
        "tag": "main-in",
        "listen": "127.0.0.1",
        "listen_port": mixed_port,
    });
    let return_in = json!({
        "type": "mixed",
        "tag": "mitm-return",
        "listen": "127.0.0.1",
        "listen_port": chain.return_port,
    });
    obj.insert(
        "inbounds".to_string(),
        Value::Array(vec![main_in, return_in]),
    );

    // outbounds append lanpath-mitm http outbound.
    let mut outbounds = obj
        .remove("outbounds")
        .and_then(|o| o.as_array().cloned())
        .unwrap_or_default();
    outbounds.push(json!({
        "type": "http",
        "tag": "lanpath-mitm",
        "server": "127.0.0.1",
        "server_port": chain.proxy_addr.port(),
    }));
    obj.insert("outbounds".to_string(), Value::Array(outbounds));

    let mut route = obj
        .remove("route")
        .and_then(|r| r.as_object().cloned())
        .unwrap_or_default();
    let mut predicates = vec![json!({ "inbound": ["main-in"] })];
    if !excluded.is_empty() {
        // Evaluate the exclusion before any broad suffix/wildcard inclusion.
        predicates.push(json!({
            "type": "logical", "mode": "or", "rules": excluded, "invert": true,
        }));
    }
    predicates.push(json!({ "type": "logical", "mode": "or", "rules": included }));
    let rule = json!({
        "type": "logical", "mode": "and", "rules": predicates, "outbound": "lanpath-mitm",
    });
    let mut rules = route
        .remove("rules")
        .and_then(|r| r.as_array().cloned())
        .unwrap_or_default();
    rules.insert(0, rule);
    route.insert("rules".to_string(), Value::Array(rules));
    obj.insert("route".to_string(), Value::Object(route));
    Ok(Value::Object(obj))
}

/// A port-qualified hostname is an AND pair; unrelated host/port pairs must not
/// be flattened into a cross product. IPv6 literals retain their whole address.
fn hostname_rule(pattern: &str) -> LanpathResult<Value> {
    let invalid = || LanpathError::Client("invalid MITM hostname or port pattern".into());
    let (host, port) = if let Some(bracketed) = pattern.strip_prefix('[') {
        let (host, rest) = bracketed.split_once(']').ok_or_else(invalid)?;
        let port = if rest.is_empty() {
            None
        } else {
            Some(rest.strip_prefix(':').ok_or_else(invalid)?)
        };
        (host, port)
    } else if pattern.parse::<std::net::IpAddr>().is_ok() {
        (pattern, None)
    } else if let Some((host, port)) = pattern.rsplit_once(':') {
        if host.contains(':') {
            return Err(invalid());
        }
        (host, Some(port))
    } else {
        (pattern, None)
    };
    if host.is_empty() {
        return Err(invalid());
    }
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty()
        || (host.parse::<std::net::IpAddr>().is_err()
            && !host
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | '*' | '?')))
    {
        return Err(invalid());
    }
    let mut rule = serde_json::Map::new();
    if let Some(suffix) = host
        .strip_prefix("*.")
        .filter(|suffix| !suffix.contains(['*', '?']))
    {
        rule.insert("domain_suffix".into(), json!([suffix]));
    } else if host.contains(['*', '?']) {
        let regex = format!(
            "^{}$",
            regex::escape(&host)
                .replace(r"\*", ".*")
                .replace(r"\?", ".")
        );
        rule.insert("domain_regex".into(), json!([regex]));
    } else if let Ok(address) = host.parse::<std::net::IpAddr>() {
        rule.insert(
            "ip_cidr".into(),
            json!([format!(
                "{address}/{}",
                if address.is_ipv4() { 32 } else { 128 }
            )]),
        );
    } else {
        rule.insert("domain".into(), json!([host]));
    }
    if let Some(port) = port.filter(|port| *port != "*") {
        let port = port.parse::<u16>().map_err(|_| invalid())?;
        if port == 0 {
            return Err(invalid());
        }
        rule.insert("port".into(), json!([port]));
    }
    Ok(Value::Object(rule))
}

/// sing-box 1.12+ DNS compatibility adaptation.
///
/// sing-box 1.12+ requires explicit domain resolver declaration: when config contains
/// `dns.servers` (and non-empty), `route.default_domain_resolver` must exist, otherwise
/// config check directly rejects (`missing route.default_domain_resolver`). Subscription templates
/// usually only declare `dns.servers`, this function takes tag from first server (auto-generates
/// `dns-<i>` when missing) and writes to `route.default_domain_resolver`.
pub(crate) fn ensure_domain_resolver(obj: &mut serde_json::Map<String, Value>) {
    let has_servers = obj
        .get("dns")
        .and_then(|d| d.as_object())
        .and_then(|d| d.get("servers"))
        .and_then(|s| s.as_array())
        .map(|arr| !arr.is_empty())
        .unwrap_or(false);
    if !has_servers {
        return;
    }
    // Skip when explicit resolver already exists.
    let has_resolver = obj
        .get("route")
        .and_then(|r| r.as_object())
        .and_then(|r| r.get("default_domain_resolver"))
        .is_some();
    if has_resolver {
        return;
    }

    let mut resolver_tag: Option<String> = None;
    if let Some(servers) = obj
        .get_mut("dns")
        .and_then(|d| d.as_object_mut())
        .and_then(|d| d.get_mut("servers"))
        .and_then(|s| s.as_array_mut())
    {
        for (i, server) in servers.iter_mut().enumerate() {
            if let Some(s) = server.as_object_mut() {
                if !s.contains_key("tag") {
                    s.insert("tag".to_string(), Value::String(format!("dns-{i}")));
                }
                if let Some(tag) = s.get("tag").and_then(|t| t.as_str()) {
                    resolver_tag = Some(tag.to_string());
                    break;
                }
            }
        }
    }

    let Some(tag) = resolver_tag else { return };
    let route = obj
        .entry("route")
        .or_insert_with(|| Value::Object(Default::default()));
    if let Some(r) = route.as_object_mut() {
        r.insert(
            "default_domain_resolver".to_string(),
            json!({ "server": tag }),
        );
    }
}
