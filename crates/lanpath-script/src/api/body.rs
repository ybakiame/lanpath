use rquickjs::{ArrayBuffer, Ctx, TypedArray, Value};

pub(crate) fn bytes(value: &Value<'_>) -> rquickjs::Result<Option<Vec<u8>>> {
    if let Some(array) = value.as_object().and_then(|o| o.as_typed_array::<u8>()) {
        return array.as_bytes().map(|b| Some(b.to_vec())).ok_or_else(|| {
            rquickjs::Error::new_from_js_message("Uint8Array", "body", "detached buffer")
        });
    }
    if let Some(buffer) = ArrayBuffer::from_value(value.clone()) {
        return buffer.as_bytes().map(|b| Some(b.to_vec())).ok_or_else(|| {
            rquickjs::Error::new_from_js_message("ArrayBuffer", "body", "detached buffer")
        });
    }
    Ok(None)
}

pub(crate) fn binary<'js>(
    ctx: &Ctx<'js>,
    bytes: Vec<u8>,
    array_buffer: bool,
) -> rquickjs::Result<Value<'js>> {
    if array_buffer {
        Ok(ArrayBuffer::new(ctx.clone(), bytes)?.into_value())
    } else {
        Ok(TypedArray::<u8>::new(ctx.clone(), bytes)?.into_value())
    }
}

pub(crate) fn text<'js>(ctx: &Ctx<'js>, bytes: Vec<u8>) -> rquickjs::Result<Value<'js>> {
    let body = String::from_utf8(bytes).map_err(|_| {
        rquickjs::Error::new_from_js_message(
            "body",
            "UTF-8",
            "response body is not UTF-8; enable binary-mode or use bodyBytes",
        )
    })?;
    Ok(rquickjs::String::from_str(ctx.clone(), &body)?.into_value())
}
