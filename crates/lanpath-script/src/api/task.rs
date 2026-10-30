use std::sync::Arc;

use rquickjs::function::{Async, Func};
use rquickjs::{Ctx, Object, Value};

use super::execution::Execution;
use super::http_options;
use crate::host::ScriptHost;
use crate::types::ScriptDialect;

pub(crate) fn install<'js>(
    ctx: &Ctx<'js>,
    host: &Arc<ScriptHost>,
    execution: &Execution,
) -> rquickjs::Result<()> {
    let task = Object::new(ctx.clone())?;
    let host = Arc::clone(host);
    let execution = execution.clone();
    let fetch = Async(move |ctx: Ctx<'js>, value: Value<'js>| {
        let host = Arc::clone(&host);
        let execution = execution.clone();
        async move {
            if !execution.active() {
                return Ok(Value::new_undefined(ctx.clone()));
            }
            let options = http_options::parse(&ctx, &value, ScriptDialect::QuantumultX)
                .map_err(|error| throw_error(&ctx, &error.to_string()))?;
            match execution.run(host.http.execute(options.request)).await {
                None => Ok(Value::new_undefined(ctx.clone())),
                Some(Err(error)) => Err(throw_error(&ctx, &error.to_string())),
                Some(Ok(response)) => {
                    let result = Object::new(ctx.clone())?;
                    result.set("statusCode", response.status)?;
                    result.set(
                        "headers",
                        http_options::headers(&ctx, &response.headers, false)?,
                    )?;
                    result.set(
                        "fullHeaders",
                        http_options::headers(&ctx, &response.headers, true)?,
                    )?;
                    // QX exposes raw bytes even when the response is not UTF-8.
                    if let Ok(text) = std::str::from_utf8(&response.body) {
                        result.set("body", text)?;
                    }
                    result.set("bodyBytes", super::body::binary(&ctx, response.body, true)?)?;
                    Ok(result.into_value())
                }
            }
        }
    });
    task.set("fetch", Func::from(fetch))?;
    ctx.globals().set("$task", task)?;
    Ok(())
}

fn throw_error(ctx: &Ctx<'_>, message: &str) -> rquickjs::Error {
    // Public QX samples consume reason.error, not a rejected string.
    match Object::new(ctx.clone()).and_then(|object| {
        object.set("error", message)?;
        object.set("message", message)?;
        Ok(object.into_value())
    }) {
        Ok(value) => ctx.throw(value),
        Err(error) => error,
    }
}
