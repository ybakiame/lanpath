use super::*;
use crate::import::{ArgSpec, parse_import};

// Exact argument and MITM declarations from Task/WPS/WPS.sgmodule. Intentionally
// exclude its remote hooks and cron: these tests never fetch or execute WPS assets.
const WPS_MITM: &str =
    "#!arguments=禁用MITM:hostname\n[MITM]\n{{{禁用MITM}}} = %APPEND% personal-act.wps.cn\n";

fn resource() -> RemoteResource {
    RemoteResource {
        name: "parameterized-module".into(),
        url: "https://example.invalid/module.sgmodule".into(),
        kind: RemoteKind::Snippet,
        dialect: ScriptDialect::Surge,
        ..Default::default()
    }
}

fn cache_remote(
    manager: &RemoteManager,
    remote: &RemoteResource,
    content: &str,
) -> std::path::PathBuf {
    let imported = parse_import(content, remote.dialect).unwrap();
    let id = crate::remote::activation::remote_module_id(remote);
    let mut cached = CachedRemoteConfig::from_imported(&imported);
    cached.bind_module(&id);
    let dir = manager.data_dir.join("remote_cache");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{}.json", id.replace(':', "-")));
    std::fs::write(&path, serde_json::to_vec(&cached).unwrap()).unwrap();
    path
}

fn assert_active(manager: &RemoteManager, expected: &[&str], source: &str) {
    let merged = manager.load_cached().unwrap();
    let mut hostnames = merged.hostnames;
    hostnames.sort();
    let mut expected: Vec<String> = expected.iter().map(|value| (*value).to_string()).collect();
    expected.sort();
    assert_eq!(hostnames, expected);
    let sources = collect_hostname_sources(&manager.data_dir);
    assert_eq!(
        sources
            .iter()
            .map(|entry| entry.hostname.clone())
            .collect::<Vec<_>>(),
        expected
    );
    for entry in sources {
        assert_eq!(entry.sources, [source]);
    }
}

#[test]
fn wps_defaults_disable_and_reenable_offline_without_freezing_templates() {
    let dir = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(dir.path().to_path_buf());
    let mut remote = resource();
    manager.save(&[remote.clone()]).unwrap();
    let path = cache_remote(&manager, &remote, WPS_MITM);
    let original = std::fs::read(&path).unwrap();
    let cached: CachedRemoteConfig = serde_json::from_slice(&original).unwrap();
    assert!(cached.hostnames.is_empty());
    assert_eq!(cached.mitm_templates.len(), 1);
    assert_active(&manager, &["personal-act.wps.cn"], &remote.name);

    remote.argument_values = vec![("禁用MITM".into(), "#".into())];
    manager.save(&[remote.clone()]).unwrap();
    assert_active(&manager, &[], &remote.name);
    remote.argument_values[0].1 = "hostname".into();
    manager.save(&[remote.clone()]).unwrap();
    assert_active(&manager, &["personal-act.wps.cn"], &remote.name);

    remote.enabled = false;
    manager.save(&[remote.clone()]).unwrap();
    assert_active(&manager, &[], &remote.name);
    remote.enabled = true;
    manager.save(&[remote.clone()]).unwrap();
    assert_active(&manager, &["personal-act.wps.cn"], &remote.name);
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn current_manifest_defaults_and_user_values_have_identical_runtime_and_provenance_priority() {
    let dir = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(dir.path().to_path_buf());
    let mut remote = resource();
    cache_remote(&manager, &remote, WPS_MITM);
    remote.arguments = vec![ArgSpec {
        key: "禁用MITM".into(),
        default_value: "#".into(),
        ..Default::default()
    }];
    manager.save(&[remote.clone()]).unwrap();
    assert_active(&manager, &[], &remote.name);
    remote.argument_values = vec![("禁用MITM".into(), "hostname".into())];
    manager.save(&[remote.clone()]).unwrap();
    assert_active(&manager, &["personal-act.wps.cn"], &remote.name);
    remote.arguments[0].default_value = "hostname".into();
    remote.argument_values[0].1 = "#".into();
    manager.save(&[remote.clone()]).unwrap();
    assert_active(&manager, &[], &remote.name);
    remote.argument_values.clear();
    manager.save(&[remote.clone()]).unwrap();
    assert_active(&manager, &["personal-act.wps.cn"], &remote.name);
}

#[test]
fn local_import_uses_declared_defaults_and_reports_template_hostname_counts() {
    let dir = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(dir.path().to_path_buf());
    let imported = parse_import(WPS_MITM, ScriptDialect::Surge).unwrap();
    assert!(imported.hostnames.is_empty());
    assert_eq!(imported.mitm_templates.len(), 1);
    let summary = manager.merge_imported(&imported).unwrap();
    assert_eq!(summary.hostnames, 1);
    assert_active(&manager, &["personal-act.wps.cn"], "本地导入");
}

#[test]
fn locally_disabled_defaults_remain_inactive_and_unsafe_defaults_warn_without_source_text() {
    for content in [
        "#!arguments=key:#\n[MITM]\n{{{key}}} = default.example\n",
        "#!arguments=key:p12,value:private-ca-secret\n[MITM]\n{{{key}}} = {{{value}}}\n",
        "[MITM]\nhostname = {{{private-unresolved-key}}}\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let manager = RemoteManager::new(dir.path().to_path_buf());
        let imported = parse_import(content, ScriptDialect::Surge).unwrap();
        let summary = manager.merge_imported(&imported).unwrap();
        assert_eq!(summary.hostnames, 0);
        assert_active(&manager, &[], "本地导入");
        if content.contains("private") {
            assert!(!summary.warnings.is_empty());
            assert!(
                summary
                    .warnings
                    .iter()
                    .all(|warning| !warning.contains("private"))
            );
        }
    }
}

#[tokio::test]
async fn fetch_summary_counts_current_parameters_not_only_static_hostnames() {
    let base = spawn_remote_server(|_| WPS_MITM.to_string()).await;
    let dir = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(dir.path().to_path_buf());
    let mut remote = resource();
    remote.url = format!("{base}/snippet");
    manager.save(&[remote.clone()]).unwrap();
    let report = manager.fetch_all(&[remote.clone()]).await;
    assert_eq!((report.fetched, report.hostnames), (1, 1));
    assert_active(&manager, &["personal-act.wps.cn"], &remote.name);
    remote.argument_values = vec![("禁用MITM".into(), "#".into())];
    manager.save(&[remote.clone()]).unwrap();
    let report = manager.fetch_all(&[remote.clone()]).await;
    assert_eq!((report.fetched, report.hostnames), (1, 0));
    assert_active(&manager, &[], &remote.name);
}

#[test]
fn generic_key_and_value_templates_preserve_static_entries_and_exclusions() {
    let dir = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(dir.path().to_path_buf());
    let mut remote = resource();
    cache_remote(
        &manager,
        &remote,
        "#!arguments=key:hostname,domain:default.example\n[MITM]\nhostname = static.example\n{{{key}}} = %APPEND% {{{domain}}}, !excluded.example\n",
    );
    remote.argument_values = vec![("domain".into(), "user.example, *.other.example".into())];
    manager.save(&[remote.clone()]).unwrap();
    assert_active(
        &manager,
        &[
            "static.example",
            "user.example",
            "*.other.example",
            "-excluded.example",
        ],
        &remote.name,
    );
    remote.argument_values.push(("key".into(), "#".into()));
    manager.save(&[remote.clone()]).unwrap();
    assert_active(&manager, &["static.example"], &remote.name);
}

#[test]
fn unsafe_or_unresolved_parameters_never_activate_or_leak_in_warnings() {
    let dir = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(dir.path().to_path_buf());
    let mut remote = resource();
    cache_remote(
        &manager,
        &remote,
        "#!arguments=key:hostname,value:default.example\n[MITM]\n{{{key}}} = {{{value}}}\nhostname = {{{undeclared}}}\n",
    );
    remote.argument_values = vec![("undeclared".into(), "must-stay-inactive.example".into())];
    for (key, value) in [
        ("key", "skip_validating_cert"),
        ("key", "p12"),
        ("key", "hostname\n[Script]\nsecret-injected-script"),
        ("key", "hostname\rsecret-control"),
        ("key", "hostname\u{0000}secret-control"),
        (
            "value",
            "secret.example\n[MITM]\nskip_validating_cert = true",
        ),
        ("value", "secret.example\tother.example"),
        ("value", "secret.example, [Script]"),
        ("value", "secret.example, p12=secret-ca"),
        ("value", "{{{undeclared}}}"),
    ] {
        remote.argument_values.truncate(1);
        remote.argument_values.push((key.into(), value.into()));
        manager.save(&[remote.clone()]).unwrap();
        assert_active(&manager, &[], &remote.name);
        let merged = manager.load_cached().unwrap();
        assert!(!merged.warnings.is_empty());
        for warning in merged.warnings {
            assert!(!warning.contains(value));
            assert!(!warning.contains("secret"));
            assert!(!warning.contains("undeclared"));
            assert!(!warning.contains("must-stay-inactive"));
        }
    }
}

#[test]
fn old_v2_shape_keeps_static_hostnames_without_inventing_lost_templates() {
    let dir = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(dir.path().to_path_buf());
    let remote = resource();
    manager.save(std::slice::from_ref(&remote)).unwrap();
    let id = crate::remote::activation::remote_module_id(&remote);
    let old = serde_json::json!({
        "version": REMOTE_CACHE_VERSION,
        "module_id": id,
        "hostnames": ["old-static.example"],
        "meta": {"arguments": [{"key": "禁用MITM", "default_value": "hostname", "description": null}]}
    });
    let cached: CachedRemoteConfig = serde_json::from_value(old.clone()).unwrap();
    assert!(cached.mitm_templates.is_empty());
    let cache_dir = dir.path().join("remote_cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::write(
        cache_dir.join(format!("{}.json", id.replace(':', "-"))),
        serde_json::to_vec(&old).unwrap(),
    )
    .unwrap();
    assert_active(&manager, &["old-static.example"], &remote.name);
}

#[test]
fn literal_formatting_and_port_scopes_survive_but_parameter_controls_do_not() {
    let dir = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(dir.path().to_path_buf());
    let imported = parse_import(
        "#!arguments=key:hostname,domain:api.example:443\n[MITM]\n{{{key}}}\t=\t%APPEND% {{{domain}}},\t[::1]:8443,\t!skip.example:443\n",
        ScriptDialect::Surge,
    ).unwrap();
    let summary = manager.merge_imported(&imported).unwrap();
    assert_eq!(summary.hostnames, 3);
    assert_active(
        &manager,
        &["api.example:443", "[::1]:8443", "-skip.example:443"],
        "本地导入",
    );
}
