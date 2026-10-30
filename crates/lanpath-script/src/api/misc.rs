use std::time::{SystemTime, UNIX_EPOCH};

use rquickjs::function::{Func, Opt};
use rquickjs::{Ctx, Object, Value};
use tokio::sync::oneshot;

use crate::api::value_to_string;
use crate::host::TimerRegistry;
use crate::types::{
    ScriptCompletion, ScriptDialect, ScriptExecutionMetadata, ScriptKind, ScriptOutput,
};

/// 公共运行环境，以及按方言公开的 timers / queueMicrotask API。
pub(crate) fn install<'js>(
    ctx: &Ctx<'js>,
    done_tx: oneshot::Sender<ScriptOutput>,
    metadata: &ScriptExecutionMetadata,
    kind: ScriptKind,
    timers: &TimerRegistry,
    dialect: ScriptDialect,
    execution: &super::execution::Execution,
) -> rquickjs::Result<()> {
    let globals = ctx.globals();

    // $done(value)：序列化成功且 sender 接受后提交完成屏障；后续调用不再序列化。
    // Sender::send 消耗自身，故用 Mutex<Option<Sender>> 包裹，保证闭包是 Fn。
    // Ctx 由框架作为首参注入（不捕获，避免 JS 对象持有 Ctx 形成引用循环）。
    let done_tx = std::sync::Mutex::new(Some(done_tx));
    let done_execution = execution.clone();
    let done_fn = move |ctx: Ctx<'js>, value: Opt<Value<'js>>| -> rquickjs::Result<()> {
        if done_execution.completed() {
            return Ok(());
        }
        done_execution.check(&ctx)?;
        let value = value.0.unwrap_or_else(|| Value::new_undefined(ctx.clone()));
        let completion = if value.is_undefined() {
            ScriptCompletion::Undefined
        } else {
            ScriptCompletion::Value
        };
        let json = crate::api::js_to_json(&ctx, value)?;
        // Serializing a value may run user toJSON/getters and accept a nested $done.
        if done_execution.completed() {
            return Ok(());
        }
        done_execution.check(&ctx)?;
        if let Ok(mut tx) = done_tx.lock()
            && let Some(tx) = tx.take()
            && tx.send(ScriptOutput(json, completion)).is_ok()
        {
            done_execution.complete();
        }
        Ok(())
    };
    globals.set("$done", Func::from(done_fn))?;

    // console.log(...args)
    let console = Object::new(ctx.clone())?;
    let log_execution = execution.clone();
    let log_fn =
        move |ctx: Ctx<'js>, args: rquickjs::prelude::Rest<Value<'js>>| -> rquickjs::Result<()> {
            log_execution.check(&ctx)?;
            let parts: Vec<String> = args
                .0
                .iter()
                .map(|v| value_to_string(&ctx, v.clone()))
                .collect();
            log_execution.check(&ctx)?;
            tracing::info!(target: "lanpath_script::console", "{}", parts.join(" "));
            Ok(())
        };
    console.set("log", Func::from(log_fn))?;
    globals.set("console", console)?;

    super::timers::install(ctx, timers, dialect, execution)?;

    // $environment
    let environment = Object::new(ctx.clone())?;
    environment.set("system", std::env::consts::OS)?;
    environment.set("engine", "quickjs")?;
    globals.set("$environment", environment)?;

    // $script
    let script = Object::new(ctx.clone())?;
    script.set("name", metadata.name.as_str())?;
    if dialect == ScriptDialect::Surge {
        script.set("binaryBodyMode", metadata.binary_body_mode)?;
    }
    if dialect == ScriptDialect::Loon {
        let date: Value = ctx.eval("new Date()")?;
        script.set("startTime", date)?;
    } else {
        script.set(
            "startTime",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs_f64())
                .unwrap_or_default(),
        )?;
    }
    script.set("type", kind_name(kind))?;
    globals.set("$script", script)?;
    // Public proprietary/UI hosts are deliberately not simulated.
    let unsupported = Func::from(|ctx: Ctx<'js>| -> rquickjs::Result<()> {
        Err(rquickjs::Exception::throw_message(
            &ctx,
            "proprietary/WebView APIs are unsupported",
        ))
    });
    globals.set("$webView", unsupported)?;

    Ok(())
}

fn kind_name(kind: ScriptKind) -> &'static str {
    match kind {
        ScriptKind::HttpRequest => "http-request",
        ScriptKind::HttpResponse => "http-response",
        ScriptKind::Cron => "cron",
        ScriptKind::Generic => "generic",
    }
}
