use super::*;
use lanpath_client::RuleAction;
use serde_json::json;

#[test]
fn batch_validation_reports_line_and_saves_nothing_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let rules = Rules::for_root(dir.path()).unwrap();
    let before = rules.load().unwrap();
    let error = rules
        .add_lines("suffix example.com -> proxy\ncidr invalid -> direct", true)
        .unwrap_err();
    assert!(error.to_string().contains("第 2 行"));
    assert_eq!(rules.load().unwrap(), before);
}
#[test]
fn add_edit_disable_reorder_and_remove_persist() {
    let dir = tempfile::tempdir().unwrap();
    let service = Rules::for_root(dir.path()).unwrap();
    let added = service
        .add_lines(
            "# personal rules\nsuffix Example.COM. -> proxy\nprocess chrome.exe -> direct",
            true,
        )
        .unwrap();
    let first = added
        .iter()
        .find(|r| r.target == "example.com")
        .unwrap()
        .id
        .clone();
    let second = added
        .iter()
        .find(|r| r.target == "chrome.exe")
        .unwrap()
        .id
        .clone();
    assert_eq!(added[0].id, first);
    service.move_rule(&second, -1).unwrap();
    assert_eq!(service.load().unwrap()[0].id, second);
    service
        .edit(&first, "domain www.example.com -> reject")
        .unwrap();
    service.toggle(&first).unwrap();
    let reloaded = Rules::for_root(dir.path()).unwrap().load().unwrap();
    let edited = reloaded.iter().find(|r| r.id == first).unwrap();
    assert_eq!(edited.target, "www.example.com");
    assert_eq!(edited.action, RuleAction::Reject);
    assert!(!edited.enabled);
    service.remove(&first).unwrap();
    assert!(!service.load().unwrap().iter().any(|r| r.id == first));
}
#[test]
fn rules_reach_both_core_configs_in_the_same_order() {
    let rules = parse_lines("suffix example.com -> proxy\ncidr 10.0.0.0/8 -> direct\nport 8000-9000 -> reject\nprocess test.exe -> direct").unwrap();
    let mut singbox = json!({"route":{"rules":[]},"outbounds":[]});
    lanpath_client::apply_singbox_rules(&mut singbox, &rules);
    normalize_singbox_ports(&mut singbox);
    assert_eq!(
        singbox["route"]["rules"][0]["domain_suffix"],
        json!("example.com")
    );
    assert_eq!(
        singbox["route"]["rules"][2]["port_range"],
        json!(["8000:9000"])
    );
    assert_eq!(singbox["route"]["rules"][2]["action"], "reject");
    let mut mihomo = json!({"rules":["MATCH,proxy"]});
    assert!(apply_mihomo_rules(&mut mihomo, &rules).is_empty());
    assert_eq!(mihomo["rules"][0], "DOMAIN-SUFFIX,example.com,proxy");
    assert_eq!(mihomo["rules"][2], "DST-PORT,8000-9000,REJECT");
    assert_eq!(mihomo["rules"][4], "MATCH,proxy");
}
#[test]
fn suffix_boundaries_and_unknown_context_do_not_claim_real_routing() {
    let rules = parse_lines("process chrome.exe -> direct\nsuffix example.com -> proxy").unwrap();
    let trace = trace_rules(&rules, "www.example.com");
    assert_eq!(trace.outbound.as_deref(), Some("proxy"));
    assert_eq!(trace.uncertain_before, 1);
    assert!(trace_rules(&rules, "notexample.com").matched.is_none());
    let cidr = parse_lines("cidr 10.0.0.0/8 -> direct\ncidr 2001:db8::/32 -> reject").unwrap();
    assert_eq!(
        trace_rules(&cidr, "10.2.3.4").outbound.as_deref(),
        Some("direct")
    );
    assert_eq!(
        trace_rules(&cidr, "2001:db8::1").outbound.as_deref(),
        Some("reject")
    );
}
#[test]
fn invalid_ports_networks_and_domains_are_rejected() {
    for line in [
        "port 0 -> direct",
        "port 100-2 -> direct",
        "cidr ::/129 -> proxy",
        "suffix https://example.com -> direct",
        "suffix example.com -> typo",
    ] {
        assert!(parse_lines(line).is_err(), "{line}");
    }
}
#[test]
fn rules_share_the_existing_backup_document() {
    let dir = tempfile::tempdir().unwrap();
    let rules = Rules::for_root(dir.path()).unwrap();
    rules
        .add_lines("domain example.com -> direct", true)
        .unwrap();
    let raw = std::fs::read_to_string(dir.path().join(RULES_FILE)).unwrap();
    let document = lanpath_client::rules::parse_rules_toml(&raw).unwrap();
    assert!(document.iter().any(|r| r.target == "example.com"));
}
