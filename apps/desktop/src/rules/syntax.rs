//! 简短的批量规则输入，先整批解析，任一错误不写盘。
use lanpath_client::{LocalRule, RuleAction, RuleAdvancedOptions, RuleMatchType};
use lanpath_common::{LanpathError, LanpathResult};

pub(crate) fn parse_lines(source: &str) -> LanpathResult<Vec<LocalRule>> {
    let mut rules = Vec::new();
    for (number, line) in source.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut result = parse_line(line)
            .map_err(|error| LanpathError::Validation(format!("第 {} 行：{error}", number + 1)))?;
        result.sort_order = rules.len() as i32;
        rules.push(result);
    }
    if rules.is_empty() {
        return Err(LanpathError::Validation("请输入至少一条规则".into()));
    }
    Ok(rules)
}
fn parse_line(line: &str) -> Result<LocalRule, String> {
    let (condition, action) = line
        .split_once("->")
        .ok_or("格式：suffix example.com -> proxy")?;
    let (kind, target) = condition
        .trim()
        .split_once(char::is_whitespace)
        .ok_or("缺少匹配值")?;
    let target = target.trim();
    if target.is_empty() {
        return Err("缺少匹配值".into());
    }
    let match_type = match kind {
        "domain" => RuleMatchType::Domain,
        "suffix" => RuleMatchType::DomainSuffix,
        "keyword" => RuleMatchType::DomainKeyword,
        "cidr" => {
            validate_cidr(target)?;
            RuleMatchType::IpCidr
        }
        "process" => RuleMatchType::ProcessName,
        "port" => {
            validate_port(target)?;
            RuleMatchType::Port
        }
        "ruleset" => {
            validate_ruleset_target(target)?;
            RuleMatchType::RuleSet
        }
        _ => {
            return Err(
                "匹配类型：domain / suffix / keyword / cidr / process / port / ruleset".into(),
            );
        }
    };
    let target = if matches!(
        match_type,
        RuleMatchType::Domain | RuleMatchType::DomainSuffix
    ) {
        normalize_domain(target)?
    } else {
        target.to_owned()
    };
    let action = match action.trim() {
        "" => return Err("缺少出口目标（如 proxy / direct / reject 或自定义策略组名）".into()),
        "proxy" => RuleAction::Proxy,
        "direct" => RuleAction::Direct,
        "reject" => RuleAction::Reject,
        custom => {
            if custom.contains(char::is_whitespace) || custom.contains("->") {
                return Err("出口名称不能包含空白或 ->".into());
            }
            RuleAction::Outbound {
                tag: custom.to_owned(),
            }
        }
    };
    Ok(LocalRule {
        id: uuid::Uuid::new_v4().to_string(),
        name: String::new(),
        enabled: true,
        match_type,
        target,
        action,
        advanced: RuleAdvancedOptions::default(),
        note: String::new(),
        created_at: chrono::Utc::now().timestamp().max(0) as u64,
        sort_order: 0,
        builtin: false,
    })
}
pub(crate) fn normalize_domain(value: &str) -> Result<String, String> {
    let value = value.trim().trim_end_matches('.').to_ascii_lowercase();
    if value.is_empty()
        || value.len() > 253
        || value.split('.').any(|label| {
            label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
    {
        return Err("请输入域名（不含协议、路径或端口；国际域名请用 Punycode）".into());
    }
    Ok(value)
}
pub(super) fn validate_cidr(target: &str) -> Result<(), String> {
    let (address, prefix) = target
        .split_once('/')
        .ok_or("网段格式：192.168.0.0/16 或 2001:db8::/32")?;
    let ip: std::net::IpAddr = address.parse().map_err(|_| "IP 地址无效")?;
    let prefix: u8 = prefix.parse().map_err(|_| "网段前缀无效")?;
    if prefix > if ip.is_ipv4() { 32 } else { 128 } {
        return Err("网段前缀超出范围".into());
    }
    Ok(())
}
pub(super) fn validate_port(target: &str) -> Result<(), String> {
    let (start, end) = target.split_once('-').unwrap_or((target, target));
    let start: u16 = start.parse().map_err(|_| "端口应为 1–65535 或递增范围")?;
    let end: u16 = end.parse().map_err(|_| "端口应为 1–65535 或递增范围")?;
    if start == 0 || end < start {
        return Err("端口应为 1–65535 或递增范围".into());
    }
    Ok(())
}
pub(super) fn validate_ruleset_target(target: &str) -> Result<(), String> {
    let tags = lanpath_client::rules::parse_rule_set_tags(target);
    if tags.is_empty() {
        return Err("请输入规则集标签或 @分组（多个用逗号分隔）".into());
    }
    for token in &tags {
        let raw = token
            .strip_prefix(lanpath_client::rulesets::GROUP_REF_PREFIX)
            .unwrap_or(token);
        lanpath_client::rulesets::validate_rule_set_tag(raw).map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub(crate) fn rule_line(rule: &LocalRule) -> String {
    let kind = match rule.match_type {
        RuleMatchType::Domain => "domain",
        RuleMatchType::DomainSuffix => "suffix",
        RuleMatchType::DomainKeyword => "keyword",
        RuleMatchType::IpCidr => "cidr",
        RuleMatchType::ProcessName => "process",
        RuleMatchType::Port => "port",
        RuleMatchType::Final => "final",
        RuleMatchType::SourceIpCidr => "source-cidr",
        RuleMatchType::RuleSet => "ruleset",
    };
    format!("{kind} {} -> {}", rule.target, rule.action.outbound_tag())
}
pub(crate) fn editable(rule: &LocalRule) -> bool {
    !rule.advanced.invert
        && !rule.advanced.no_resolve
        && matches!(
            rule.match_type,
            RuleMatchType::Domain
                | RuleMatchType::DomainSuffix
                | RuleMatchType::DomainKeyword
                | RuleMatchType::IpCidr
                | RuleMatchType::ProcessName
                | RuleMatchType::Port
                | RuleMatchType::RuleSet
        )
}
