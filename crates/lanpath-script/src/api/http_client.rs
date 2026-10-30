use std::sync::Arc;

use rquickjs::function::{Async, Func};
use rquickjs::{Ctx, Function, Object, Value};

use super::execution::Execution;
use super::http_options;
use crate::host::ScriptHost;
use crate::types::ScriptDialect;

/// Callback errors are execution failures, not discarded Promise rejections.
pub(crate) fn install<'js>(
    ctx: &Ctx<'js>,
    host: &Arc<ScriptHost>,
    dialect: ScriptDialect,
    execution: &Execution,
) -> rquickjs::Result<()> {
    let client = Object::new(ctx.clone())?;
    macro_rules! method {
        ($name:literal, $method:literal) => {{
            let host = Arc::clone(host);
            let execution = execution.clone();
            let function = Async(
                move |ctx: Ctx<'js>, value: Value<'js>, callback: Function<'js>| {
                    let host = Arc::clone(&host);
                    let execution = execution.clone();
                    async move {
                        if !execution.active() {
                            return Ok::<(), rquickjs::Error>(());
                        }
                        let options = http_options::parse(&ctx, &value, dialect);
                        let result = match options {
                            Ok(mut options) => {
                                options.request.method = $method.into();
                                let binary = options.binary;
                                let full = options.full_headers;
                                match execution.run(host.http.execute(options.request)).await {
                                    None => return Ok(()),
                                    Some(Err(error)) => Err(error.to_string()),
                                    Some(Ok(response)) => {
                                        build_response(&ctx, response, binary, full, dialect)
                                            .map_err(|error| error.to_string())
                                    }
                                }
                            }
                            Err(error) => Err(error.to_string()),
                        };
                        if !execution.active() {
                            return Ok(());
                        }
                        let null = Value::new_null(ctx.clone());
                        let called = match result {
                            Ok((response, data)) => callback.call::<_, ()>((null, response, data)),
                            Err(message) => callback.call::<_, ()>((message, null.clone(), null)),
                        };
                        if let Err(error) = called {
                            execution.callback_error(&ctx, error);
                        }
                        Ok(())
                    }
                },
            );
            client.set($name, Func::from(function))?;
        }};
    }
    method!("get", "GET");
    method!("post", "POST");
    method!("put", "PUT");
    method!("delete", "DELETE");
    method!("head", "HEAD");
    method!("options", "OPTIONS");
    method!("patch", "PATCH");
    ctx.globals().set("$httpClient", client)?;
    Ok(())
}

fn build_response<'js>(
    ctx: &Ctx<'js>,
    response: crate::types::HttpResponseData,
    binary: bool,
    full: bool,
    dialect: ScriptDialect,
) -> rquickjs::Result<(Value<'js>, Value<'js>)> {
    let object = Object::new(ctx.clone())?;
    object.set("status", response.status)?;
    object.set("statusCode", response.status)?;
    object.set(
        "headers",
        http_options::headers(ctx, &response.headers, full)?,
    )?;
    object.set(
        "fullHeaders",
        http_options::headers(ctx, &response.headers, true)?,
    )?;
    let data = if binary
        || (dialect == ScriptDialect::Loon && std::str::from_utf8(&response.body).is_err())
    {
        super::body::binary(ctx, response.body, false)?
    } else {
        super::body::text(ctx, response.body)?
    };
    Ok((object.into_value(), data))
}
