#![cfg_attr(not(windows), allow(dead_code))]

use super::protocol::{MAX_CONFIG, MAX_RESOURCES, Resource, failure};
use crate::configuration::resources::TLS_INPUT_PATHS;
use lanpath_common::LanpathResult;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const PREFIX: &str = "lanpath-resource:";

/// Only schema-qualified locations are capabilities. Header and predefined-host
/// map keys remain opaque data even when named `command` or `certificate_path`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfigContext {
    Root,
    Dns,
    DnsServer,
    Hosts,
    Outbound,
    HttpClient,
    Certificate,
    Tls,
    Ech,
    Route,
    RuleSet,
    LocalRuleSet,
    Experimental,
    CacheFile,
    ClashApi,
    Log,
    Other,
}

impl ConfigContext {
    pub(super) fn object(self, value: &serde_json::Map<String, Value>) -> Self {
        match (self, value.get("type").and_then(Value::as_str)) {
            (Self::DnsServer, Some("hosts")) => Self::Hosts,
            (Self::RuleSet, Some("local")) => Self::LocalRuleSet,
            _ => self,
        }
    }

    pub(super) fn child(self, key: &str) -> Self {
        match (self, key) {
            (Self::Root, "dns") => Self::Dns,
            (Self::Dns, "servers") => Self::DnsServer,
            (Self::Root, "outbounds") => Self::Outbound,
            (Self::Root, "http_clients") => Self::HttpClient,
            (Self::Root, "certificate") => Self::Certificate,
            (Self::Root, "route") => Self::Route,
            (Self::Route, "rule_set") => Self::RuleSet,
            (Self::Root, "experimental") => Self::Experimental,
            (Self::Experimental, "cache_file") => Self::CacheFile,
            (Self::Experimental, "clash_api") => Self::ClashApi,
            (Self::Root, "log") => Self::Log,
            (Self::Outbound | Self::HttpClient | Self::DnsServer | Self::Hosts, "tls") => Self::Tls,
            (Self::Tls, "ech") => Self::Ech,
            (Self::DnsServer | Self::Hosts | Self::RuleSet | Self::LocalRuleSet, "http_client") => {
                Self::HttpClient
            }
            _ => Self::Other,
        }
    }

    pub(super) fn input_path(self, key: &str) -> bool {
        match self {
            Self::Tls => TLS_INPUT_PATHS.contains(&key),
            Self::Ech => key == "config_path",
            Self::Certificate => key == "certificate_path",
            Self::Hosts | Self::LocalRuleSet => key == "path",
            _ => false,
        }
    }
}

pub(super) fn resource_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    !name.is_empty()
        && name.len() <= 80
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
        && !name.ends_with('.')
        && !name.contains("..")
        && !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !(stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

/// Run only in the unelevated UI. Caller-owned file references become immutable request bytes.
pub fn snapshot_resources(root: &Path, config: &mut Value) -> LanpathResult<Vec<Resource>> {
    fn walk(
        root: &Path,
        value: &mut Value,
        context: ConfigContext,
        out: &mut Vec<Resource>,
        total: &mut usize,
        depth: usize,
    ) -> LanpathResult<()> {
        if depth > 48 {
            return Err(failure("configuration nesting limit exceeded"));
        }
        match value {
            Value::Object(map) => {
                let context = context.object(map);
                for (key, child) in map {
                    if context.input_path(key) {
                        let values = match child {
                            Value::Array(items) => items.as_mut_slice(),
                            value => std::slice::from_mut(value),
                        };
                        for value in values {
                            if out.len() >= 128 {
                                return Err(failure("resource count exceeds bound"));
                            }
                            let path = value.as_str().ok_or_else(|| {
                                failure("resource reference must be a string or array of strings")
                            })?;
                            if path.starts_with(PREFIX) {
                                return Err(failure("resource IDs cannot reference files"));
                            }
                            let path = crate::configuration::resources::confined(root, path, key)?;
                            let mut file = std::fs::File::open(path)?;
                            let metadata = file.metadata()?;
                            let remaining = MAX_RESOURCES - *total;
                            if !metadata.is_file() || metadata.len() > remaining as u64 {
                                return Err(failure("resource size/type rejected"));
                            }
                            use std::io::Read;
                            let mut bytes = Vec::new();
                            file.by_ref()
                                .take((remaining + 1) as u64)
                                .read_to_end(&mut bytes)?;
                            *total = total
                                .checked_add(bytes.len())
                                .ok_or_else(|| failure("resource size overflow"))?;
                            if *total > MAX_RESOURCES {
                                return Err(failure("combined resources exceed bound"));
                            }
                            let name = format!("resource-{}", out.len());
                            *value = Value::String(format!("{PREFIX}{name}"));
                            out.push(Resource { name, bytes });
                        }
                    } else {
                        walk(root, child, context.child(key), out, total, depth + 1)?;
                    }
                }
            }
            Value::Array(items) => {
                for child in items {
                    walk(root, child, context, out, total, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut resources = Vec::new();
    walk(root, config, ConfigContext::Root, &mut resources, &mut 0, 0)?;
    Ok(resources)
}

pub(super) fn admit(
    config: &mut Value,
    resources: &[Resource],
    session: &Path,
    interface: &str,
) -> LanpathResult<()> {
    if serde_json::to_vec(config)?.len() > MAX_CONFIG || resources.len() > 128 {
        return Err(failure("config/resource count exceeds bound"));
    }
    let mut names = BTreeMap::new();
    let mut total = 0usize;
    for resource in resources {
        total = total
            .checked_add(resource.bytes.len())
            .ok_or_else(|| failure("resource size overflow"))?;
        if total > MAX_RESOURCES
            || !resource_name(&resource.name)
            || names
                .insert(resource.name.as_str(), resource.bytes.len())
                .is_some()
        {
            return Err(failure("invalid/duplicate/oversize resource"));
        }
    }
    let root = config
        .as_object_mut()
        .ok_or_else(|| failure("core config must be an object"))?;
    if root.keys().any(|k| {
        ![
            "$schema",
            "log",
            "dns",
            "ntp",
            "certificate",
            "http_clients",
            "inbounds",
            "outbounds",
            "route",
            "experimental",
            "endpoints",
        ]
        .contains(&k.as_str())
    }) {
        return Err(failure("unsupported privileged core section"));
    }
    if root.contains_key("endpoints") {
        return Err(failure("privileged endpoint extensions are not admitted"));
    }
    let inbounds = root
        .get_mut("inbounds")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| failure("missing inbounds"))?;
    if inbounds.is_empty() || inbounds.len() > 8 {
        return Err(failure("inbound count rejected"));
    }
    let mut tun_count = 0;
    for inbound in inbounds {
        let object = inbound
            .as_object_mut()
            .ok_or_else(|| failure("invalid inbound"))?;
        match object.get("type").and_then(Value::as_str) {
            Some("tun") => {
                tun_count += 1;
                const KEYS: &[&str] = &[
                    "type",
                    "tag",
                    "interface_name",
                    "address",
                    "mtu",
                    "auto_route",
                    "strict_route",
                    "stack",
                    "sniff",
                    "sniff_override_destination",
                    "sniff_timeout",
                    "domain_strategy",
                    "route_address",
                    "route_exclude_address",
                    "udp_timeout",
                    "endpoint_independent_nat",
                ];
                if object.keys().any(|k| !KEYS.contains(&k.as_str())) {
                    return Err(failure("unsupported privileged TUN option"));
                }
                object.insert("interface_name".into(), interface.into());
                if let Some(mtu) = object.get("mtu")
                    && !(576..=9000).contains(&mtu.as_u64().unwrap_or(0))
                {
                    return Err(failure("TUN MTU rejected"));
                }
                if let Some(stack) = object.get("stack").and_then(Value::as_str)
                    && !["system", "gvisor", "mixed"].contains(&stack)
                {
                    return Err(failure("TUN stack rejected"));
                }
            }
            Some("mixed" | "direct") => {
                if object.get("listen").and_then(Value::as_str) != Some("127.0.0.1") {
                    return Err(failure("privileged listeners must bind IPv4 loopback"));
                }
                if !object
                    .get("listen_port")
                    .and_then(Value::as_u64)
                    .is_some_and(|p| (1024..=65535).contains(&p))
                {
                    return Err(failure("privileged listener port rejected"));
                }
                if object.contains_key("tls") || object.contains_key("transport") {
                    return Err(failure("privileged inbound transports are not admitted"));
                }
            }
            _ => return Err(failure("unsupported privileged inbound")),
        }
    }
    if tun_count != 1 {
        return Err(failure("exactly one managed TUN inbound is required"));
    }
    if let Some(items) = root.get("outbounds").and_then(Value::as_array) {
        if items.len() > 2048 {
            return Err(failure("too many outbounds"));
        }
        for outbound in items {
            if !outbound
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| {
                    [
                        "direct",
                        "block",
                        "dns",
                        "selector",
                        "urltest",
                        "socks",
                        "http",
                        "shadowsocks",
                        "vmess",
                        "vless",
                        "trojan",
                        "hysteria",
                        "hysteria2",
                        "tuic",
                        "ssh",
                        "wireguard",
                        "shadowtls",
                        "anytls",
                    ]
                    .contains(&kind)
                })
            {
                return Err(failure("unsupported privileged outbound implementation"));
            }
        }
    }
    let mut referenced = BTreeSet::new();
    let mut nodes = 0;
    validate_value(
        config,
        ConfigContext::Root,
        &names,
        session,
        &mut referenced,
        &mut nodes,
        0,
    )?;
    if referenced.len() != names.len() {
        return Err(failure("unreferenced resource rejected"));
    }
    if let Some(log) = config.get_mut("log").and_then(Value::as_object_mut) {
        log.insert(
            "output".into(),
            session
                .join("logs/core.log")
                .to_string_lossy()
                .into_owned()
                .into(),
        );
    }
    if let Some(cache) = config
        .pointer_mut("/experimental/cache_file")
        .and_then(Value::as_object_mut)
    {
        cache.insert(
            "path".into(),
            session
                .join("cache/cache.db")
                .to_string_lossy()
                .into_owned()
                .into(),
        );
    }
    Ok(())
}

fn validate_value(
    value: &mut Value,
    context: ConfigContext,
    names: &BTreeMap<&str, usize>,
    session: &Path,
    referenced: &mut BTreeSet<String>,
    nodes: &mut usize,
    depth: usize,
) -> LanpathResult<()> {
    *nodes += 1;
    if depth > 48 || *nodes > 100_000 {
        return Err(failure("configuration complexity limit exceeded"));
    }
    match value {
        Value::Object(map) => {
            let context = context.object(map);
            if context == ConfigContext::RuleSet {
                return Err(failure("privileged core may not download rule sets"));
            }
            if context == ConfigContext::ClashApi
                && let Some(controller) = map.get("external_controller").and_then(Value::as_str)
            {
                let address: std::net::SocketAddr = controller
                    .parse()
                    .map_err(|_| failure("invalid controller address"))?;
                if address.ip() != std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
                    || address.port() < 1024
                {
                    return Err(failure("controller must bind unprivileged loopback"));
                }
            }
            for (key, child) in map {
                // The complete shared capability schema runs independently in
                // staging after immutable resource files have been materialized.
                if context.input_path(key) {
                    let values = match child {
                        Value::Array(items) => items.as_mut_slice(),
                        value => std::slice::from_mut(value),
                    };
                    for value in values {
                        let id = value
                            .as_str()
                            .and_then(|s| s.strip_prefix(PREFIX))
                            .ok_or_else(|| {
                                failure("file reference must use a snapshotted resource ID")
                            })?;
                        if !names.contains_key(id) {
                            return Err(failure("resource reference missing"));
                        }
                        referenced.insert(id.to_owned());
                        *value = session
                            .join("resources")
                            .join(id)
                            .to_string_lossy()
                            .into_owned()
                            .into();
                    }
                } else if key == "path" && context == ConfigContext::CacheFile {
                    if child.as_str().is_some_and(|s| !s.is_empty()) {
                        return Err(failure("caller cache path rejected"));
                    }
                } else if context == ConfigContext::Log
                    && key == "output"
                    && !child
                        .as_str()
                        .is_some_and(|v| ["", "stdout", "stderr"].contains(&v))
                {
                    return Err(failure("caller log output rejected"));
                } else {
                    validate_value(
                        child,
                        context.child(key),
                        names,
                        session,
                        referenced,
                        nodes,
                        depth + 1,
                    )?;
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                validate_value(item, context, names, session, referenced, nodes, depth + 1)?;
            }
        }
        Value::String(text) if text.len() > 65_536 || text.contains('\0') => {
            return Err(failure("invalid configuration string"));
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
#[path = "config_snapshot_tests.rs"]
mod snapshot_tests;
