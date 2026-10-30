use std::sync::Arc;

use rquickjs::function::{Func, Opt};
use rquickjs::{Ctx, Exception, IntoJs, Object, Value};

use crate::host::ScriptHost;
use crate::persistent_store::{DEFAULT_KEY, SHARED_SCOPE};

/// Explicit vendor keys share one namespace across scripts and dialects. An
/// omitted key has a private default slot: source identity for Surge, or the
/// display script name for Stash/Loon.
fn location<'js>(
    ctx: &Ctx<'js>,
    identity: &str,
    key: Opt<Value<'js>>,
) -> rquickjs::Result<(String, String)> {
    match key.0 {
        None => Ok((format!("script:{identity}"), DEFAULT_KEY.to_owned())),
        Some(key) if key.is_undefined() => {
            Ok((format!("script:{identity}"), DEFAULT_KEY.to_owned()))
        }
        Some(key) => {
            let Some(key) = key.as_string() else {
                return Err(Exception::throw_type(ctx, "storage key must be a string"));
            };
            Ok((SHARED_SCOPE.to_owned(), key.to_string()?))
        }
    }
}

fn read<'js>(
    ctx: &Ctx<'js>,
    host: &ScriptHost,
    scope: &str,
    key: &str,
) -> rquickjs::Result<Value<'js>> {
    match host
        .store
        .read(scope, key)
        .map_err(|e| Exception::throw_message(ctx, &e.to_string()))?
    {
        Some(value) => value.into_js(ctx),
        None => Ok(Value::new_null(ctx.clone())),
    }
}

/// QX keys are required; removeAllValues clears the shared explicit-key namespace.
/// Storage errors are thrown rather than returning an apparent successful write.
pub(crate) fn install_qx<'js>(
    ctx: &Ctx<'js>,
    host: &Arc<ScriptHost>,
    _script_name: &str,
    execution: &super::execution::Execution,
) -> rquickjs::Result<()> {
    let prefs = Object::new(ctx.clone())?;
    let host_read = Arc::clone(host);
    let read_execution = execution.clone();
    prefs.set(
        "valueForKey",
        Func::from(move |ctx: Ctx<'js>, key: String| {
            read_execution.check(&ctx)?;
            read(&ctx, &host_read, SHARED_SCOPE, &key)
        }),
    )?;

    let host_write = Arc::clone(host);
    let write_execution = execution.clone();
    prefs.set(
        "setValueForKey",
        Func::from(
            move |ctx: Ctx<'js>, value: String, key: String| -> rquickjs::Result<bool> {
                write_execution.check(&ctx)?;
                host_write
                    .store
                    .write(SHARED_SCOPE, &key, &value)
                    .map_err(|e| Exception::throw_message(&ctx, &e.to_string()))?;
                Ok(true)
            },
        ),
    )?;

    let host_erase = Arc::clone(host);
    let erase_execution = execution.clone();
    prefs.set(
        "removeValueForKey",
        Func::from(
            move |ctx: Ctx<'js>, key: String| -> rquickjs::Result<bool> {
                erase_execution.check(&ctx)?;
                host_erase
                    .store
                    .erase(SHARED_SCOPE, &key)
                    .map_err(|e| Exception::throw_message(&ctx, &e.to_string()))?;
                Ok(true)
            },
        ),
    )?;

    let host_clear = Arc::clone(host);
    let clear_execution = execution.clone();
    prefs.set(
        "removeAllValues",
        Func::from(move |ctx: Ctx<'js>| -> rquickjs::Result<bool> {
            clear_execution.check(&ctx)?;
            host_clear
                .store
                .clear(SHARED_SCOPE)
                .map_err(|e| Exception::throw_message(&ctx, &e.to_string()))?;
            Ok(true)
        }),
    )?;

    ctx.globals().set("$prefs", prefs)
}

/// Surge write(data, [key]): null deletes; successful writes/deletes return
/// true. Omitted keys refer to the same script source, not its transient hook name.
/// https://manual.nssurge.com/scripting/api.html
pub(crate) fn install_surge<'js>(
    ctx: &Ctx<'js>,
    host: &Arc<ScriptHost>,
    source_id: &str,
    execution: &super::execution::Execution,
) -> rquickjs::Result<()> {
    let pstore = persistent_store(ctx, host, source_id, StoreDialect::Surge, execution)?;
    ctx.globals().set("$persistentStore", pstore)
}

/// Stash permits string values only, and defaults to the display script name.
/// https://stash.wiki/en/script/syntax-and-interface
pub(crate) fn install_stash<'js>(
    ctx: &Ctx<'js>,
    host: &Arc<ScriptHost>,
    script_name: &str,
    execution: &super::execution::Execution,
) -> rquickjs::Result<()> {
    let pstore = persistent_store(ctx, host, script_name, StoreDialect::Stash, execution)?;
    ctx.globals().set("$persistentStore", pstore)
}

/// Loon defaults to $script.name, permits undefined/null deletion, and remove()
/// clears all script storage, not only the current default slot. Writes report
/// I/O failure as false; read/reset errors throw instead of masquerading as misses.
/// https://nsloon.app/en/docs/Script/script_api#5-persistent-storage
pub(crate) fn install_loon<'js>(
    ctx: &Ctx<'js>,
    host: &Arc<ScriptHost>,
    script_name: &str,
    execution: &super::execution::Execution,
) -> rquickjs::Result<()> {
    let pstore = persistent_store(ctx, host, script_name, StoreDialect::Loon, execution)?;
    let host_clear = Arc::clone(host);
    let execution = execution.clone();
    pstore.set(
        "remove",
        Func::from(move |ctx: Ctx<'js>| -> rquickjs::Result<()> {
            execution.check(&ctx)?;
            host_clear
                .store
                .clear_all()
                .map_err(|e| Exception::throw_message(&ctx, &e.to_string()))
        }),
    )?;
    ctx.globals().set("$persistentStore", pstore)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StoreDialect {
    Surge,
    Stash,
    Loon,
}

fn persistent_store<'js>(
    ctx: &Ctx<'js>,
    host: &Arc<ScriptHost>,
    default_identity: &str,
    dialect: StoreDialect,
    execution: &super::execution::Execution,
) -> rquickjs::Result<Object<'js>> {
    let pstore = Object::new(ctx.clone())?;
    let identity = default_identity.to_owned();
    let host_read = Arc::clone(host);
    let read_execution = execution.clone();
    pstore.set(
        "read",
        Func::from(move |ctx: Ctx<'js>, key: Opt<Value<'js>>| {
            read_execution.check(&ctx)?;
            let (scope, key) = location(&ctx, &identity, key)?;
            read(&ctx, &host_read, &scope, &key)
        }),
    )?;

    let identity = default_identity.to_owned();
    let host_write = Arc::clone(host);
    let write_execution = execution.clone();
    pstore.set(
        "write",
        Func::from(
            move |ctx: Ctx<'js>,
                  data: Opt<Value<'js>>,
                  key: Opt<Value<'js>>|
                  -> rquickjs::Result<bool> {
                write_execution.check(&ctx)?;
                let (scope, key) = location(&ctx, &identity, key)?;
                let data = data.0.unwrap_or_else(|| Value::new_undefined(ctx.clone()));
                let deletes = match dialect {
                    StoreDialect::Surge => data.is_null(),
                    StoreDialect::Stash => false,
                    StoreDialect::Loon => data.is_null() || data.is_undefined(),
                };
                let result = if deletes {
                    host_write.store.erase(&scope, &key).map(|_| ())
                } else if let Some(value) = data.as_string() {
                    host_write.store.write(&scope, &key, &value.to_string()?)
                } else {
                    let message = if dialect == StoreDialect::Stash {
                        "storage data must be a string"
                    } else {
                        "storage data must be a string or null"
                    };
                    return Err(Exception::throw_type(&ctx, message));
                };
                if dialect == StoreDialect::Loon {
                    Ok(result.is_ok())
                } else {
                    result.map_err(|e| Exception::throw_message(&ctx, &e.to_string()))?;
                    Ok(true)
                }
            },
        ),
    )?;
    Ok(pstore)
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
