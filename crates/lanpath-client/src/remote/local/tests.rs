use super::*;
use crate::remote::{RemoteManager, RemoteResourceService};
fn configured() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    crate::settings::ensure_app_settings(root.path()).unwrap();
    root
}
fn source(content: &str) -> LocalSource {
    LocalSource {
        name: "Owned module".into(),
        enabled: true,
        dialect: ScriptDialect::Surge,
        content: Some(content.into()),
        icon: None,
    }
}
const SNIPPET: &str = "[Script]\nhook = type=http-request,pattern=^https://example.invalid/,script-path=https://assets.invalid/hook.js\n[MITM]\nhostname = example.invalid\n";
fn assets(body: &str) -> Vec<(String, String)> {
    vec![("https://assets.invalid/hook.js".into(), body.into())]
}

#[tokio::test]
async fn resource_lifecycle_replaces_instead_of_appending_and_controls_activation() {
    let root = configured();
    let store = LocalResourceStore::new(root.path());
    let document = store
        .prepare(None, source(SNIPPET), &assets("$done({});"))
        .unwrap();
    let id = document.id.clone();
    store.save(document, true).await.unwrap();
    let manager = RemoteManager::new(root.path().into());
    assert_eq!(manager.load_cached().unwrap().scripts.len(), 1);
    store.set_enabled(&id, false).await.unwrap();
    assert!(manager.load_cached().unwrap().scripts.is_empty());
    assert!(manager.load_cached().unwrap().hostnames.is_empty());
    let previous = store.load().unwrap().pop().unwrap();
    let mut edited = previous.source();
    edited.enabled = true;
    edited.name = "Renamed".into();
    let updated = store
        .prepare(Some(&previous), edited, &assets("$done({updated:true});"))
        .unwrap();
    assert_eq!(updated.id, id);
    store.save(updated, false).await.unwrap();
    assert_eq!(store.load().unwrap().len(), 1);
    assert_eq!(
        manager.load_cached().unwrap().scripts[0].source,
        "$done({updated:true});"
    );
    store.remove(&id).await.unwrap();
    assert!(store.load().unwrap().is_empty());
    assert!(manager.load_cached().unwrap().scripts.is_empty());
    assert!(
        !root
            .path()
            .join(format!("resources/imports/local/{id}.json"))
            .exists()
    );
}
#[tokio::test]
async fn invalid_or_unbound_edits_do_not_replace_the_saved_bundle() {
    let root = configured();
    let store = LocalResourceStore::new(root.path());
    let document = store
        .prepare(None, source(SNIPPET), &assets("$done({});"))
        .unwrap();
    let id = document.id.clone();
    store.save(document, true).await.unwrap();
    let path = root
        .path()
        .join(format!("resources/imports/local/{id}.json"));
    let before = std::fs::read(&path).unwrap();
    let previous = store.load().unwrap().pop().unwrap();
    for supplied in [
        vec![],
        assets(""),
        vec![("https://unrelated.invalid/a.js".into(), "$done({});".into())],
    ] {
        assert!(
            store
                .prepare(Some(&previous), source(SNIPPET), &supplied)
                .is_err()
        );
    }
    assert!(
        store
            .prepare(
                Some(&previous),
                source(
                    "[Script]\nbroken = type=unknown,script-path=https://assets.invalid/hook.js"
                ),
                &assets("$done({});")
            )
            .is_err()
    );
    assert_eq!(std::fs::read(path).unwrap(), before);
}
#[tokio::test]
async fn legacy_import_keeps_original_rules_and_js_without_fabricating_source() {
    let root = configured();
    let manager = RemoteManager::new(root.path().into());
    let mut parsed = crate::import::parse_import(SNIPPET, ScriptDialect::Surge).unwrap();
    parsed.scripts[0].source = "$done({legacy:true});".into();
    manager.merge_imported(&parsed).unwrap();
    let store = LocalResourceStore::new(root.path());
    let previous = store.load().unwrap().pop().unwrap();
    assert!(previous.source().content.is_none());
    let updated = store
        .prepare(
            Some(&previous),
            previous.source(),
            &assets("$done({edited:true});"),
        )
        .unwrap();
    store.save(updated, false).await.unwrap();
    let current = store.load().unwrap().pop().unwrap();
    assert!(current.source().content.is_none());
    assert_eq!(current.cached.hostnames, previous.cached.hostnames);
    assert_eq!(
        current.cached.scripts[0].pattern,
        previous.cached.scripts[0].pattern
    );
    assert_eq!(
        manager.load_cached().unwrap().scripts[0].source,
        "$done({edited:true});"
    );
}
#[tokio::test]
async fn icons_and_source_survive_cache_clear_and_portable_backup() {
    use crate::backup::{capture, decode, encode};
    let root = configured();
    let store = LocalResourceStore::new(root.path());
    crate::ClientConfig::new(root.path().into(), "", "", Default::default())
        .save()
        .unwrap();
    let mut src = source(SNIPPET);
    src.icon = Some(LocalIcon::from_bytes("module.png", b"\x89PNG\r\n\x1a\nfixture").unwrap());
    let document = store.prepare(None, src, &assets("$done({});")).unwrap();
    store.save(document, true).await.unwrap();
    std::fs::create_dir_all(root.path().join(".cache/icons")).unwrap();
    std::fs::remove_dir_all(root.path().join(".cache")).unwrap();
    let saved = store.load().unwrap().pop().unwrap();
    assert!(saved.source().icon.is_some());
    // These exports include resources/imports unchanged, including the original source and icon.
    let snapshot = capture(root.path(), None).unwrap();
    let encrypted = encode(&snapshot, "local-resource-test").unwrap();
    let restored = decode(&encrypted, "local-resource-test").unwrap();
    let key = format!("resources/imports/local/{}.json", saved.id);
    let roundtrip: CachedRemoteConfig = serde_json::from_slice(&restored.files[&key]).unwrap();
    assert_eq!(
        roundtrip.local.as_ref().unwrap().content.as_deref(),
        Some(SNIPPET)
    );
    assert_eq!(roundtrip.local.unwrap().icon, saved.source().icon);
}
#[test]
fn rejects_non_gpui_and_invalid_icon_formats() {
    let root = tempfile::tempdir().unwrap();
    assert!(LocalResourceStore::new(root.path()).load().is_err());
    assert!(LocalIcon::from_bytes("image.svg", b"<svg></svg>").is_err());
    assert!(LocalIcon::from_bytes("huge.png", &vec![0; 512 * 1024 + 1]).is_err());
}
#[tokio::test]
async fn damaged_registry_is_never_replaced_and_unknown_files_never_activate() {
    let root = configured();
    let store = LocalResourceStore::new(root.path());
    let document = store
        .prepare(None, source(SNIPPET), &assets("$done({});"))
        .unwrap();
    let directory = root.path().join("resources/imports");
    std::fs::create_dir_all(directory.join("local")).unwrap();
    std::fs::write(directory.join("local_imports.json"), b"broken").unwrap();
    assert!(store.save(document, true).await.is_err());
    assert_eq!(
        std::fs::read(directory.join("local_imports.json")).unwrap(),
        b"broken"
    );
    std::fs::write(directory.join("local_imports.json"), b"[]").unwrap();
    std::fs::write(directory.join("local/orphan.json"), b"{}").unwrap();
    assert!(store.load().unwrap().is_empty());
    assert!(
        RemoteManager::new(root.path().into())
            .load_cached()
            .unwrap()
            .scripts
            .is_empty()
    );
    // Existing legacy shell import still works with the optional bundle field absent.
    let legacy = tempfile::tempdir().unwrap();
    let service = RemoteResourceService::new(legacy.path());
    service
        .import_content("[MITM]\nhostname = legacy.invalid", ScriptDialect::Surge)
        .await
        .unwrap();
    assert_eq!(
        RemoteManager::new(legacy.path().into())
            .load_cached()
            .unwrap()
            .hostnames,
        vec!["legacy.invalid"]
    );
}

#[tokio::test]
async fn local_filenames_bind_only_inside_owned_bundles_and_do_not_read_disk() {
    let root = configured();
    let store = LocalResourceStore::new(root.path());
    for (dialect, content) in [
        (
            ScriptDialect::Surge,
            "[Script]\nhook = type=http-request,pattern=^https://example.invalid/,script-path=hook.js",
        ),
        (
            ScriptDialect::QuantumultX,
            "[rewrite_local]\n^https://example.invalid/ url script-request-header hook.js",
        ),
    ] {
        assert!(
            !crate::import::parse_import(content, dialect)
                .unwrap()
                .warnings
                .is_empty()
        );
        let mut src = source(content);
        src.dialect = dialect;
        let document = store
            .prepare(None, src, &[("hook.js".into(), "$done({});".into())])
            .unwrap();
        assert_eq!(document.cached.scripts[0].source_id, "hook.js");
        store.save(document, true).await.unwrap();
    }
    assert_eq!(
        RemoteManager::new(root.path().into())
            .load_cached()
            .unwrap()
            .scripts
            .len(),
        2
    );
}

#[tokio::test]
async fn supported_body_limit_advisory_does_not_block_local_resource_saving() {
    let root = configured();
    let store = LocalResourceStore::new(root.path());
    let snippet = "[Script]\nresponse=type=http-response,pattern=^https://example.invalid/,script-path=response.js,requires-body=true,max-size=-1";
    let supplied = vec![("response.js".into(), "$done({});".into())];
    let document = store.prepare(None, source(snippet), &supplied).unwrap();
    assert_eq!(document.cached.scripts[0].max_size, usize::MAX);
    store.save(document, true).await.unwrap();
    assert_eq!(
        RemoteManager::new(root.path().into())
            .load_cached()
            .unwrap()
            .scripts
            .len(),
        1
    );
    let previous = store.load().unwrap().pop().unwrap();
    let oversized = snippet.replace("max-size=-1", "max-size=33554433");
    assert!(
        store
            .prepare(Some(&previous), source(&oversized), &supplied)
            .is_err()
    );
    let unsupported = snippet.replace("max-size=-1", "max-size=-1,engine=webview");
    assert!(
        store
            .prepare(Some(&previous), source(&unsupported), &supplied)
            .is_err()
    );
    assert_eq!(
        store.load().unwrap()[0].cached.scripts[0].max_size,
        usize::MAX
    );
}
