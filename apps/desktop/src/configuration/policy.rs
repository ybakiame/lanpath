//! Capability boundary based on the current sing-box configuration docs:
//! https://sing-box.sagernet.org/configuration/
//! Unknown capability-bearing fields are rejected, not dropped. The real core
//! remains responsible for types and protocol/schema compatibility.
use std::path::Path;

use lanpath_common::LanpathResult;
use serde_json::{Map, Value};

use super::resources::{denied, read_paths, write_path};
use super::{Bindings, outbound};

pub(super) fn object<'a>(
    value: &'a Value,
    location: &str,
) -> LanpathResult<&'a Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| denied(location, "expected a JSON object"))
}

pub(super) fn array<'a>(value: &'a Value, location: &str) -> LanpathResult<&'a Vec<Value>> {
    value
        .as_array()
        .ok_or_else(|| denied(location, "expected a JSON array"))
}

pub(super) fn fields(
    obj: &Map<String, Value>,
    location: &str,
    allowed: &[&str],
) -> LanpathResult<()> {
    for key in obj.keys() {
        if !allowed
            .iter()
            .any(|list| list.split_ascii_whitespace().any(|name| name == key))
        {
            return Err(denied(
                &format!("{location}.{key}"),
                "unsupported experimental capability/field; remove it explicitly",
            ));
        }
    }
    Ok(())
}

fn false_only(obj: &Map<String, Value>, key: &str, location: &str) -> LanpathResult<()> {
    if obj
        .get(key)
        .is_some_and(|value| value != &Value::Bool(false))
    {
        return Err(denied(
            &format!("{location}.{key}"),
            "system side effects are not authorized; only false or an absent field is allowed",
        ));
    }
    Ok(())
}

pub(super) fn check(root: &Path, value: &Value, bindings: Bindings) -> LanpathResult<()> {
    let obj = object(value, "$")?;
    fields(
        obj,
        "$",
        &["$schema log dns ntp certificate http_clients inbounds outbounds route experimental"],
    )?;
    if bindings.mixed_port == 0
        || bindings.api_port == 0
        || bindings.mixed_port == bindings.api_port
    {
        return Err(denied(
            "bindings",
            "managed ports must be nonzero and distinct",
        ));
    }
    let inbounds = obj
        .get("inbounds")
        .ok_or_else(|| denied("inbounds", "one managed mixed inbound is required"))?;
    let inbounds = array(inbounds, "inbounds")?;
    if inbounds.len() != 1 {
        return Err(denied(
            "inbounds",
            "exactly one managed mixed inbound is allowed; TUN and extra listeners are unauthorized",
        ));
    }
    let inbound = object(&inbounds[0], "inbounds[0]")?;
    fields(
        inbound,
        "inbounds[0]",
        &[
            "type tag listen listen_port users set_system_proxy tcp_fast_open tcp_multi_path udp_fragment udp_timeout sniff sniff_override_destination sniff_timeout domain_strategy",
        ],
    )?;
    if inbound.get("type").and_then(Value::as_str) != Some("mixed") {
        return Err(denied(
            "inbounds[0].type",
            "only the managed mixed inbound is authorized",
        ));
    }
    if inbound.get("listen").and_then(Value::as_str) != Some("127.0.0.1") {
        return Err(denied("inbounds[0].listen", "must be explicitly 127.0.0.1"));
    }
    if inbound.get("listen_port").and_then(Value::as_u64) != Some(u64::from(bindings.mixed_port)) {
        return Err(denied(
            "inbounds[0].listen_port",
            &format!("must be the managed port {}", bindings.mixed_port),
        ));
    }
    false_only(inbound, "set_system_proxy", "inbounds[0]")?;
    experimental(root, obj.get("experimental"), bindings)?;
    if let Some(value) = obj.get("log") {
        let log = object(value, "log")?;
        fields(log, "log", &["disabled level output timestamp"])?;
        if let Some(output) = log.get("output")
            && output.as_str() != Some("")
        {
            write_path(root, output, "log.output", "logs")?;
        }
    }
    if let Some(value) = obj.get("ntp") {
        let ntp = object(value, "ntp")?;
        fields(
            ntp,
            "ntp",
            &[
                "enabled server server_port interval write_to_system",
                outbound::DIAL,
            ],
        )?;
        false_only(ntp, "write_to_system", "ntp")?;
        outbound::shared(root, ntp, "ntp")?;
    }
    if let Some(value) = obj.get("certificate") {
        let certificate = object(value, "certificate")?;
        // Directory scanning is deliberately unsupported: certificate_path
        // handles explicit files without following unexamined directory entries.
        fields(
            certificate,
            "certificate",
            &["store certificate certificate_path"],
        )?;
        if let Some(paths) = certificate.get("certificate_path") {
            read_paths(root, paths, "certificate.certificate_path")?;
        }
    }
    if let Some(value) = obj.get("http_clients") {
        for (index, client) in array(value, "http_clients")?.iter().enumerate() {
            outbound::http_client(root, client, &format!("http_clients[{index}]"))?;
        }
    }
    if let Some(value) = obj.get("outbounds") {
        for (index, outbound) in array(value, "outbounds")?.iter().enumerate() {
            outbound::check(root, outbound, &format!("outbounds[{index}]"))?;
        }
    }
    if let Some(value) = obj.get("dns") {
        outbound::dns(root, value)?;
        if let Some(rules) = value.get("rules") {
            check_rules(rules, "dns.rules", true)?;
        }
    }
    if let Some(value) = obj.get("route") {
        route(root, value)?;
    }
    Ok(())
}

fn experimental(root: &Path, value: Option<&Value>, bindings: Bindings) -> LanpathResult<()> {
    let value = value.ok_or_else(|| {
        denied(
            "experimental.clash_api",
            "the managed Clash API is required",
        )
    })?;
    let exp = object(value, "experimental")?;
    fields(exp, "experimental", &["clash_api cache_file"])?;
    let clash = exp.get("clash_api").ok_or_else(|| {
        denied(
            "experimental.clash_api",
            "the managed Clash API is required",
        )
    })?;
    let clash = object(clash, "experimental.clash_api")?;
    // external_ui is rejected even without an explicit download URL: sing-box
    // can supply a default URL. No UI assets are downloaded implicitly.
    fields(
        clash,
        "experimental.clash_api",
        &[
            "external_controller secret default_mode access_control_allow_origin access_control_allow_private_network",
        ],
    )?;
    let controller = format!("127.0.0.1:{}", bindings.api_port);
    if clash.get("external_controller").and_then(Value::as_str) != Some(&controller) {
        return Err(denied(
            "experimental.clash_api.external_controller",
            &format!("must be exactly {controller}"),
        ));
    }
    if clash
        .get("secret")
        .is_some_and(|secret| secret.as_str() != Some(""))
    {
        return Err(denied(
            "experimental.clash_api.secret",
            "current observers are unauthenticated; remove the secret explicitly or use an empty string",
        ));
    }
    if let Some(value) = exp.get("cache_file") {
        let cache = object(value, "experimental.cache_file")?;
        fields(
            cache,
            "experimental.cache_file",
            &[
                "enabled path cache_id store_fakeip store_rdrc rdrc_timeout store_dns buffer_size flush_interval",
            ],
        )?;
        if cache.get("enabled") == Some(&Value::Bool(true)) && !cache.contains_key("path") {
            return Err(denied(
                "experimental.cache_file.path",
                "enabled caching requires an explicit cache/ file path; the implicit cache.db is unsupported",
            ));
        }
        if let Some(path) = cache.get("path") {
            write_path(root, path, "experimental.cache_file.path", "cache")?;
        }
    }
    Ok(())
}

fn route(root: &Path, value: &Value) -> LanpathResult<()> {
    let obj = object(value, "route")?;
    fields(
        obj,
        "route",
        &[
            "rules rule_set final auto_detect_interface default_interface find_process default_http_client default_domain_resolver default_network_strategy default_network_type default_fallback_network_type default_fallback_delay",
        ],
    )?;
    if let Some(resolver) = obj.get("default_domain_resolver") {
        outbound::resolver(resolver, "route.default_domain_resolver")?;
    }
    if let Some(rules) = obj.get("rules") {
        check_rules(rules, "route.rules", false)?;
    }
    if let Some(sets) = obj.get("rule_set") {
        for (index, set) in array(sets, "route.rule_set")?.iter().enumerate() {
            let loc = format!("route.rule_set[{index}]");
            let set = object(set, &loc)?;
            let kind = set.get("type").and_then(Value::as_str).unwrap_or("inline");
            let allowed = match kind {
                "inline" => "type tag rules",
                "local" => "type tag format path",
                "remote" => {
                    "type tag format url initial_path http_client update_interval download_detour"
                }
                _ => {
                    return Err(denied(
                        &format!("{loc}.type"),
                        "unsupported rule-set capability",
                    ));
                }
            };
            fields(set, &loc, &[allowed])?;
            // sing-box expands {tag} inside resource paths. Arrays/placeholders
            // must not bypass confinement after our check of the literal path.
            if set.get("tag").is_some_and(Value::is_array) {
                return Err(denied(
                    &format!("{loc}.tag"),
                    "multi-tag resource expansion is unsupported; use separate rule-set entries",
                ));
            }
            for name in ["path", "initial_path"] {
                if let Some(path) = set.get(name) {
                    if path
                        .as_str()
                        .is_some_and(|path| path.contains('{') || path.contains('}'))
                    {
                        return Err(denied(
                            &format!("{loc}.{name}"),
                            "resource placeholders are unsupported",
                        ));
                    }
                    read_paths(root, path, &format!("{loc}.{name}"))?;
                }
            }
            if let Some(client) = set.get("http_client") {
                outbound::http_client(root, client, &format!("{loc}.http_client"))?;
            }
            if let Some(rules) = set.get("rules") {
                check_rules(rules, &format!("{loc}.rules"), false)?;
            }
        }
    }
    Ok(())
}

fn check_rules(value: &Value, location: &str, dns: bool) -> LanpathResult<()> {
    const MATCHES: &str = "type mode rules invert inbound ip_version network auth_user protocol client domain domain_suffix domain_keyword domain_regex source_ip_cidr source_ip_is_private ip_cidr ip_is_private source_port source_port_range port port_range process_name process_path process_path_regex package_name package_name_regex user user_id clash_mode network_type network_is_expensive network_is_constrained interface_address network_interface_address default_interface_address wifi_ssid wifi_bssid rule_set rule_set_ipcidr_match_source rule_set_ip_cidr_match_source query_type outbound ip_accept_any rule_set_ip_cidr_accept_empty dns_server_address dns_search_domain";
    const ACTIONS: &str = "action outbound server strategy disable_cache disable_optimistic_cache rewrite_ttl client_subnet timeout method no_drop override_address override_port network_strategy fallback_delay udp_disable_domain_unmapping udp_connect udp_timeout tls_fragment tls_fragment_fallback_delay tls_record_fragment sniffer rcode answer ns extra";
    for (index, value) in array(value, location)?.iter().enumerate() {
        let loc = format!("{location}[{index}]");
        let rule = object(value, &loc)?;
        fields(rule, &loc, &[MATCHES, ACTIONS])?;
        if let Some(action) = rule.get("action") {
            let supported = if dns {
                matches!(
                    action.as_str(),
                    Some("route" | "route-options" | "reject" | "predefined")
                )
            } else {
                matches!(
                    action.as_str(),
                    Some("route" | "route-options" | "reject" | "hijack-dns" | "sniff" | "resolve")
                )
            };
            if !supported {
                return Err(denied(&format!("{loc}.action"), "unsupported rule action"));
            }
        }
        if let Some(nested) = rule.get("rules") {
            check_rules(nested, &format!("{loc}.rules"), dns)?;
        }
    }
    Ok(())
}
