use super::*;
use crate::host::{MemoryPersistentStore, MockHttpExecutor, PersistentStore, RecordingNotifier};
use crate::persistent_store::FilePersistentStore;
use lanpath_common::{LanpathError, LanpathResult};
use rquickjs::{Context, Runtime};

fn with_js(store: Arc<dyn PersistentStore>, run: impl for<'js> FnOnce(Ctx<'js>, &Arc<ScriptHost>)) {
    let host = Arc::new(ScriptHost::new(
        Arc::new(MockHttpExecutor::with_responses(vec![])),
        store,
        Arc::new(RecordingNotifier::new()),
    ));
    let runtime = Runtime::new().unwrap();
    let context = Context::full(&runtime).unwrap();
    context.with(|ctx| run(ctx, &host));
}

fn assert_js(ctx: &Ctx<'_>, source: &str) {
    assert!(ctx.eval::<bool, _>(source).unwrap());
}

fn execution() -> crate::api::execution::Execution {
    crate::api::execution::Execution::new(
        std::time::Instant::now() + std::time::Duration::from_secs(30),
        Arc::new(crate::cancellation::Cancellation::default()),
    )
}

#[test]
fn explicit_keys_share_across_dialects_instances_and_context_restarts() {
    let root = tempfile::tempdir().unwrap();
    let first: Arc<dyn PersistentStore> =
        Arc::new(FilePersistentStore::new(root.path().to_path_buf()));
    let second: Arc<dyn PersistentStore> =
        Arc::new(FilePersistentStore::new(root.path().to_path_buf()));
    with_js(first, |ctx, host| {
        install_qx(&ctx, host, "qx-script", &execution()).unwrap();
        assert_js(&ctx, r#"$prefs.setValueForKey("token-1", "auth") === true"#);
    });
    with_js(Arc::clone(&second), |ctx, host| {
        install_surge(&ctx, host, "https://example.test/surge.js", &execution()).unwrap();
        assert_js(
            &ctx,
            r#"$persistentStore.read("auth") === "token-1" && $persistentStore.write("token-2", "auth") === true"#,
        );
    });
    with_js(Arc::clone(&second), |ctx, host| {
        install_loon(&ctx, host, "Loon task", &execution()).unwrap();
        assert_js(
            &ctx,
            r#"$persistentStore.read("auth") === "token-2" && $persistentStore.write(undefined, "auth") === true"#,
        );
    });
    with_js(
        Arc::new(FilePersistentStore::new(root.path().to_path_buf())),
        |ctx, host| {
            install_qx(&ctx, host, "another-qx-script", &execution()).unwrap();
            assert_js(&ctx, r#"$prefs.valueForKey("auth") === null"#);
        },
    );
}

#[test]
fn surge_defaults_follow_source_identity_not_explicit_default_key() {
    let store = Arc::new(MemoryPersistentStore::new());
    with_js(store.clone(), |ctx, host| {
        install_surge(&ctx, host, "https://example.test/source.js", &execution()).unwrap();
        assert_js(
            &ctx,
            r#"
            $persistentStore.read() === null &&
            $persistentStore.write("private") === true &&
            $persistentStore.write("explicit", "__default__") === true &&
            $persistentStore.read(undefined) === "private" &&
            $persistentStore.read("__default__") === "explicit"
        "#,
        );
        install_surge(&ctx, host, "https://example.test/other.js", &execution()).unwrap();
        assert_js(
            &ctx,
            r#"
            $persistentStore.read() === null &&
            $persistentStore.read("__default__") === "explicit" &&
            $persistentStore.write("other", undefined) === true
        "#,
        );
        // Reinstalling for a new execution of the same source sees its private slot.
        install_surge(&ctx, host, "https://example.test/source.js", &execution()).unwrap();
        assert_js(
            &ctx,
            r#"
            $persistentStore.read() === "private" &&
            $persistentStore.write(null) === true &&
            $persistentStore.read() === null &&
            $persistentStore.read("__default__") === "explicit" &&
            typeof $persistentStore.erase === "undefined" &&
            typeof $persistentStore.remove === "undefined"
        "#,
        );
    });
    assert_eq!(
        store
            .read("script:https://example.test/other.js", DEFAULT_KEY)
            .unwrap()
            .as_deref(),
        Some("other")
    );
}

#[test]
fn qx_requires_string_keys_and_reset_only_clears_explicit_namespace() {
    let store = Arc::new(MemoryPersistentStore::new());
    store
        .write("script:source", DEFAULT_KEY, "private")
        .unwrap();
    with_js(store.clone(), |ctx, host| {
        install_qx(&ctx, host, "qx", &execution()).unwrap();
        assert_js(
            &ctx,
            r#"
            (() => {
                const invalid = [
                    () => $prefs.valueForKey(),
                    () => $prefs.valueForKey(undefined),
                    () => $prefs.valueForKey(null),
                    () => $prefs.valueForKey(42),
                    () => $prefs.setValueForKey("v"),
                    () => $prefs.setValueForKey("v", undefined),
                    () => $prefs.setValueForKey(null, "k"),
                    () => $prefs.removeValueForKey(),
                    () => $prefs.removeValueForKey(null)
                ];
                if (!invalid.every(call => { try { call(); return false; } catch (_) { return true; } })) return false;
                if ($prefs.setValueForKey("", "") !== true || $prefs.valueForKey("") !== "") return false;
                if ($prefs.removeValueForKey("") !== true || $prefs.valueForKey("") !== null) return false;
                if ($prefs.removeValueForKey("absent") !== true) return false;
                $prefs.setValueForKey("value", "key");
                return $prefs.removeAllValues() === true && $prefs.valueForKey("key") === null;
            })()
        "#,
        );
    });
    assert_eq!(
        store.read("script:source", DEFAULT_KEY).unwrap().as_deref(),
        Some("private")
    );
    assert_eq!(store.read("shared", "__default__").unwrap(), None);
}

#[test]
fn loon_name_defaults_undefined_deletion_and_global_remove_follow_documented_contract() {
    let store = Arc::new(MemoryPersistentStore::new());
    store.write(SHARED_SCOPE, "qx-key", "qx").unwrap();
    store
        .write("script:surge-source", DEFAULT_KEY, "surge")
        .unwrap();
    with_js(store.clone(), |ctx, host| {
        install_loon(&ctx, host, "first name", &execution()).unwrap();
        assert_js(
            &ctx,
            r#"
            $persistentStore.write("first") === true &&
            $persistentStore.read() === "first" &&
            $persistentStore.write("", "empty") === true &&
            $persistentStore.read("empty") === "" &&
            typeof $persistentStore.erase === "undefined"
        "#,
        );
        install_loon(&ctx, host, "second name", &execution()).unwrap();
        assert_js(
            &ctx,
            r#"
            $persistentStore.read() === null &&
            $persistentStore.write("second") === true &&
            $persistentStore.write(undefined) === true &&
            $persistentStore.read() === null &&
            $persistentStore.write(null, "empty") === true &&
            $persistentStore.read("empty") === null
        "#,
        );
        install_loon(&ctx, host, "first name", &execution()).unwrap();
        assert_js(
            &ctx,
            r#"
            $persistentStore.read() === "first" &&
            $persistentStore.write() === true &&
            $persistentStore.read() === null &&
            $persistentStore.remove() === undefined
        "#,
        );
    });
    assert_eq!(store.read(SHARED_SCOPE, "qx-key").unwrap(), None);
    assert_eq!(
        store.read("script:surge-source", DEFAULT_KEY).unwrap(),
        None
    );
}

#[test]
fn invalid_surge_data_and_nonstring_optional_keys_do_not_mutate_values() {
    let store = Arc::new(MemoryPersistentStore::new());
    with_js(store.clone(), |ctx, host| {
        install_surge(&ctx, host, "source", &execution()).unwrap();
        assert_js(
            &ctx,
            r#"
            (() => {
                $persistentStore.write("kept", "key");
                $persistentStore.write("private");
                const invalid = [
                    () => $persistentStore.write(undefined, "key"),
                    () => $persistentStore.write(),
                    () => $persistentStore.write(false, "key"),
                    () => $persistentStore.write(0, "key"),
                    () => $persistentStore.write({}, "key"),
                    () => $persistentStore.write("new", null),
                    () => $persistentStore.read(null),
                    () => $persistentStore.read(3)
                ];
                return invalid.every(call => { try { call(); return false; } catch (_) { return true; } }) &&
                    $persistentStore.read("key") === "kept" && $persistentStore.read() === "private";
            })()
        "#,
        );
    });
}

struct FailedStore;

fn unavailable<T>() -> LanpathResult<T> {
    Err(LanpathError::Internal("owned storage failure".to_owned()))
}

impl PersistentStore for FailedStore {
    fn read(&self, _scope: &str, _key: &str) -> LanpathResult<Option<String>> {
        unavailable()
    }
    fn write(&self, _scope: &str, _key: &str, _value: &str) -> LanpathResult<()> {
        unavailable()
    }
    fn erase(&self, _scope: &str, _key: &str) -> LanpathResult<bool> {
        unavailable()
    }
    fn clear(&self, _scope: &str) -> LanpathResult<()> {
        unavailable()
    }
    fn clear_all(&self) -> LanpathResult<()> {
        unavailable()
    }
}

#[test]
fn qx_and_surge_io_failures_throw_instead_of_fabricating_misses_or_success() {
    with_js(Arc::new(FailedStore), |ctx, host| {
        install_qx(&ctx, host, "qx", &execution()).unwrap();
        install_surge(&ctx, host, "surge", &execution()).unwrap();
        assert_js(
            &ctx,
            r#"
            (() => {
                const operations = [
                    () => $prefs.valueForKey("key"),
                    () => $prefs.setValueForKey("value", "key"),
                    () => $prefs.removeValueForKey("key"),
                    () => $prefs.removeAllValues(),
                    () => $persistentStore.read("key"),
                    () => $persistentStore.write("value", "key"),
                    () => $persistentStore.write(null, "key")
                ];
                return operations.every(call => {
                    try { call(); return false; }
                    catch (error) { return String(error).includes("owned storage failure"); }
                });
            })()
        "#,
        );
    });
}

#[test]
fn loon_writes_report_false_on_io_failure_but_reads_and_remove_throw() {
    with_js(Arc::new(FailedStore), |ctx, host| {
        install_loon(&ctx, host, "loon", &execution()).unwrap();
        assert_js(
            &ctx,
            r#"
            (() => {
                if ($persistentStore.write("value", "key") !== false) return false;
                if ($persistentStore.write(undefined, "key") !== false) return false;
                if ($persistentStore.write(null) !== false) return false;
                return [() => $persistentStore.read(), () => $persistentStore.remove()].every(call => {
                    try { call(); return false; }
                    catch (error) { return String(error).includes("owned storage failure"); }
                });
            })()
        "#,
        );
    });
}

#[test]
fn real_corrupt_disk_state_reaches_javascript_instead_of_becoming_a_cache_hit() {
    let root = tempfile::tempdir().unwrap();
    let store = Arc::new(FilePersistentStore::new(root.path().to_path_buf()));
    with_js(store.clone(), |ctx, host| {
        install_surge(&ctx, host, "source", &execution()).unwrap();
        assert_js(&ctx, r#"$persistentStore.write("saved", "key") === true"#);
        std::fs::write(root.path().join("store-v1.json"), b"broken state").unwrap();
        assert_js(
            &ctx,
            r#"
            [() => $persistentStore.read("key"), () => $persistentStore.write(null, "key")].every(call => {
                try { call(); return false; }
                catch (error) { return String(error).includes("invalid versioned state"); }
            })
        "#,
        );
    });
    assert_eq!(
        std::fs::read(root.path().join("store-v1.json")).unwrap(),
        b"broken state"
    );
}

#[test]
fn stash_name_defaults_share_across_executions_but_only_strings_are_writable() {
    let store = Arc::new(MemoryPersistentStore::new());
    with_js(store.clone(), |ctx, host| {
        install_stash(&ctx, host, "named script", &execution()).unwrap();
        assert_js(
            &ctx,
            r#"
            $persistentStore.read() === null &&
            $persistentStore.write("default") === true &&
            $persistentStore.write("shared", "token") === true
        "#,
        );
        install_stash(&ctx, host, "other script", &execution()).unwrap();
        assert_js(
            &ctx,
            r#"
            $persistentStore.read() === null &&
            $persistentStore.read("token") === "shared"
        "#,
        );
        install_stash(&ctx, host, "named script", &execution()).unwrap();
        assert_js(
            &ctx,
            r#"
            (() => {
                const invalid = [
                    () => $persistentStore.write(null),
                    () => $persistentStore.write(undefined),
                    () => $persistentStore.write(),
                    () => $persistentStore.write(false, "token")
                ];
                return invalid.every(call => {
                    try { call(); return false; } catch (_) { return true; }
                }) && $persistentStore.read() === "default" &&
                    $persistentStore.read("token") === "shared" &&
                    typeof $persistentStore.erase === "undefined" &&
                    typeof $persistentStore.remove === "undefined";
            })()
        "#,
        );
    });
}

#[test]
fn stash_io_failures_throw_instead_of_reporting_saved_data() {
    with_js(Arc::new(FailedStore), |ctx, host| {
        install_stash(&ctx, host, "stash", &execution()).unwrap();
        assert_js(
            &ctx,
            r#"
            [() => $persistentStore.read(), () => $persistentStore.write("value")].every(call => {
                try { call(); return false; }
                catch (error) { return String(error).includes("owned storage failure"); }
            })
        "#,
        );
    });
}
