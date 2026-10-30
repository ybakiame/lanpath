pub(crate) mod body;
pub(crate) mod execution;
pub(crate) mod http_client;
pub(crate) mod http_options;
pub(crate) mod misc;
pub(crate) mod notify;
pub(crate) mod store;
pub(crate) mod task;
mod timers;

use std::sync::Arc;

use rquickjs::{Ctx, Object, Value};
use tokio::sync::oneshot;

use crate::host::{ScriptHost, TimerRegistry};
use crate::types::{ScriptDialect, ScriptExecutionMetadata, ScriptKind, ScriptOutput};

/// Borrowed identity and trigger of one execution; no JS values or runtime ownership.
pub(crate) struct ScriptContext<'a> {
    pub dialect: ScriptDialect,
    pub kind: ScriptKind,
    pub metadata: &'a ScriptExecutionMetadata,
}

/// 注入全部全局 API。`$done` 的 oneshot sender 被移入 `$done` 函数。
pub(crate) fn install_apis<'js>(
    ctx: &Ctx<'js>,
    host: &Arc<ScriptHost>,
    script: ScriptContext<'_>,
    done_tx: oneshot::Sender<ScriptOutput>,
    timers: &TimerRegistry,
    execution: &execution::Execution,
) -> rquickjs::Result<()> {
    let ScriptContext {
        dialect,
        kind,
        metadata,
    } = script;
    let script_name = metadata.name.as_str();
    // 公共 API（所有方言都有）。
    misc::install(ctx, done_tx, metadata, kind, timers, dialect, execution)?;

    match dialect {
        ScriptDialect::QuantumultX => {
            task::install(ctx, host, execution)?;
            store::install_qx(ctx, host, script_name, execution)?;
            notify::install_qx(ctx, host, execution)?;
        }
        ScriptDialect::Surge => {
            http_client::install(ctx, host, dialect, execution)?;
            store::install_surge(ctx, host, &metadata.source_id, execution)?;
            notify::install_surge(ctx, host, execution)?;
        }
        ScriptDialect::Loon => {
            http_client::install(ctx, host, dialect, execution)?;
            store::install_loon(ctx, host, script_name, execution)?;
            notify::install_surge(ctx, host, execution)?;
            ctx.globals().set(
                "$loon",
                concat!("Lanpath GPUI compatibility ", env!("CARGO_PKG_VERSION")),
            )?;
        }
        ScriptDialect::Stash => {
            http_client::install(ctx, host, dialect, execution)?;
            store::install_stash(ctx, host, script_name, execution)?;
            notify::install_surge(ctx, host, execution)?;
        }
    }
    Ok(())
}

/// 按脚本类型注入 `$request` / `$response` 全局（http-request / http-response 脚本）。
pub(crate) fn inject_script_arg<'js>(
    ctx: &Ctx<'js>,
    kind: ScriptKind,
    arg: Option<&serde_json::Value>,
) -> rquickjs::Result<()> {
    let Some(arg) = arg else {
        return Ok(());
    };
    if !matches!(kind, ScriptKind::HttpRequest | ScriptKind::HttpResponse) {
        return Ok(());
    }
    // Hook envelopes carry the original request alongside the response.
    if let Some(object) = arg
        .as_object()
        .filter(|object| object.contains_key("$request") || object.contains_key("$response"))
    {
        for name in ["$request", "$response"] {
            if let Some(value) = object.get(name) {
                ctx.globals().set(name, context_value(ctx, value)?)?;
            }
        }
    } else {
        let name = if kind == ScriptKind::HttpRequest {
            "$request"
        } else {
            "$response"
        };
        ctx.globals().set(name, context_value(ctx, arg)?)?;
    }
    Ok(())
}

fn context_value<'js>(ctx: &Ctx<'js>, value: &serde_json::Value) -> rquickjs::Result<Value<'js>> {
    let mut public = value.clone();
    let bytes = public
        .as_object_mut()
        .and_then(|object| object.remove("__bodyBytes"));
    let binary = public
        .as_object_mut()
        .and_then(|object| object.remove("__binary"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let result = json_to_js(ctx, &public)?;
    if let Some(bytes) = bytes {
        let bytes: Vec<u8> = serde_json::from_value(bytes).map_err(|error| {
            rquickjs::Error::new_from_js_message("body", "bytes", error.to_string())
        })?;
        if let Some(object) = result.as_object() {
            object.set("bodyBytes", body::binary(ctx, bytes.clone(), true)?)?;
            if binary {
                object.set("body", body::binary(ctx, bytes, false)?)?;
            }
        }
    }
    Ok(result)
}

/// serde_json::Value → JS 值（递归）。
pub(crate) fn json_to_js<'js>(
    ctx: &Ctx<'js>,
    v: &serde_json::Value,
) -> rquickjs::Result<Value<'js>> {
    match v {
        serde_json::Value::Null => Ok(Value::new_null(ctx.clone())),
        serde_json::Value::Bool(b) => Ok(Value::new_bool(ctx.clone(), *b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                if let Ok(i) = i32::try_from(i) {
                    Ok(Value::new_int(ctx.clone(), i))
                } else {
                    Ok(Value::new_float(
                        ctx.clone(),
                        n.as_f64().unwrap_or_default(),
                    ))
                }
            } else {
                Ok(Value::new_float(
                    ctx.clone(),
                    n.as_f64().unwrap_or_default(),
                ))
            }
        }
        serde_json::Value::String(s) => {
            let s = rquickjs::String::from_str(ctx.clone(), s)?;
            Ok(s.into_value())
        }
        serde_json::Value::Array(items) => {
            let arr = rquickjs::Array::new(ctx.clone())?;
            for (i, item) in items.iter().enumerate() {
                let v = json_to_js(ctx, item)?;
                arr.set(i, v)?;
            }
            Ok(arr.into_value())
        }
        serde_json::Value::Object(map) => {
            let obj = Object::new(ctx.clone())?;
            for (k, item) in map {
                let v = json_to_js(ctx, item)?;
                obj.set(k.as_str(), v)?;
            }
            Ok(obj.into_value())
        }
    }
}

/// JS 值 → serde_json::Value（递归）。
#[allow(clippy::only_used_in_recursion)]
pub(crate) fn js_to_json<'js>(
    ctx: &Ctx<'js>,
    v: Value<'js>,
) -> rquickjs::Result<serde_json::Value> {
    if v.is_undefined() || v.is_null() {
        return Ok(serde_json::Value::Null);
    }
    if let Some(b) = v.as_bool() {
        return Ok(serde_json::Value::Bool(b));
    }
    if let Some(n) = v.as_number() {
        // as_number() 返回 f64。
        if n.trunc() == n && n.is_finite() {
            if n >= 0.0 {
                return Ok(serde_json::Value::Number(serde_json::Number::from(
                    n as u64,
                )));
            }
            return Ok(serde_json::Value::Number(serde_json::Number::from(
                n as i64,
            )));
        }
        return Ok(serde_json::Value::Number(
            serde_json::Number::from_f64(n).unwrap_or(serde_json::Number::from(0)),
        ));
    }
    if let Some(s) = v.as_string() {
        return Ok(serde_json::Value::String(s.to_string()?));
    }
    if let Some(bytes) = body::bytes(&v)? {
        return Ok(serde_json::Value::Array(
            bytes
                .into_iter()
                .map(|byte| serde_json::json!(byte))
                .collect(),
        ));
    }
    if let Some(arr) = v.as_array() {
        let mut out = Vec::new();
        for item in arr.iter() {
            let item = item?;
            out.push(js_to_json(ctx, item)?);
        }
        return Ok(serde_json::Value::Array(out));
    }
    if let Some(obj) = v.as_object() {
        let mut map = serde_json::Map::new();
        for entry in obj.props::<rquickjs::Atom, Value>() {
            let (key, val) = entry?;
            let key = key.to_string()?;
            map.insert(key, js_to_json(ctx, val)?);
        }
        return Ok(serde_json::Value::Object(map));
    }
    // 函数等其他类型：无法序列化，视为 null。
    Ok(serde_json::Value::Null)
}

/// JS 值 → 字符串（用于错误消息与日志）。优先取 string / Error 的 message。
pub(crate) fn value_to_string<'js>(ctx: &Ctx<'js>, v: Value<'js>) -> String {
    if let Some(s) = v.as_string()
        && let Ok(s) = s.to_string()
    {
        return s;
    }
    if let Some(obj) = v.as_object() {
        if let Ok(msg) = obj.get::<_, String>("message") {
            return msg;
        }
        if let Ok(v2) = obj.get::<_, Value>("stack")
            && let Some(s) = v2.as_string()
            && let Ok(s) = s.to_string()
        {
            return s;
        }
    }
    let _ = ctx;
    format!("{:?}", v.type_of())
}
