//! Per-source script hooks. Traffic bodies remain bytes until a selected text API boundary.
use crate::{Pattern, PatternError};
use std::sync::Arc;

use lanpath_script::{
    ScriptCompletion, ScriptDialect, ScriptExecutionMetadata, ScriptHost, ScriptKind, ScriptLimits,
    ScriptOutput, ScriptWorker,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

/// Explicit local memory envelope, including Surge's documented request hard cap.
pub const SURGE_BODY_ENVELOPE: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HookMode {
    #[default]
    Modify,
    Echo,
}

pub struct ScriptRule {
    pub name: String,
    pub kind: ScriptKind,
    pub pattern: Pattern,
    pub requires_body: bool,
    pub max_size: usize,
    pub source: String,
    pub argument: Option<String>,
    pub dialect: ScriptDialect,
    pub mode: HookMode,
    pub enabled: bool,
    pub timeout_ms: Option<u64>,
    pub binary: bool,
    pub full_headers: bool,
    /// Stable source identity, not the display name or rule's ordinal position.
    pub source_id: String,
    pub module_id: Option<String>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum HookOutcome {
    #[default]
    Continue,
    Mock(MockResponse),
    Abort,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// The outgoing request, retained for response scripts rather than rebuilt from response fields.
#[derive(Debug, Clone, Default)]
pub struct RequestContext {
    pub id: Uuid,
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

pub struct ScriptHookEngine {
    worker: ScriptWorker,
    rules: Vec<ScriptRule>,
}

impl ScriptHookEngine {
    pub fn new(host: Arc<ScriptHost>, limits: ScriptLimits, rules: Vec<ScriptRule>) -> Self {
        Self {
            worker: ScriptWorker::new(host, limits),
            rules,
        }
    }

    fn selected(&self, url: &str, kind: ScriptKind) -> Result<Option<&ScriptRule>, PatternError> {
        for rule in &self.rules {
            if rule.enabled && rule.kind == kind && rule.pattern.is_match(url)? {
                return Ok(Some(rule));
            }
        }
        Ok(None)
    }

    pub fn request_body_limit(&self, url: &str) -> Result<Option<usize>, PatternError> {
        self.transport_body_limit(url, ScriptKind::HttpRequest, usize::MAX)
    }

    pub fn response_body_limit(&self, url: &str) -> Result<Option<usize>, PatternError> {
        self.transport_body_limit(url, ScriptKind::HttpResponse, usize::MAX)
    }

    /// Original request-body context is bounded by the selected response hook's own limit.
    pub fn request_context_body_limit(&self, url: &str) -> Result<Option<usize>, PatternError> {
        self.response_body_limit(url)
    }

    pub(crate) fn transport_body_limit(
        &self,
        url: &str,
        kind: ScriptKind,
        other_dialect_cap: usize,
    ) -> Result<Option<usize>, PatternError> {
        Ok(self
            .selected(url, kind)?
            .filter(|rule| rule.requires_body)
            .map(|rule| {
                if rule.dialect == ScriptDialect::Surge {
                    rule.max_size.min(SURGE_BODY_ENVELOPE)
                } else {
                    rule.max_size.min(other_dialect_cap)
                }
            }))
    }

    pub fn rejects_request_body_overflow(&self, url: &str) -> Result<bool, PatternError> {
        Ok(self
            .selected(url, ScriptKind::HttpRequest)?
            .is_some_and(|rule| rule.requires_body && rule.dialect == ScriptDialect::Surge))
    }

    pub async fn run_request_hooks(
        &self,
        request_id: Uuid,
        url: &mut String,
        method: &str,
        headers: &mut Vec<(String, String)>,
        body: &mut Option<Vec<u8>>,
    ) -> HookOutcome {
        let rule = match self.selected(url, ScriptKind::HttpRequest) {
            Ok(Some(rule)) => rule,
            Ok(None) => return HookOutcome::Continue,
            Err(error) => {
                tracing::warn!(%error, "request script regex rejected traffic");
                return HookOutcome::Abort;
            }
        };
        if rule.requires_body
            && rule.dialect == ScriptDialect::Surge
            && body
                .as_ref()
                .is_some_and(|bytes| bytes.len() > rule.max_size.min(SURGE_BODY_ENVELOPE))
        {
            return HookOutcome::Abort;
        }
        if rule.requires_body && body.is_none() {
            return if rule.dialect == ScriptDialect::Surge {
                HookOutcome::Abort
            } else {
                HookOutcome::Continue
            };
        }
        let Some(request) = request_value(request_id, url, method, headers, body.as_deref(), rule)
        else {
            return HookOutcome::Continue;
        };
        match self.run_one(rule, json!({"$request":request})).await {
            Some(out) => apply_output(&out, rule, Some(url), None, headers, body),
            None => HookOutcome::Continue,
        }
    }

    pub async fn run_response_hooks(
        &self,
        request: &RequestContext,
        status: &mut u16,
        headers: &mut Vec<(String, String)>,
        body: &mut Option<Vec<u8>>,
    ) -> HookOutcome {
        let rule = match self.selected(&request.url, ScriptKind::HttpResponse) {
            Ok(Some(rule)) => rule,
            Ok(None) => return HookOutcome::Continue,
            Err(error) => {
                tracing::warn!(%error, "response script regex rejected traffic");
                return HookOutcome::Abort;
            }
        };
        let mut response =
            json!({"status":*status,"statusCode":*status,"headers":headers_value(headers, false)});
        if !inject_body(&mut response, body.as_deref(), rule, true) {
            return HookOutcome::Continue;
        }
        if rule.full_headers {
            response["headers"] = headers_value(headers, true);
        }
        response["fullHeaders"] = headers_value(headers, true);
        // The response's body mode does not require an original request to have a body.
        let Some(mut request_arg) = request_value(
            request.id,
            &request.url,
            &request.method,
            &request.headers,
            None,
            rule,
        ) else {
            return HookOutcome::Continue;
        };
        if let Some(bytes) = request
            .body
            .as_deref()
            .filter(|bytes| bytes.len() <= rule.max_size)
        {
            inject_body(&mut request_arg, Some(bytes), rule, false);
        }
        match self
            .run_one(rule, json!({"$request":request_arg,"$response":response}))
            .await
        {
            Some(out) => apply_output(&out, rule, None, Some(status), headers, body),
            None => HookOutcome::Continue,
        }
    }

    async fn run_one(&self, rule: &ScriptRule, arg: Value) -> Option<ScriptOutput> {
        let identity = if rule.source_id.is_empty() {
            &rule.name
        } else {
            &rule.source_id
        };
        match self
            .worker
            .run_script_with_timeout(
                &rule.source,
                rule.kind,
                Some(arg),
                rule.argument.as_deref(),
                rule.dialect,
                ScriptExecutionMetadata {
                    name: rule.name.clone(),
                    source_id: identity.clone(),
                    module_id: rule.module_id.clone(),
                    binary_body_mode: rule.binary,
                },
                rule.timeout_ms,
            )
            .await
        {
            Ok(output) => Some(output),
            Err(error) => {
                tracing::warn!(script = %rule.name, "hook script failed: {error}");
                None
            }
        }
    }
}

fn request_value(
    request_id: Uuid,
    url: &str,
    method: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
    rule: &ScriptRule,
) -> Option<Value> {
    let mut value = json!({"id":request_id,"url":url,"method":method,"path":url_path(url),"headers":headers_value(headers,rule.full_headers),"fullHeaders":headers_value(headers,true)});
    if !inject_body(&mut value, body, rule, body.is_some()) {
        return None;
    }
    Some(value)
}

fn inject_body(value: &mut Value, body: Option<&[u8]>, rule: &ScriptRule, required: bool) -> bool {
    if !rule.requires_body {
        return true;
    }
    let Some(bytes) = body else { return !required };
    if bytes.len() > rule.max_size {
        return false;
    }
    value["__bodyBytes"] = json!(bytes);
    value["__binary"] = json!(rule.binary);
    if !rule.binary {
        let Ok(text) = std::str::from_utf8(bytes) else {
            return false;
        };
        value["body"] = json!(text);
    }
    true
}

fn headers_value(headers: &[(String, String)], full: bool) -> Value {
    if full {
        return json!(
            headers
                .iter()
                .map(|(field, value)| json!({"field":field,"value":value}))
                .collect::<Vec<_>>()
        );
    }
    let mut map = Map::new();
    for (name, value) in headers {
        let key = map
            .keys()
            .find(|key| key.eq_ignore_ascii_case(name))
            .cloned()
            .unwrap_or_else(|| name.clone());
        match map.get_mut(&key) {
            Some(Value::Array(values)) => values.push(json!(value)),
            Some(existing) => *existing = json!([existing.clone(), value]),
            None => {
                map.insert(key, if full { json!([value]) } else { json!(value) });
            }
        }
    }
    Value::Object(map)
}

fn url_path(url: &str) -> String {
    let authority = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    match authority.find(['/', '?']) {
        Some(index) if authority.as_bytes()[index] == b'?' => format!("/{}", &authority[index..]),
        Some(index) => authority[index..].to_string(),
        None => "/".to_string(),
    }
}

fn apply_output(
    output: &ScriptOutput,
    rule: &ScriptRule,
    url: Option<&mut String>,
    status: Option<&mut u16>,
    headers: &mut Vec<(String, String)>,
    body: &mut Option<Vec<u8>>,
) -> HookOutcome {
    let out = &output.0;
    if output.1 == ScriptCompletion::Undefined || out.is_null() {
        return if matches!(rule.dialect, ScriptDialect::Loon | ScriptDialect::Stash) {
            HookOutcome::Abort
        } else {
            HookOutcome::Continue
        };
    }
    if let Some(text) = out.as_str() {
        if rule.requires_body {
            *body = Some(text.as_bytes().to_vec());
        }
        return HookOutcome::Continue;
    }
    let Some(object) = out.as_object() else {
        return HookOutcome::Continue;
    };
    if let Some(option) = object.keys().find(|key| {
        !matches!(
            key.as_str(),
            "url"
                | "status"
                | "statusCode"
                | "headers"
                | "body"
                | "bodyBytes"
                | "response"
                | "abort"
        )
    }) {
        tracing::warn!(script = %rule.name, %option, "unsupported script completion option; mutation rejected");
        return HookOutcome::Continue;
    }
    if object.get("abort").and_then(Value::as_bool) == Some(true) {
        return HookOutcome::Abort;
    }
    if rule.mode == HookMode::Echo || object.contains_key("response") {
        let response = if rule.mode == HookMode::Echo {
            object
        } else {
            let Some(response) = object.get("response").and_then(Value::as_object) else {
                return HookOutcome::Continue;
            };
            response
        };
        return match mock_response(response) {
            Some(response) => HookOutcome::Mock(response),
            None => {
                tracing::warn!(script = %rule.name, "invalid script response ignored");
                HookOutcome::Continue
            }
        };
    }
    // Validate the whole mutation before committing any field.
    if let Some(value) = object.get("url").filter(|_| url.is_some())
        && !value.as_str().is_some_and(|value| {
            value.parse::<http::Uri>().is_ok_and(|uri| {
                matches!(uri.scheme_str(), Some("http" | "https")) && uri.authority().is_some()
            })
        })
    {
        return HookOutcome::Continue;
    }
    let new_headers = match object.get("headers") {
        Some(value) => match parse_headers(value) {
            Some(headers) => Some(headers),
            None => return HookOutcome::Continue,
        },
        None => None,
    };
    let new_body = if rule.requires_body {
        match output_body(object) {
            Ok(body) => body,
            Err(()) => return HookOutcome::Continue,
        }
    } else {
        None
    };
    let new_status = match object.get("status").or_else(|| object.get("statusCode")) {
        Some(value) => match parse_status(value) {
            Some(status) => Some(status),
            None => return HookOutcome::Continue,
        },
        None => None,
    };
    if let (Some(target), Some(value)) = (url, object.get("url").and_then(Value::as_str)) {
        *target = value.to_owned();
    }
    if let (Some(target), Some(value)) = (status, new_status) {
        *target = value;
    }
    if let Some(value) = new_headers {
        *headers = value;
    }
    if let Some(value) = new_body {
        *body = Some(value);
    }
    HookOutcome::Continue
}

fn parse_status(value: &Value) -> Option<u16> {
    let code = if let Some(code) = value.as_u64() {
        u16::try_from(code).ok()?
    } else {
        let text = value.as_str()?;
        text.split_whitespace()
            .find_map(|part| part.parse::<u16>().ok())?
    };
    (100..=599).contains(&code).then_some(code)
}

fn parse_headers(value: &Value) -> Option<Vec<(String, String)>> {
    let mut headers = Vec::new();
    if let Some(fields) = value.as_array() {
        for field in fields {
            headers.push((
                field.get("field")?.as_str()?.to_owned(),
                field.get("value")?.as_str()?.to_owned(),
            ));
        }
        return Some(headers);
    }
    for (name, value) in value.as_object()? {
        match value {
            Value::String(value) => headers.push((name.clone(), value.clone())),
            Value::Array(values) => {
                for value in values {
                    headers.push((name.clone(), value.as_str()?.to_string()));
                }
            }
            _ => return None,
        }
    }
    Some(headers)
}

fn output_body(object: &Map<String, Value>) -> Result<Option<Vec<u8>>, ()> {
    let Some(value) = object.get("bodyBytes").or_else(|| object.get("body")) else {
        return Ok(None);
    };
    if let Some(text) = value.as_str() {
        return Ok(Some(text.as_bytes().to_vec()));
    }
    let Some(values) = value.as_array() else {
        return Err(());
    };
    values
        .iter()
        .map(|value| value.as_u64().and_then(|n| u8::try_from(n).ok()).ok_or(()))
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn mock_response(object: &Map<String, Value>) -> Option<MockResponse> {
    let status = match object.get("status").or_else(|| object.get("statusCode")) {
        Some(status) => parse_status(status)?,
        None => 200,
    };
    let headers = match object.get("headers") {
        Some(headers) => parse_headers(headers)?,
        None => Vec::new(),
    };
    Some(MockResponse {
        status,
        headers,
        body: output_body(object).ok()?,
    })
}
