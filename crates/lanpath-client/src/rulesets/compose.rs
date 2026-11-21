//! 规则集分组展开、引用扫描与 sing-box `route.rule_set` 注入。

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde_json::{Value, json};

use super::{GROUP_REF_PREFIX, RuleSetEntry, RuleSetGroup, RuleSetStore};

/// 将 `route.rules` 与 `dns.rules` 中的 `@<group_tag>` 规则集分组引用展开为具体成员 tag。
///
/// - 保持原有顺序并自动去重；
/// - 未找到分组或分组无成员时记录告警；若某条规则仅引用了失效/空分组，则移除该悬空规则避免核心校验失败。
pub fn expand_rule_set_groups(config: &mut Value, groups: &[RuleSetGroup]) -> Vec<String> {
    let group_map: HashMap<String, &RuleSetGroup> =
        groups.iter().map(|g| (g.tag.to_lowercase(), g)).collect();
    let mut warnings = Vec::new();

    for pointer in ["/route/rules", "/dns/rules"] {
        let Some(rules) = config.pointer_mut(pointer).and_then(Value::as_array_mut) else {
            continue;
        };
        let mut kept = Vec::with_capacity(rules.len());
        for mut rule in std::mem::take(rules) {
            let Some(raw) = rule.get("rule_set").cloned() else {
                kept.push(rule);
                continue;
            };
            let inputs = extract_rule_set_list(&raw);
            if !inputs.iter().any(|t| t.starts_with(GROUP_REF_PREFIX)) {
                kept.push(rule);
                continue;
            }
            let mut expanded = Vec::new();
            let mut seen = HashSet::new();
            for token in inputs {
                if let Some(group_tag) = token.strip_prefix(GROUP_REF_PREFIX) {
                    let key = group_tag.trim().to_lowercase();
                    match group_map.get(&key) {
                        Some(group) if !group.member_tags.is_empty() => {
                            for member in &group.member_tags {
                                let m = member.trim();
                                if !m.is_empty() && seen.insert(m.to_owned()) {
                                    expanded.push(m.to_owned());
                                }
                            }
                        }
                        Some(_) => {
                            warnings.push(format!("规则集分组 `{token}` 不含任何成员，已跳过"));
                        }
                        None => {
                            warnings.push(format!("引用的规则集分组 `{token}` 不存在，已跳过"));
                        }
                    }
                } else if !token.is_empty() && seen.insert(token.clone()) {
                    expanded.push(token);
                }
            }
            if expanded.is_empty() {
                continue;
            }
            if let Some(obj) = rule.as_object_mut() {
                let value = match expanded.as_slice() {
                    [only] => Value::String(only.clone()),
                    _ => Value::Array(expanded.into_iter().map(Value::String).collect()),
                };
                obj.insert("rule_set".to_owned(), value);
            }
            kept.push(rule);
        }
        *rules = kept;
    }

    warnings
}

/// 收集已合成 sing-box 配置中（`route.rules` 与 `dns.rules`）引用的全部 `rule_set` tag。
#[must_use]
pub fn collect_referenced_rule_set_tags(config: &Value) -> HashSet<String> {
    let mut tags = HashSet::new();
    for pointer in ["/route/rules", "/dns/rules"] {
        if let Some(rules) = config.pointer(pointer).and_then(Value::as_array) {
            for rule in rules {
                if let Some(value) = rule.get("rule_set") {
                    for tag in extract_rule_set_list(value) {
                        if !tag.starts_with(GROUP_REF_PREFIX) {
                            tags.insert(tag);
                        }
                    }
                }
            }
        }
    }
    tags
}

fn extract_rule_set_list(value: &Value) -> Vec<String> {
    match value {
        Value::String(tag) if !tag.trim().is_empty() => vec![tag.trim().to_owned()],
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

/// 将被配置引用的受管规则集以 `type: local`（相对 `<data_root>` 路径）注入 `route.rule_set`。
///
/// - 同 tag 已存在时替换为本地受管条目（避免重复 tag 导致 `sing-box check` 失败）；
/// - 仅注入本地文件已存在且非空的规则集；
/// - 返回被引用但本地文件缺失的受管规则集 tag 列表，供调用方告警。
pub fn apply_managed_rule_sets(
    data_dir: &Path,
    config: &mut Value,
    entries: &[RuleSetEntry],
) -> Vec<String> {
    let referenced = collect_referenced_rule_set_tags(config);
    if referenced.is_empty() || entries.is_empty() {
        return Vec::new();
    }

    let store = RuleSetStore::new(data_dir.to_path_buf());
    let mut injected = Vec::new();
    let mut missing = Vec::new();

    for entry in entries {
        if !referenced.contains(entry.tag.as_str()) {
            continue;
        }
        if !store.has_backing_file(entry) {
            missing.push(entry.tag.clone());
            continue;
        }
        let format = entry.file_format();
        injected.push(json!({
            "type": "local",
            "tag": entry.tag,
            "format": format.as_singbox_str(),
            "path": RuleSetStore::backing_file_rel_path(&entry.id, format),
        }));
    }

    if injected.is_empty() {
        return missing;
    }

    let Some(obj) = config.as_object_mut() else {
        return missing;
    };
    let route = obj
        .entry("route")
        .or_insert_with(|| Value::Object(Default::default()));
    let Some(route_obj) = route.as_object_mut() else {
        return missing;
    };
    let rule_set = route_obj
        .entry("rule_set")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(arr) = rule_set.as_array_mut() else {
        return missing;
    };

    let injected_tags: HashSet<String> = injected
        .iter()
        .filter_map(|item| item.get("tag").and_then(Value::as_str).map(str::to_owned))
        .collect();
    arr.retain(|item| {
        item.get("tag")
            .and_then(Value::as_str)
            .is_none_or(|tag| !injected_tags.contains(tag))
    });
    arr.extend(injected);

    missing
}
