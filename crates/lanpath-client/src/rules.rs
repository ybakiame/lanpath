//! 自定义规则工作台存储（`<data_root>/rules.toml`）与 sing-box 路由规则渲染。
//!
//! `rules.toml` 仅保存用户自定义的快速/批量分流规则条目，不包含最终合成配置或
//! 覆写切片（sing-box 分段切片位于 `overwrite/sing-box/*.json`）。

use std::collections::HashSet;
use std::io::Write;
use std::path::Path;

use lanpath_common::{LanpathError, LanpathResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 数据根内的自定义规则文件名（已纳入加密备份）。
pub const RULES_FILE: &str = "rules.toml";

/// `rules.toml` 顶层文档。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RulesDocument {
    #[serde(default)]
    pub rules: Vec<LocalRule>,
}

/// 单条自定义分流规则。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalRule {
    /// 规则唯一 ID（UUID v4）。
    pub id: String,
    /// 可选名称（空时由 UI 自动展示摘要）。
    #[serde(default)]
    pub name: String,
    /// 是否启用。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 匹配类型。
    pub match_type: RuleMatchType,
    /// 匹配目标（语义由 `match_type` 决定）。
    pub target: String,
    /// 分流动作。
    pub action: RuleAction,
    /// 高级选项。
    #[serde(default)]
    pub advanced: RuleAdvancedOptions,
    /// 备注。
    #[serde(default)]
    pub note: String,
    /// 创建时间戳（Unix 秒，用于稳定排序）。
    pub created_at: u64,
    /// 排序权重（越小越靠前）。
    pub sort_order: i32,
    /// 是否由内置预设生成。
    #[serde(default)]
    pub builtin: bool,
}

/// 规则匹配类型（与 sing-box route rule 字段对齐）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleMatchType {
    Domain,
    DomainSuffix,
    DomainKeyword,
    IpCidr,
    SourceIpCidr,
    RuleSet,
    #[cfg(target_os = "android")]
    AppPackage,
    #[cfg(not(target_os = "android"))]
    ProcessName,
    Port,
    Final,
}

/// 规则路由动作。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    Proxy,
    Direct,
    Reject,
    Outbound { tag: String },
}

impl RuleAction {
    /// 返回核心配置对应的出站 tag。
    #[must_use]
    pub fn outbound_tag(&self) -> &str {
        match self {
            RuleAction::Proxy => "proxy",
            RuleAction::Direct => "direct",
            RuleAction::Reject => "reject",
            RuleAction::Outbound { tag } => tag.as_str(),
        }
    }
}

/// 规则高级选项。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleAdvancedOptions {
    #[serde(default)]
    pub no_resolve: bool,
    #[serde(default)]
    pub invert: bool,
    #[serde(skip)]
    pub _sniff: bool,
}

#[inline]
#[must_use]
const fn default_true() -> bool {
    true
}

/// 解析逗号分隔的 `rule_set` tag 列表，去空白与去重并保序。
#[must_use]
pub fn parse_rule_set_tags(target: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut tags = Vec::new();
    for segment in target.split(',') {
        let tag = segment.trim();
        if tag.is_empty() || !seen.insert(tag.to_owned()) {
            continue;
        }
        tags.push(tag.to_owned());
    }
    tags
}

/// 解析 `rules.toml` 文本并按 `(sort_order, created_at, id)` 排序。
pub fn parse_rules_toml(text: &str) -> LanpathResult<Vec<LocalRule>> {
    let mut document: RulesDocument = toml::from_str(text)
        .map_err(|error| LanpathError::Config(format!("解析 {RULES_FILE} 失败：{error}")))?;
    sort_rules(&mut document.rules);
    Ok(document.rules)
}

/// 从 `<root>/rules.toml` 读取自定义规则；文件不存在时返回空列表且不落盘。
pub fn load_rules(root: &Path) -> LanpathResult<Vec<LocalRule>> {
    let path = root.join(RULES_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => parse_rules_toml(&text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

/// 将规则列表原子写入 `<root>/rules.toml`。
pub fn persist_rules(root: &Path, rules: &[LocalRule]) -> LanpathResult<()> {
    std::fs::create_dir_all(root)?;
    let text = toml::to_string_pretty(&RulesDocument {
        rules: rules.to_vec(),
    })
    .map_err(|error| LanpathError::Config(format!("序列化 {RULES_FILE} 失败：{error}")))?;
    let mut temp = tempfile::NamedTempFile::new_in(root)?;
    temp.write_all(text.as_bytes())?;
    temp.as_file().sync_all()?;
    temp.persist(root.join(RULES_FILE))
        .map_err(|error| error.error)?;
    Ok(())
}

/// 将启用规则按序前置注入 sing-box `route.rules`，并将 `Final` 规则写入 `route.final`。
pub fn apply_singbox_rules(config: &mut Value, rules: &[LocalRule]) {
    let Some(obj) = config.as_object_mut() else {
        return;
    };
    let mut ordered: Vec<&LocalRule> = rules.iter().collect();
    ordered.sort_by(|a, b| {
        a.sort_order
            .cmp(&b.sort_order)
            .then(a.created_at.cmp(&b.created_at))
            .then(a.id.cmp(&b.id))
    });

    let local_rules: Vec<Value> = ordered
        .iter()
        .copied()
        .filter(|r| r.enabled && !matches!(r.match_type, RuleMatchType::Final))
        .map(build_singbox_rule_entry)
        .collect();

    if !local_rules.is_empty() {
        let route = obj
            .entry("route")
            .or_insert_with(|| Value::Object(Default::default()));
        if let Some(route_obj) = route.as_object_mut() {
            let existing = route_obj
                .remove("rules")
                .and_then(|r| r.as_array().cloned())
                .unwrap_or_default();
            let mut combined = local_rules;
            combined.extend(existing);
            route_obj.insert("rules".to_owned(), Value::Array(combined));
        }
    }

    if let Some(final_rule) = ordered
        .into_iter()
        .find(|r| r.enabled && matches!(r.match_type, RuleMatchType::Final))
    {
        let route = obj
            .entry("route")
            .or_insert_with(|| Value::Object(Default::default()));
        if let Some(route_obj) = route.as_object_mut() {
            let tag = match &final_rule.action {
                RuleAction::Reject => "block",
                other => other.outbound_tag(),
            };
            route_obj.insert("final".to_owned(), Value::String(tag.to_owned()));
        }
    }
}

fn sort_rules(rules: &mut [LocalRule]) {
    rules.sort_by(|a, b| {
        a.sort_order
            .cmp(&b.sort_order)
            .then(a.created_at.cmp(&b.created_at))
            .then(a.id.cmp(&b.id))
    });
}

fn build_singbox_rule_entry(rule: &LocalRule) -> Value {
    let mut map = serde_json::Map::new();
    match &rule.match_type {
        RuleMatchType::Domain => {
            map.insert("domain".to_owned(), Value::String(rule.target.clone()));
        }
        RuleMatchType::DomainSuffix => {
            map.insert(
                "domain_suffix".to_owned(),
                Value::String(rule.target.clone()),
            );
        }
        RuleMatchType::DomainKeyword => {
            map.insert(
                "domain_keyword".to_owned(),
                Value::String(rule.target.clone()),
            );
        }
        RuleMatchType::IpCidr => {
            map.insert("ip_cidr".to_owned(), Value::String(rule.target.clone()));
        }
        RuleMatchType::SourceIpCidr => {
            map.insert(
                "source_ip_cidr".to_owned(),
                Value::String(rule.target.clone()),
            );
        }
        RuleMatchType::RuleSet => {
            let tags = parse_rule_set_tags(&rule.target);
            let value = match tags.as_slice() {
                [only] => Value::String(only.clone()),
                _ => Value::Array(tags.into_iter().map(Value::String).collect()),
            };
            map.insert("rule_set".to_owned(), value);
        }
        #[cfg(target_os = "android")]
        RuleMatchType::AppPackage => {
            map.insert(
                "package_name".to_owned(),
                Value::String(rule.target.clone()),
            );
        }
        #[cfg(not(target_os = "android"))]
        RuleMatchType::ProcessName => {
            map.insert(
                "process_name".to_owned(),
                Value::String(rule.target.clone()),
            );
        }
        RuleMatchType::Port => {
            map.insert("port".to_owned(), Value::String(rule.target.clone()));
        }
        RuleMatchType::Final => {}
    }

    if matches!(rule.action, RuleAction::Reject) {
        map.insert("action".to_owned(), Value::String("reject".to_owned()));
    } else {
        map.insert(
            "outbound".to_owned(),
            Value::String(rule.action.outbound_tag().to_owned()),
        );
    }

    if rule.advanced.invert {
        map.insert("invert".to_owned(), Value::Bool(true));
    }

    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample(id: &str, match_type: RuleMatchType, target: &str, action: RuleAction) -> LocalRule {
        LocalRule {
            id: id.into(),
            name: String::new(),
            enabled: true,
            match_type,
            target: target.into(),
            action,
            advanced: RuleAdvancedOptions::default(),
            note: String::new(),
            created_at: 1_700_000_000,
            sort_order: 0,
            builtin: false,
        }
    }

    #[test]
    fn rules_toml_roundtrips_all_actions_and_match_types() {
        let dir = tempfile::tempdir().unwrap();
        let rules = vec![
            sample(
                "r-1",
                RuleMatchType::DomainSuffix,
                "example.com",
                RuleAction::Proxy,
            ),
            sample(
                "r-2",
                RuleMatchType::RuleSet,
                "geosite-cn, geoip-cn",
                RuleAction::Direct,
            ),
            sample(
                "r-3",
                RuleMatchType::Domain,
                "ads.example.com",
                RuleAction::Reject,
            ),
            sample(
                "r-4",
                RuleMatchType::IpCidr,
                "10.0.0.0/8",
                RuleAction::Outbound {
                    tag: "jp-relay".into(),
                },
            ),
        ];

        persist_rules(dir.path(), &rules).unwrap();
        let raw = std::fs::read_to_string(dir.path().join(RULES_FILE)).unwrap();
        assert!(raw.contains("[[rules]]"));
        let loaded = load_rules(dir.path()).unwrap();
        assert_eq!(loaded, rules);
    }

    #[test]
    fn apply_singbox_rules_prepends_enabled_and_sets_final() {
        let mut config = json!({
            "route": {
                "rules": [{ "protocol": "dns", "action": "hijack-dns" }],
                "final": "proxy"
            }
        });
        let mut disabled = sample(
            "r-off",
            RuleMatchType::Domain,
            "skip.test",
            RuleAction::Direct,
        );
        disabled.enabled = false;
        let rules = vec![
            sample(
                "r-1",
                RuleMatchType::RuleSet,
                "geosite-cn, geoip-cn",
                RuleAction::Direct,
            ),
            sample("r-2", RuleMatchType::Domain, "ad.test", RuleAction::Reject),
            disabled,
            sample("r-final", RuleMatchType::Final, "", RuleAction::Direct),
        ];

        apply_singbox_rules(&mut config, &rules);
        let route_rules = config["route"]["rules"].as_array().unwrap();
        assert_eq!(route_rules.len(), 3);
        assert_eq!(
            route_rules[0]["rule_set"],
            json!(["geosite-cn", "geoip-cn"])
        );
        assert_eq!(route_rules[0]["outbound"], "direct");
        assert_eq!(route_rules[1]["action"], "reject");
        assert_eq!(route_rules[2]["protocol"], "dns");
        assert_eq!(config["route"]["final"], "direct");
    }
}
