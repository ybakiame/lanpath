//! 本地规则预估，不读取/伪造运行中核心命中；缺少上下文时保留不确定性。
use lanpath_client::{LocalRule, RuleMatchType};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RuleTrace {
    pub(crate) target: String,
    pub(crate) matched: Option<String>,
    pub(crate) outbound: Option<String>,
    pub(crate) uncertain_before: usize,
}

pub(crate) fn trace_rules(rules: &[LocalRule], target: &str) -> RuleTrace {
    let target = target.trim().trim_end_matches('.').to_ascii_lowercase();
    let mut uncertain_before = 0;
    for rule in rules.iter().filter(|r| r.enabled) {
        let result = match rule.match_type {
            RuleMatchType::Domain => Some(target == rule.target.to_ascii_lowercase()),
            RuleMatchType::DomainSuffix => {
                let suffix = rule.target.trim_start_matches('.').to_ascii_lowercase();
                Some(target == suffix || target.ends_with(&format!(".{suffix}")))
            }
            RuleMatchType::DomainKeyword => {
                Some(target.contains(&rule.target.to_ascii_lowercase()))
            }
            RuleMatchType::IpCidr => target
                .parse::<std::net::IpAddr>()
                .ok()
                .map(|ip| in_cidr(ip, &rule.target)),
            RuleMatchType::Final => Some(true),
            _ => None,
        };
        match result.map(|matched| matched != rule.advanced.invert) {
            Some(true) => {
                return RuleTrace {
                    target: target.clone(),
                    matched: Some(super::rule_line(rule)),
                    outbound: Some(rule.action.outbound_tag().into()),
                    uncertain_before,
                };
            }
            None => uncertain_before += 1,
            _ => (),
        }
    }
    RuleTrace {
        target,
        matched: None,
        outbound: None,
        uncertain_before,
    }
}
fn in_cidr(ip: std::net::IpAddr, target: &str) -> bool {
    let Some((base, prefix)) = target.split_once('/') else {
        return false;
    };
    let (Ok(base), Ok(prefix)) = (base.parse::<std::net::IpAddr>(), prefix.parse::<u32>()) else {
        return false;
    };
    match (ip, base) {
        (std::net::IpAddr::V4(ip), std::net::IpAddr::V4(base)) if prefix <= 32 => {
            let mask = u32::MAX.checked_shl(32 - prefix).unwrap_or(0);
            u32::from(ip) & mask == u32::from(base) & mask
        }
        (std::net::IpAddr::V6(ip), std::net::IpAddr::V6(base)) if prefix <= 128 => {
            let mask = u128::MAX.checked_shl(128 - prefix).unwrap_or(0);
            u128::from(ip) & mask == u128::from(base) & mask
        }
        _ => false,
    }
}
