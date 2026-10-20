//! Conversion and validation helpers for local override commands.

use lanpath_client::local_override::{CoreLocalOverride, LocalOverride, LocalRule};

use super::views::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(super) fn convert_input_to_model(
    input: SaveLocalOverrideInput,
) -> Result<LocalOverride, String> {
    Ok(LocalOverride {
        ruleset_metadata_version: 1,
        singbox: convert_core_input(input.singbox)?,
        // 内置订阅段已废弃：save 恒写空数组（serde 兼容旧字段，空数组无害）。
        rule_set_subscriptions: Vec::new(),
        // 场景模板已移除：save 恒写空数组（serde 兼容旧字段，空数组无害）。
        applied_templates: Vec::new(),
        custom_rule_sets: input.custom_rule_sets,
        custom_templates: Vec::new(),
        // The seeding marker is not part of the save contract; the command layer backfills it
        // from the on-disk value (see `run_save`) so a deleted built-in rule set stays deleted.
        builtins_seeded: false,
    })
}

pub(super) fn convert_core_input(
    input: CoreLocalOverrideInput,
) -> Result<CoreLocalOverride, String> {
    // 兼容字段：规则总开关已从模型移除（规则/规则集各自带 enabled），
    // 这里显式消费该字段以保持前端契约，同时不写入模型。
    let _ = input.enabled;
    Ok(CoreLocalOverride {
        rules: input
            .rules
            .into_iter()
            .map(convert_rule_input)
            .collect::<Result<Vec<_>, _>>()?,
        rule_sets: input
            .rule_sets
            .into_iter()
            .map(convert_rule_set_ref_input)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn convert_rule_input(input: LocalRuleInput) -> Result<LocalRule, String> {
    let match_type = parse_match_type(&input.match_type)?;
    let action = parse_action(&input.action)?;
    Ok(LocalRule {
        id: input.id,
        name: input.name,
        enabled: input.enabled,
        match_type,
        target: input.target,
        action,
        advanced: lanpath_client::local_override::RuleAdvancedOptions {
            no_resolve: input.no_resolve,
            invert: input.invert,
            _sniff: false,
        },
        note: input.note,
        created_at: input.created_at,
        sort_order: input.sort_order,
        builtin: input.builtin,
    })
}

fn parse_match_type(s: &str) -> Result<lanpath_client::local_override::RuleMatchType, String> {
    match s {
        "domain" => Ok(lanpath_client::local_override::RuleMatchType::Domain),
        "domain_suffix" => Ok(lanpath_client::local_override::RuleMatchType::DomainSuffix),
        "domain_keyword" => Ok(lanpath_client::local_override::RuleMatchType::DomainKeyword),
        "ip_cidr" => Ok(lanpath_client::local_override::RuleMatchType::IpCidr),
        "source_ip_cidr" => Ok(lanpath_client::local_override::RuleMatchType::SourceIpCidr),
        "rule_set" => Ok(lanpath_client::local_override::RuleMatchType::RuleSet),
        #[cfg(target_os = "android")]
        "app_package" => Ok(lanpath_client::local_override::RuleMatchType::AppPackage),
        #[cfg(not(target_os = "android"))]
        "process_name" => Ok(lanpath_client::local_override::RuleMatchType::ProcessName),
        "port" => Ok(lanpath_client::local_override::RuleMatchType::Port),
        "final" => Ok(lanpath_client::local_override::RuleMatchType::Final),
        _ => Err(format!("unknown match_type: {s}")),
    }
}

fn parse_action(s: &str) -> Result<lanpath_client::local_override::RuleAction, String> {
    match s {
        "proxy" => Ok(lanpath_client::local_override::RuleAction::Proxy),
        "direct" => Ok(lanpath_client::local_override::RuleAction::Direct),
        "reject" => Ok(lanpath_client::local_override::RuleAction::Reject),
        _ => {
            if let Some(tag) = s.strip_prefix("outbound:") {
                Ok(lanpath_client::local_override::RuleAction::Outbound {
                    tag: tag.to_string(),
                })
            } else {
                Err(format!("unknown action: {s}"))
            }
        }
    }
}

fn convert_rule_set_ref_input(
    input: LocalRuleSetRefInput,
) -> Result<lanpath_client::local_override::LocalRuleSetRef, String> {
    let kind = parse_rule_set_kind(&input.kind)?;
    let source = lanpath_client::local_override::RuleSetSource::Remote { url: input.source };
    Ok(lanpath_client::local_override::LocalRuleSetRef {
        id: input.id,
        name: input.name,
        tag: input.tag,
        kind,
        source,
        enabled: input.enabled,
        auto_update_interval_minutes: input.auto_update_interval_minutes,
        last_updated: input.last_updated,
    })
}

fn parse_rule_set_kind(s: &str) -> Result<lanpath_client::local_override::RuleSetKind, String> {
    match s {
        "singbox_remote" => Ok(lanpath_client::local_override::RuleSetKind::SingBoxRemote),
        "singbox_local" => Ok(lanpath_client::local_override::RuleSetKind::SingBoxLocal),
        _ => Err(format!("unknown rule_set kind: {s}")),
    }
}

// ---------------------------------------------------------------------------
// Round-trip tests: model → view → input → model
// ---------------------------------------------------------------------------
//
// The frontend receives `LocalRuleView` (string-typed enums), then echoes the
// exact same strings back as `LocalRuleInput` / `LocalRuleSetRefInput`. The
// view strings must therefore be accepted by the `parse_*` functions below, or
// saves fail with `unknown match_type` / `unknown action` / `unknown rule_set
// kind` (regression: `format!("{:?}").to_lowercase()` emitted `"ruleset"`,
// `"domainsuffix"`, … which `parse_match_type` rejected).

#[cfg(test)]
mod roundtrip_tests {
    use super::*;

    fn sample_rule(match_type: lanpath_client::local_override::RuleMatchType) -> LocalRule {
        LocalRule {
            id: "r1".to_string(),
            name: "rule".to_string(),
            enabled: true,
            match_type,
            target: "example.com".to_string(),
            action: lanpath_client::local_override::RuleAction::Proxy,
            advanced: Default::default(),
            note: String::new(),
            created_at: 1,
            sort_order: 0,
            builtin: false,
        }
    }

    /// `LocalRule` → `LocalRuleView` → `LocalRuleInput` → `LocalRule`.
    fn rule_round_trip(model: LocalRule) -> LocalRule {
        let view = LocalRuleView::from_model(&model);
        let input = LocalRuleInput {
            builtin: view.builtin,
            id: view.id,
            name: view.name,
            enabled: view.enabled,
            match_type: view.match_type,
            target: view.target,
            action: view.action,
            no_resolve: view.no_resolve,
            invert: view.invert,
            note: view.note,
            created_at: view.created_at,
            sort_order: view.sort_order,
        };
        convert_rule_input(input).unwrap()
    }

    #[test]
    fn every_match_type_round_trips_through_view_string() {
        use lanpath_client::local_override::RuleMatchType as M;
        let cases: Vec<(M, &str)> = vec![
            (M::Domain, "domain"),
            (M::DomainSuffix, "domain_suffix"),
            (M::DomainKeyword, "domain_keyword"),
            (M::IpCidr, "ip_cidr"),
            (M::SourceIpCidr, "source_ip_cidr"),
            (M::RuleSet, "rule_set"),
            (M::Port, "port"),
            (M::Final, "final"),
            #[cfg(not(target_os = "android"))]
            (M::ProcessName, "process_name"),
            #[cfg(target_os = "android")]
            (M::AppPackage, "app_package"),
        ];
        for (match_type, expected) in cases {
            let model = sample_rule(match_type);
            let view = LocalRuleView::from_model(&model);
            assert_eq!(
                view.match_type, expected,
                "match_type {:?}",
                model.match_type
            );
            let back = rule_round_trip(model.clone());
            assert_eq!(
                back, model,
                "match_type {:?} must survive round trip",
                model.match_type
            );
        }
    }

    #[test]
    fn every_action_round_trips_through_view_string() {
        use lanpath_client::local_override::RuleAction as A;
        let cases: Vec<(A, &str)> = vec![
            (A::Proxy, "proxy"),
            (A::Direct, "direct"),
            (A::Reject, "reject"),
            (
                A::Outbound {
                    tag: "my-group".to_string(),
                },
                "outbound:my-group",
            ),
        ];
        for (action, expected) in cases {
            let mut model = sample_rule(lanpath_client::local_override::RuleMatchType::Domain);
            model.action = action;
            let view = LocalRuleView::from_model(&model);
            assert_eq!(view.action, expected, "action {:?}", model.action);
            let back = rule_round_trip(model.clone());
            assert_eq!(
                back, model,
                "action {:?} must survive round trip",
                model.action
            );
        }
    }

    #[test]
    fn rule_set_kind_round_trips_through_view_string() {
        use lanpath_client::local_override::{LocalRuleSetRef, RuleSetKind as K, RuleSetSource};
        let cases: Vec<(K, &str)> = vec![
            (K::SingBoxRemote, "singbox_remote"),
            (K::SingBoxLocal, "singbox_local"),
        ];
        for (kind, expected) in cases {
            let model = LocalRuleSetRef {
                id: "rs1".to_string(),
                name: "RS".to_string(),
                tag: "rs-tag".to_string(),
                kind,
                source: RuleSetSource::Remote {
                    url: "https://example.com/x.srs".to_string(),
                },
                enabled: true,
                auto_update_interval_minutes: 0,
                last_updated: 0,
            };
            let view = LocalRuleSetRefView::from_model(&model);
            assert_eq!(view.kind, expected, "kind {:?}", model.kind);
            let input = LocalRuleSetRefInput {
                id: view.id,
                name: view.name,
                tag: view.tag,
                kind: view.kind,
                source: view.source,
                enabled: view.enabled,
                auto_update_interval_minutes: view.auto_update_interval_minutes,
                last_updated: view.last_updated,
            };
            let back = convert_rule_set_ref_input(input).unwrap();
            assert_eq!(back.kind, model.kind);
        }
    }
}

// ---------------------------------------------------------------------------
// rule_set multi-tag validation
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "convert_tests.rs"]
mod rule_set_validation_tests;
