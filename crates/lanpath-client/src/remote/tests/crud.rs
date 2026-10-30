use super::*;
use crate::import::parse_import;

#[test]
fn local_collections_keep_rewrites_and_hosts_but_never_activate_missing_sources() {
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().to_path_buf());
    let imported = parse_import("[Script]\nhook=type=http-response,pattern=^https://example.invalid/,script-path=https://assets.invalid/hook.js\ntask=type=cron,cronexp=\"0 9 * * *\",script-path=https://assets.invalid/task.js\n[URL Rewrite]\n^https://example.invalid/ - reject\n[MITM]\nhostname=example.invalid", ScriptDialect::Surge).unwrap();
    let summary = manager.merge_imported(&imported).unwrap();
    assert_eq!(
        (
            summary.rewrites,
            summary.hostnames,
            summary.scripts,
            summary.tasks
        ),
        (1, 1, 0, 0)
    );
    assert_eq!(summary.warnings.len(), 2);
    let merged = manager.load_cached().unwrap();
    assert_eq!(merged.rewrites.len(), 1);
    assert_eq!(merged.hostnames, ["example.invalid"]);
    assert!(merged.scripts.is_empty() && merged.task_scripts.is_empty());
    assert!(!root.path().join("remote_cache/imported.json").exists());
    let ids: Vec<String> = serde_json::from_str(
        &std::fs::read_to_string(root.path().join("remote_cache/local_imports.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(ids.len(), 1);
    assert!(
        root.path()
            .join(format!("remote_cache/local/{}.json", ids[0]))
            .exists()
    );
}

#[test]
fn local_imports_are_independent_and_resolve_only_their_own_defaults() {
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().to_path_buf());
    for (value, hostname) in [("first", "one.invalid"), ("second", "two.invalid")] {
        let mut imported = parse_import(&format!("#!arguments=token:{value}\n[Script]\nhook=type=http-request,pattern=^https://{hostname}/,script-path=https://assets.invalid/hook.js,argument={{{{token}}}}\n[MITM]\nhostname={hostname}"), ScriptDialect::Surge).unwrap();
        // Use a literal template rather than format-string brace escaping.
        imported.scripts[0].argument = Some("{token}".into());
        imported.scripts[0].source = "$done({});".into();
        manager.merge_imported(&imported).unwrap();
    }
    let merged = manager.load_cached().unwrap();
    assert_eq!(merged.scripts.len(), 2);
    assert_eq!(merged.hostnames, ["one.invalid", "two.invalid"]);
    assert_eq!(merged.scripts[0].argument.as_deref(), Some("first"));
    assert_eq!(merged.scripts[1].argument.as_deref(), Some("second"));
    assert_ne!(merged.scripts[0].module_id, merged.scripts[1].module_id);
    // Registration, not mere file existence, controls local activation.
    let index = root.path().join("remote_cache/local_imports.json");
    let mut ids: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string(&index).unwrap()).unwrap();
    ids.remove(0);
    std::fs::write(index, serde_json::to_string(&ids).unwrap()).unwrap();
    let remaining = manager.load_cached().unwrap();
    assert_eq!(remaining.scripts.len(), 1);
    assert_eq!(remaining.scripts[0].argument.as_deref(), Some("second"));
    assert_eq!(remaining.hostnames, ["two.invalid"]);
}

#[tokio::test]
async fn disabling_and_deleting_remote_makes_cached_hooks_tasks_and_hosts_inert() {
    let base = spawn_remote_server(|base| format!("[Script]\nhook=type=http-response,pattern=^https://example.invalid/,script-path={base}/hook.js,requires-body=true,timeout=2\ntask=type=cron,cronexp=\"0 9 * * *\",script-path={base}/task.js,argument=literal\n[MITM]\nhostname=example.invalid")).await;
    let root = tempfile::tempdir().unwrap();
    let service = RemoteResourceService::new(root.path());
    service
        .add(RemoteResource {
            name: "module".into(),
            url: format!("{base}/snippet"),
            kind: RemoteKind::Snippet,
            dialect: ScriptDialect::Surge,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(service.fetch_all().await.unwrap().fetched, 1);
    let manager = RemoteManager::new(root.path().to_path_buf());
    let active = manager.load_cached().unwrap();
    assert_eq!(
        (
            active.scripts.len(),
            active.task_scripts.len(),
            active.hostnames.len()
        ),
        (1, 1, 1)
    );
    assert_eq!(active.scripts[0].source, "const hook = 1;");
    assert_eq!(active.scripts[0].source_id, format!("{base}/hook.js"));
    assert_eq!(active.scripts[0].timeout_ms, Some(2000));
    assert_eq!(active.task_scripts[0].source, "const task = 2;");
    assert_eq!(collect_hostname_sources(root.path()).len(), 1);
    service.set_enabled("module", false).await.unwrap();
    let disabled = manager.load_cached().unwrap();
    assert!(
        disabled.scripts.is_empty()
            && disabled.task_scripts.is_empty()
            && disabled.hostnames.is_empty()
    );
    assert!(collect_hostname_sources(root.path()).is_empty());
    service.set_enabled("module", true).await.unwrap();
    assert_eq!(manager.load_cached().unwrap().scripts.len(), 1);
    service.remove("module").await.unwrap();
    let removed = manager.load_cached().unwrap();
    assert!(
        removed.scripts.is_empty()
            && removed.task_scripts.is_empty()
            && removed.hostnames.is_empty()
    );
    assert!(collect_hostname_sources(root.path()).is_empty());
    // The retained cache is recoverable, but cannot reactivate without its manifest.
    assert!(root.path().join("remote_cache").exists());
}

#[test]
fn legacy_and_future_caches_are_recoverable_but_never_activated() {
    let root = tempfile::tempdir().unwrap();
    let manager = RemoteManager::new(root.path().to_path_buf());
    let remote = RemoteResource {
        name: "legacy".into(),
        url: "https://assets.invalid/module".into(),
        kind: RemoteKind::Snippet,
        ..Default::default()
    };
    manager.save(std::slice::from_ref(&remote)).unwrap();
    let directory = root.path().join("remote_cache");
    std::fs::create_dir_all(&directory).unwrap();
    let legacy = r#"{"rewrites":[],"scripts":[],"hostnames":["private.invalid"]}"#;
    std::fs::write(directory.join("legacy.json"), legacy).unwrap();
    std::fs::write(directory.join("imported.json"), legacy).unwrap();
    let merged = manager.load_cached().unwrap();
    assert!(merged.scripts.is_empty() && merged.hostnames.is_empty());
    assert!(
        merged
            .warnings
            .iter()
            .any(|warning| warning.contains("refetch"))
    );
    assert!(
        merged
            .warnings
            .iter()
            .any(|warning| warning.contains("reimport"))
    );
    assert!(collect_hostname_sources(root.path()).is_empty());
    assert_eq!(
        std::fs::read_to_string(directory.join("legacy.json")).unwrap(),
        legacy
    );
    let id = crate::remote::activation::remote_module_id(&remote);
    let future = serde_json::json!({"version": REMOTE_CACHE_VERSION + 1, "module_id": id, "hostnames": ["future.invalid"]});
    std::fs::write(
        directory.join(format!("{}.json", id.replace(':', "-"))),
        future.to_string(),
    )
    .unwrap();
    assert!(manager.load_cached().unwrap().hostnames.is_empty());
}
