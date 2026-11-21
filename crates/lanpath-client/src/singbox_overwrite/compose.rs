//! 从 `overwrite/sing-box/*.json` 与订阅节点确定性组装 sing-box 配置对象。
//!
//! 相比 `sing-box merge` 的“标量先来赢 + 数组只追加不去重 + `mergePathResources` 越权读文件”，
//! 本模块在 Rust 内存内按切片段确定性合成：
//! - 展开 `outbounds.json` 中的 `"$nodes"` 动态占位符，并将订阅叶节点安全前置；
//! - 订阅节点若与用户自定义出站 tag 重名，自动为订阅节点追加稳定后缀避免 `duplicate tag`；
//! - 自动清理未启用的 `experimental.cache_file` 空壳并确保 `http_clients` /
//!   `route.default_domain_resolver` 与 `dns.servers` 一致。

use std::collections::HashSet;

use lanpath_common::LanpathResult;
use serde_json::{Map, Value, json};

use super::{NODES_PLACEHOLDER, SingboxOverwriteBundle};
use crate::config_slices::{DnsServer, DnsServerType};
use crate::core_config::{
    OUTBOUND_KIND_URLTEST, OUTBOUND_TAG_DIRECT, RULE_SET_HTTP_CLIENT_TAG, ensure_domain_resolver,
    ensure_rule_set_http_client,
};
use crate::profile::{
    EffectiveOverrides, apply_js_override, apply_yaml_override, compose_js_chain,
    extract_nodes_singbox,
};

/// 完整 sing-box 切片合成链：
/// `overwrite/sing-box/*.json` + 订阅动态节点（`$nodes` 展开） → Profile 覆写（JS/TS）。
pub async fn build_core_config_from_overwrite(
    sub_nodes: &[Value],
    effective: &EffectiveOverrides,
    bundle: &SingboxOverwriteBundle,
) -> LanpathResult<Value> {
    let base = compose_from_overwrite(sub_nodes, bundle)?;
    let merged = apply_yaml_override(base, &effective.remote_yaml)?;
    let merged = apply_yaml_override(merged, &effective.local_yaml)?;
    let js = compose_js_chain(&effective.remote_js, &effective.local_js);
    if js.is_empty() {
        Ok(merged)
    } else {
        apply_js_override(merged, &js).await
    }
}

/// 将订阅叶节点与 5 份 `overwrite/sing-box` 切片确定性合成为 sing-box 顶层配置对象。
pub fn compose_from_overwrite(
    sub_nodes: &[Value],
    bundle: &SingboxOverwriteBundle,
) -> LanpathResult<Value> {
    let mut root = Map::new();

    if let Some(log) = bundle.log.get("log").cloned() {
        root.insert("log".to_owned(), log);
    }
    if let Some(dns) = bundle.dns.get("dns").cloned() {
        root.insert("dns".to_owned(), dns);
    }
    if let Some(route) = bundle.route.get("route").cloned() {
        root.insert("route".to_owned(), route);
    }

    let slice_outbounds = bundle
        .outbounds
        .get("outbounds")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let merged_outbounds = compose_outbounds(sub_nodes, slice_outbounds);
    root.insert("outbounds".to_owned(), Value::Array(merged_outbounds));

    if let Some(mut exp) = bundle
        .experimental
        .get("experimental")
        .and_then(Value::as_object)
        .cloned()
    {
        // 受管 clash_api 由后续 PanelFeatures 层注入，不从切片透传。
        exp.remove("clash_api");
        if let Some(cache) = exp.get("cache_file").and_then(Value::as_object)
            && !cache
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            exp.remove("cache_file");
        } else if let Some(cache) = exp.get_mut("cache_file").and_then(Value::as_object_mut) {
            for key in ["path", "cache_id"] {
                if cache
                    .get(key)
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.trim().is_empty())
                {
                    cache.remove(key);
                }
            }
        }
        if !exp.is_empty() {
            root.insert("experimental".to_owned(), Value::Object(exp));
        }
    }

    // 若 dns.servers 已不包含默认 local，则清理悬空 default_domain_resolver 并按首服务器回填。
    let has_local_dns = root
        .get("dns")
        .and_then(|d| d.get("servers"))
        .and_then(Value::as_array)
        .is_some_and(|servers| {
            servers
                .iter()
                .any(|s| s.get("tag").and_then(Value::as_str) == Some("local"))
        });
    if !has_local_dns
        && let Some(route) = root.get_mut("route").and_then(Value::as_object_mut)
        && route
            .get("default_domain_resolver")
            .and_then(|r| r.get("server"))
            .and_then(Value::as_str)
            == Some("local")
    {
        route.remove("default_domain_resolver");
    }
    ensure_domain_resolver(&mut root);

    // 仅当规则集引用内置直连 http_client 时补齐 http_clients，并同步其 domain_resolver。
    let uses_ruleset_client = root
        .get("route")
        .and_then(|r| r.get("rule_set"))
        .and_then(Value::as_array)
        .is_some_and(|sets| {
            sets.iter().any(|s| {
                s.get("download_detour").is_none()
                    && s.get("http_client").and_then(Value::as_str)
                        == Some(RULE_SET_HTTP_CLIENT_TAG)
            })
        });
    if uses_ruleset_client {
        ensure_rule_set_http_client(&mut root);
        if !has_local_dns
            && let Some(resolver) = root
                .get("route")
                .and_then(|r| r.get("default_domain_resolver"))
                .cloned()
            && let Some(client) = root
                .get_mut("http_clients")
                .and_then(Value::as_array_mut)
                .and_then(|clients| {
                    clients.iter_mut().find(|c| {
                        c.get("tag").and_then(Value::as_str) == Some(RULE_SET_HTTP_CLIENT_TAG)
                    })
                })
        {
            client["domain_resolver"] = resolver;
        }
    }

    Ok(Value::Object(root))
}

fn compose_outbounds(sub_nodes: &[Value], slice_outbounds: Vec<Value>) -> Vec<Value> {
    let sub_wrapper = json!({ "outbounds": sub_nodes });
    let leaf_nodes = extract_nodes_singbox(&sub_wrapper);

    // 保留切片显式声明的 tag 优先权；订阅节点若与切片 tag 冲突则自动追加后缀。
    let mut used_tags: HashSet<String> = slice_outbounds
        .iter()
        .filter_map(|item| item.get("tag").and_then(Value::as_str).map(String::from))
        .collect();

    let mut resolved_nodes = Vec::with_capacity(leaf_nodes.len());
    let mut node_tags = Vec::with_capacity(leaf_nodes.len());
    for mut node in leaf_nodes {
        let Some(base_tag) = node.get("tag").and_then(Value::as_str).map(String::from) else {
            continue;
        };
        let mut tag = base_tag.clone();
        let mut suffix = 2u32;
        while !used_tags.insert(tag.clone()) {
            tag = format!("{base_tag}-{suffix}");
            suffix += 1;
        }
        node["tag"] = Value::String(tag.clone());
        node_tags.push(tag);
        resolved_nodes.push(node);
    }

    let mut out = resolved_nodes;
    for mut item in slice_outbounds {
        expand_nodes_placeholder(&mut item, &node_tags);
        out.push(item);
    }
    out
}

fn expand_nodes_placeholder(outbound: &mut Value, node_tags: &[String]) {
    let Some(obj) = outbound.as_object_mut() else {
        return;
    };
    let is_urltest = obj.get("type").and_then(Value::as_str) == Some(OUTBOUND_KIND_URLTEST);
    let Some(members) = obj.get_mut("outbounds").and_then(Value::as_array_mut) else {
        return;
    };
    if !members
        .iter()
        .any(|m| m.as_str() == Some(NODES_PLACEHOLDER))
    {
        return;
    }
    let mut expanded = Vec::with_capacity(members.len() + node_tags.len());
    let mut seen = HashSet::new();
    for entry in members.drain(..) {
        if entry.as_str() == Some(NODES_PLACEHOLDER) {
            for tag in node_tags {
                if seen.insert(tag.clone()) {
                    expanded.push(Value::String(tag.clone()));
                }
            }
        } else if let Some(tag) = entry.as_str() {
            if seen.insert(tag.to_owned()) {
                expanded.push(entry);
            }
        } else {
            expanded.push(entry);
        }
    }
    // urltest / selector 展开后若无任何成员，兜底回退 direct，保持合法 sing-box schema。
    if expanded.is_empty() && is_urltest {
        expanded.push(Value::String(OUTBOUND_TAG_DIRECT.to_owned()));
    }
    *members = expanded;
}

/// 从 `overwrite/sing-box/dns.json` 提取可探测的 DNS 上游服务器列表（供诊断页复用）。
#[must_use]
pub fn extract_dns_servers(bundle: &SingboxOverwriteBundle, _ipv6_enabled: bool) -> Vec<DnsServer> {
    let Some(servers) = bundle.dns.pointer("/dns/servers").and_then(Value::as_array) else {
        return Vec::new();
    };
    servers
        .iter()
        .filter_map(|item| {
            let obj = item.as_object()?;
            let server_type = match obj.get("type").and_then(Value::as_str)? {
                "udp" => DnsServerType::Udp,
                "tls" => DnsServerType::Tls,
                "https" => DnsServerType::Https,
                "h3" => DnsServerType::H3,
                "quic" => DnsServerType::Quic,
                "local" => DnsServerType::Local,
                "fakeip" => DnsServerType::Fakeip,
                _ => return None,
            };
            let tag = obj
                .get("tag")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let server = obj
                .get("server")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let server_port = obj
                .get("server_port")
                .and_then(Value::as_u64)
                .and_then(|p| u16::try_from(p).ok());
            let detour = obj
                .get("detour")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            Some(DnsServer {
                tag,
                server_type,
                server,
                server_port,
                detour,
                ..Default::default()
            })
        })
        .collect()
}
