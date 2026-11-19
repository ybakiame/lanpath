//! Windows 两个核心共享快速规则语义；不支持的高级规则明确警告。
use lanpath_client::{LocalRule, RuleAction, RuleMatchType};
use serde_json::Value;

pub(crate) fn apply_mihomo_rules(config: &mut Value, rules: &[LocalRule]) -> Vec<String> {
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    for rule in rules.iter().filter(|r| r.enabled) {
        if rule.advanced.invert {
            warnings.push(format!("Mihomo 未应用反向规则：{}", rule.id));
            continue;
        }
        let kind = match rule.match_type {
            RuleMatchType::Domain => "DOMAIN",
            RuleMatchType::DomainSuffix => "DOMAIN-SUFFIX",
            RuleMatchType::DomainKeyword => "DOMAIN-KEYWORD",
            RuleMatchType::IpCidr => {
                if rule.target.contains(':') {
                    "IP-CIDR6"
                } else {
                    "IP-CIDR"
                }
            }
            RuleMatchType::SourceIpCidr => "SRC-IP-CIDR",
            RuleMatchType::ProcessName => "PROCESS-NAME",
            RuleMatchType::Port => "DST-PORT",
            RuleMatchType::Final => "MATCH",
            RuleMatchType::RuleSet => {
                warnings.push(format!("Mihomo 未应用 sing-box 规则集：{}", rule.target));
                continue;
            }
        };
        let action = match &rule.action {
            RuleAction::Direct => "DIRECT",
            RuleAction::Reject => "REJECT",
            other => other.outbound_tag(),
        };
        let mut value = if matches!(rule.match_type, RuleMatchType::Final) {
            format!("{kind},{action}")
        } else {
            format!("{kind},{},{action}", rule.target)
        };
        if rule.advanced.no_resolve && matches!(rule.match_type, RuleMatchType::IpCidr) {
            value.push_str(",no-resolve");
        }
        entries.push(Value::String(value));
    }
    if let Some(existing) = config.get_mut("rules").and_then(Value::as_array_mut) {
        entries.append(existing);
        *existing = entries;
    } else {
        config["rules"] = Value::Array(entries);
    }
    warnings
}

/// 共享旧模型使用字符串端口；核心要求单端口为数字、范围为 port_range。
pub(crate) fn normalize_singbox_ports(config: &mut Value) {
    if let Some(rules) = config
        .pointer_mut("/route/rules")
        .and_then(Value::as_array_mut)
    {
        for rule in rules {
            if let Some(port) = rule.get("port").and_then(Value::as_str).map(str::to_owned) {
                if let Ok(number) = port.parse::<u16>() {
                    rule["port"] = number.into();
                } else if super::syntax::validate_port(&port).is_ok()
                    && let Some(object) = rule.as_object_mut()
                {
                    object.remove("port");
                    object.insert(
                        "port_range".into(),
                        serde_json::json!([port.replace('-', ":")]),
                    );
                }
            }
        }
    }
}
