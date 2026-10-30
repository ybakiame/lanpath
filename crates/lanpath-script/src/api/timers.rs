use crate::{host::TimerRegistry, types::ScriptDialect};
use rquickjs::{Ctx, Function, Persistent, function::Func};
use std::time::Duration;

pub(super) fn install<'js>(
    ctx: &Ctx<'js>,
    timers: &TimerRegistry,
    dialect: ScriptDialect,
    execution: &super::execution::Execution,
) -> rquickjs::Result<()> {
    let globals = ctx.globals();
    let registry = timers.clone();
    let schedule_execution = execution.clone();
    globals.set(
        "__lanpathScheduleTimer",
        Func::from(
            move |ctx: Ctx<'js>,
                  callback: Function<'js>,
                  ms: i64,
                  repeat: bool|
                  -> rquickjs::Result<u32> {
                schedule_execution.check(&ctx)?;
                if ms > 86_400_000 {
                    return Err(rquickjs::Exception::throw_range(
                        &ctx,
                        "timer delay exceeds 24 hours",
                    ));
                }
                let delay = Duration::from_millis(ms.max(0) as u64);
                registry
                    .borrow_mut()
                    .schedule(Persistent::save(&ctx, callback), delay, repeat)
                    .map_err(|message| rquickjs::Exception::throw_range(&ctx, message))
            },
        ),
    )?;
    if dialect == ScriptDialect::Stash {
        // Keep JS function references in a JS closure, not a Rust Func capture:
        // RustFunction::trace cannot trace captured Function/Ctx ownership cycles.
        let callback_execution = execution.clone();
        globals.set(
            "__lanpathDispatchMicrotask",
            Func::from(move |callback: Function<'js>| -> rquickjs::Result<()> {
                if !callback_execution.active() {
                    return Ok(());
                }
                callback.call::<_, ()>(())
            }),
        )?;
        let microtask_execution = execution.clone();
        globals.set(
            "__lanpathQueueMicrotask",
            Func::from(
                move |ctx: Ctx<'js>, dispatch: Function<'js>, callback: Function<'js>| {
                    microtask_execution.check(&ctx)?;
                    dispatch.defer((callback,))
                },
            ),
        )?;
        let registry = timers.clone();
        let clear_execution = execution.clone();
        globals.set(
            "__lanpathClearTimer",
            Func::from(move |ctx: Ctx<'js>, id: u32| -> rquickjs::Result<()> {
                clear_execution.check(&ctx)?;
                registry.borrow_mut().cancel(id);
                Ok(())
            }),
        )?;
        ctx.eval::<(), _>(r#"(function(schedule, clear, enqueue, dispatch) {
            globalThis.queueMicrotask = function(callback) {
                return enqueue(dispatch, callback);
            };
            globalThis.setTimeout = function(callback, delay, ...args) {
                return schedule(function() { callback(...args); }, delay === undefined ? 0 : delay, false);
            };
            globalThis.setInterval = function(callback, delay, ...args) {
                return schedule(function() { callback(...args); }, delay === undefined ? 0 : delay, true);
            };
            globalThis.clearTimeout = clear;
            globalThis.clearInterval = clear;
            delete globalThis.__lanpathScheduleTimer;
            delete globalThis.__lanpathClearTimer;
            delete globalThis.__lanpathQueueMicrotask;
            delete globalThis.__lanpathDispatchMicrotask;
        })(__lanpathScheduleTimer, __lanpathClearTimer, __lanpathQueueMicrotask, __lanpathDispatchMicrotask);"#)?;
    } else {
        ctx.eval::<(), _>(
            r#"(function(schedule) {
            globalThis.setTimeout = function(callback, delay, ...args) {
                schedule(function() { callback(...args); }, delay === undefined ? 0 : delay, false);
            };
            delete globalThis.__lanpathScheduleTimer;
        })(__lanpathScheduleTimer);"#,
        )?;
        // Loon explicitly defines setInterval as a one-shot operation.
        if dialect == ScriptDialect::Loon {
            let timeout: Function = globals.get("setTimeout")?;
            globals.set("setInterval", timeout)?;
        }
    }
    Ok(())
}
