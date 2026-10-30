use base64::{Engine as _, engine::general_purpose::STANDARD};
use rquickjs::{Array, Ctx, Object, Value};

use crate::types::{HttpAlpn, HttpRequestSpec, ScriptDialect};

pub(crate) struct Options {
    pub request: HttpRequestSpec,
    pub binary: bool,
    pub full_headers: bool,
}

fn unsupported(name: &str) -> rquickjs::Error {
    rquickjs::Error::new_from_js_message(
        "options",
        "HTTP request",
        format!("unsupported HTTP option: {name}"),
    )
}

pub(crate) fn parse<'js>(
    ctx: &Ctx<'js>,
    value: &Value<'js>,
    dialect: ScriptDialect,
) -> rquickjs::Result<Options> {
    let mut options = Options {
        request: HttpRequestSpec {
            method: "GET".into(),
            timeout_ms: Some(if dialect == ScriptDialect::QuantumultX {
                10_000
            } else {
                5_000
            }),
            auto_cookie: dialect != ScriptDialect::QuantumultX,
            insecure: dialect == ScriptDialect::Loon,
            alpn: if dialect == ScriptDialect::Loon {
                HttpAlpn::Http1
            } else {
                HttpAlpn::Auto
            },
            ..Default::default()
        },
        binary: false,
        full_headers: false,
    };
    if let Some(url) = value.as_string() {
        options.request.url = url.to_string()?;
        return Ok(options);
    }
    let object = value
        .as_object()
        .ok_or_else(|| rquickjs::Error::new_from_js("options", "object"))?;
    for key in object.keys::<String>() {
        let key = key?;
        match key.as_str() {
            "url" | "method" | "headers" | "body" | "bodyBytes" => {}
            "timeout" if dialect != ScriptDialect::QuantumultX => {}
            "opts" if dialect == ScriptDialect::QuantumultX => {}
            "binary-mode" | "full-header-mode" if dialect != ScriptDialect::QuantumultX => {}
            "insecure" | "auto-redirect" | "auto-cookie"
                if dialect != ScriptDialect::QuantumultX => {}
            "body-base64" | "alpn" if dialect == ScriptDialect::Loon => {}
            _ => return Err(unsupported(&key)),
        }
    }
    options.request.url = object.get("url")?;
    if options.request.url.is_empty() {
        return Err(rquickjs::Error::new_from_js_message(
            "url",
            "HTTP URL",
            "url is required",
        ));
    }
    if let Some(method) = object.get::<_, Option<String>>("method")? {
        options.request.method = method;
    }
    if let Some(timeout) = object.get::<_, Option<f64>>("timeout")? {
        let ms = if dialect == ScriptDialect::Loon {
            timeout
        } else {
            timeout * 1000.0
        };
        if !ms.is_finite() || ms <= 0.0 || ms > u64::MAX as f64 {
            return Err(rquickjs::Error::new_from_js_message(
                "timeout",
                "duration",
                "timeout must be finite and positive",
            ));
        }
        options.request.timeout_ms = Some(ms.ceil() as u64);
    }
    if let Some(headers) = object.get::<_, Option<Value>>("headers")? {
        options.request.headers = parse_headers(&headers)?;
    }
    // These headers are vendor routing controls, never ordinary outbound fields.
    if options.request.headers.iter().any(|(name, _)| {
        name.eq_ignore_ascii_case("x-stash-selected-proxy")
            || name.eq_ignore_ascii_case("x-surge-policy")
    }) {
        return Err(unsupported("routing policy header"));
    }
    options.binary = object
        .get::<_, Option<bool>>("binary-mode")?
        .unwrap_or(false);
    options.full_headers = object
        .get::<_, Option<bool>>("full-header-mode")?
        .unwrap_or(false);
    options.request.insecure = object
        .get::<_, Option<bool>>("insecure")?
        .unwrap_or(options.request.insecure);
    options.request.auto_redirect = object
        .get::<_, Option<bool>>("auto-redirect")?
        .unwrap_or(true);
    options.request.auto_cookie = object
        .get::<_, Option<bool>>("auto-cookie")?
        .unwrap_or(options.request.auto_cookie);
    if let Some(nested) = object.get::<_, Option<Object>>("opts")? {
        for key in nested.keys::<String>() {
            let key = key?;
            match key.as_str() {
                "redirection" => options.request.auto_redirect = nested.get(&key)?,
                "skip-cert-verify" => options.request.insecure = nested.get(&key)?,
                "auto-cookie" => options.request.auto_cookie = nested.get(&key)?,
                _ => return Err(unsupported(&key)),
            }
        }
    }
    if dialect == ScriptDialect::Loon {
        let alpn: Value = object.get("alpn")?;
        if !alpn.is_undefined() {
            options.request.alpn = match alpn
                .as_string()
                .map(|value| value.to_string())
                .transpose()?
                .as_deref()
            {
                Some("h1") => HttpAlpn::Http1,
                Some("h2") => HttpAlpn::Http2,
                _ => {
                    return Err(rquickjs::Error::new_from_js_message(
                        "alpn",
                        "h1 or h2",
                        "alpn must be h1 or h2",
                    ));
                }
            };
        }
    }
    let body: Value = object.get("body")?;
    let body_bytes: Value = object.get("bodyBytes")?;
    let base64: Value = object.get("body-base64")?;
    let decode_base64 = if base64.is_undefined() {
        false
    } else {
        base64
            .as_bool()
            .ok_or_else(|| rquickjs::Error::new_from_js("body-base64", "boolean"))?
    };
    if !body.is_undefined() && !body_bytes.is_undefined() {
        return Err(rquickjs::Error::new_from_js_message(
            "body",
            "HTTP request",
            "body and bodyBytes are mutually exclusive",
        ));
    }
    if decode_base64 {
        if !body_bytes.is_undefined() {
            return Err(rquickjs::Error::new_from_js_message(
                "body-base64",
                "HTTP request",
                "body-base64 and bodyBytes are mutually exclusive",
            ));
        }
        let text = body
            .as_string()
            .ok_or_else(|| rquickjs::Error::new_from_js("body", "Base64 string"))?
            .to_string()?;
        options.request.body = Some(STANDARD.decode(text).map_err(|_| {
            rquickjs::Error::new_from_js_message(
                "body",
                "Base64",
                "body is not valid standard padded Base64",
            )
        })?);
    } else if !body_bytes.is_undefined() {
        options.request.body = Some(super::body::bytes(&body_bytes)?.ok_or_else(|| {
            rquickjs::Error::new_from_js("bodyBytes", "Uint8Array or ArrayBuffer")
        })?);
    } else if !body.is_undefined() && !body.is_null() {
        options.request.body =
            if let Some(bytes) = super::body::bytes(&body)? {
                Some(bytes)
            } else if let Some(text) = body.as_string() {
                Some(text.to_string()?.into_bytes())
            } else if body.is_object()
                && dialect != ScriptDialect::QuantumultX
                && dialect != ScriptDialect::Loon
            {
                let json = super::js_to_json(ctx, body)?;
                if !options
                    .request
                    .headers
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                {
                    options
                        .request
                        .headers
                        .push(("Content-Type".into(), "application/json".into()));
                }
                Some(serde_json::to_vec(&json).map_err(|e| {
                    rquickjs::Error::new_from_js_message("body", "JSON", e.to_string())
                })?)
            } else {
                return Err(rquickjs::Error::new_from_js(
                    "body",
                    "string or binary body",
                ));
            };
    }
    Ok(options)
}

pub(crate) fn parse_headers(value: &Value<'_>) -> rquickjs::Result<Vec<(String, String)>> {
    let mut headers = Vec::new();
    if let Some(array) = value.as_array() {
        for item in array.iter::<Object>() {
            let item = item?;
            headers.push((item.get("field")?, item.get("value")?));
        }
    } else if let Some(object) = value.as_object() {
        for entry in object.props::<String, Value>() {
            let (name, value) = entry?;
            if let Some(values) = value.as_array() {
                for value in values.iter::<String>() {
                    headers.push((name.clone(), value?));
                }
            } else if let Some(value) = value.as_string() {
                headers.push((name, value.to_string()?));
            } else {
                return Err(rquickjs::Error::new_from_js(
                    "header",
                    "string or string array",
                ));
            }
        }
    } else {
        return Err(rquickjs::Error::new_from_js(
            "headers",
            "object or full-header array",
        ));
    }
    Ok(headers)
}

pub(crate) fn headers<'js>(
    ctx: &Ctx<'js>,
    fields: &[(String, String)],
    full: bool,
) -> rquickjs::Result<Value<'js>> {
    if full {
        let array = Array::new(ctx.clone())?;
        for (index, (name, value)) in fields.iter().enumerate() {
            let field = Object::new(ctx.clone())?;
            field.set("field", name.as_str())?;
            field.set("value", value.as_str())?;
            array.set(index, field)?;
        }
        return Ok(array.into_value());
    }
    let mut grouped: Vec<(&str, Vec<&str>)> = Vec::new();
    for (name, value) in fields {
        if let Some((_, values)) = grouped
            .iter_mut()
            .find(|(existing, _)| existing.eq_ignore_ascii_case(name))
        {
            values.push(value);
        } else {
            grouped.push((name, vec![value]));
        }
    }
    let object = Object::new(ctx.clone())?;
    for (name, values) in grouped {
        if values.len() == 1 {
            object.set(name, values[0])?;
        } else {
            let array = Array::new(ctx.clone())?;
            for (index, value) in values.iter().enumerate() {
                array.set(index, *value)?;
            }
            object.set(name, array)?;
        }
    }
    Ok(object.into_value())
}
