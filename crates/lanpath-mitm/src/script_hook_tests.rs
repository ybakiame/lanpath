//! Consumer-visible hook contracts; fixtures are self-authored from public API references.
use super::script_hook::*;
use crate::Pattern;
use lanpath_script::{
    MemoryPersistentStore, MockHttpExecutor, RecordingNotifier, ScriptDialect, ScriptHost,
    ScriptKind, ScriptLimits,
};
use std::sync::Arc;

fn rule(name: &str, kind: ScriptKind, source: &str) -> ScriptRule {
    ScriptRule {
        name: name.into(),
        kind,
        pattern: Pattern::new(r"^https://api\.example\.com/").unwrap(),
        requires_body: false,
        max_size: 65536,
        source: source.into(),
        argument: None,
        dialect: ScriptDialect::Surge,
        mode: HookMode::Modify,
        enabled: true,
        timeout_ms: None,
        binary: false,
        full_headers: false,
        source_id: format!("https://scripts.example/{name}.js"),
        module_id: None,
    }
}

fn engine(rules: Vec<ScriptRule>) -> ScriptHookEngine {
    let host = Arc::new(ScriptHost::new(
        Arc::new(MockHttpExecutor::with_responses(vec![])),
        Arc::new(MemoryPersistentStore::new()),
        Arc::new(RecordingNotifier::new()),
    ));
    ScriptHookEngine::new(host, ScriptLimits::default(), rules)
}

fn request() -> RequestContext {
    RequestContext {
        id: uuid::Uuid::new_v4(),
        url: "https://api.example.com/v1/data?x=1".into(),
        method: "POST".into(),
        headers: vec![("X-Request".into(), "retained".into())],
        body: Some(b"request payload".to_vec()),
    }
}

#[tokio::test]
async fn response_sees_original_request_and_rewrites_json() {
    let mut rule = rule(
        "response",
        ScriptKind::HttpResponse,
        r#"
        const data = JSON.parse($response.body);
        data.request = [$request.url, $request.method, $request.headers['X-Request'], $request.body];
        data.a = 99; $done({body: JSON.stringify(data)});
    "#,
    );
    rule.requires_body = true;
    let engine = engine(vec![rule]);
    let mut status = 200;
    let mut headers = vec![("content-type".into(), "application/json".into())];
    let mut body = Some(br#"{"a":1,"b":2}"#.to_vec());
    assert_eq!(
        engine
            .run_response_hooks(&request(), &mut status, &mut headers, &mut body)
            .await,
        HookOutcome::Continue
    );
    let result: serde_json::Value = serde_json::from_slice(body.as_deref().unwrap()).unwrap();
    assert_eq!(result["a"], 99);
    assert_eq!(result["b"], 2);
    assert_eq!(
        result["request"],
        serde_json::json!([request().url, "POST", "retained", "request payload"])
    );
}

#[tokio::test]
async fn first_enabled_match_uses_its_own_dialect_and_argument() {
    let mut disabled = rule(
        "disabled",
        ScriptKind::HttpRequest,
        "$done({url:'https://wrong.example/'})",
    );
    disabled.enabled = false;
    let mut selected = rule(
        "qx",
        ScriptKind::HttpRequest,
        r#"
        if (typeof $task === 'undefined' || typeof $httpClient !== 'undefined') throw Error('wrong dialect');
        $done({url:'https://api.example.com/' + $argument});
    "#,
    );
    selected.dialect = ScriptDialect::QuantumultX;
    selected.argument = Some("resource-local".into());
    let mut later = rule(
        "later",
        ScriptKind::HttpRequest,
        "$done({url:'https://wrong.example/'})",
    );
    later.requires_body = true;
    let engine = engine(vec![disabled, selected, later]);
    let mut url = request().url;
    let mut headers = vec![];
    let mut body = None;
    assert_eq!(engine.request_body_limit(&url).unwrap(), None);
    engine
        .run_request_hooks(
            uuid::Uuid::new_v4(),
            &mut url,
            "GET",
            &mut headers,
            &mut body,
        )
        .await;
    assert_eq!(url, "https://api.example.com/resource-local");
}

#[tokio::test]
async fn invalid_request_url_rejects_the_whole_completion_in_supported_dialects() {
    for dialect in [ScriptDialect::Surge, ScriptDialect::QuantumultX] {
        for target in [
            serde_json::json!("/relative"),
            serde_json::json!("https://[invalid/"),
            serde_json::json!("ftp://api.example.com/file"),
            serde_json::json!(42),
            serde_json::json!("https://api.example.com/changed?x=%2F"),
        ] {
            let mut selected = rule(
                "url",
                ScriptKind::HttpRequest,
                "$done({url:JSON.parse($argument),headers:{'X-Changed':'yes'},body:'changed'})",
            );
            selected.dialect = dialect;
            selected.requires_body = true;
            selected.argument = Some(target.to_string());
            let engine = engine(vec![selected]);
            let mut url = request().url;
            let original_url = url.clone();
            let mut headers = request().headers;
            let original_headers = headers.clone();
            let mut body = request().body;
            let original_body = body.clone();
            assert_eq!(
                engine
                    .run_request_hooks(
                        uuid::Uuid::new_v4(),
                        &mut url,
                        "POST",
                        &mut headers,
                        &mut body,
                    )
                    .await,
                HookOutcome::Continue
            );
            if target == "https://api.example.com/changed?x=%2F" {
                assert_eq!(url, target.as_str().unwrap());
                assert_eq!(headers, vec![("X-Changed".into(), "yes".into())]);
                assert_eq!(body.as_deref(), Some(b"changed".as_slice()));
            } else {
                assert_eq!(url, original_url);
                assert_eq!(headers, original_headers);
                assert_eq!(body, original_body);
            }
        }
    }
}

#[tokio::test]
async fn qx_echo_synthesizes_status_headers_and_binary_body() {
    let mut rule = rule(
        "echo",
        ScriptKind::HttpRequest,
        r#"
        $done({status:'HTTP/1.1 307 Temporary Redirect', headers:{Location:'https://target.example/', 'Set-Cookie':['a=1','b=2']}, bodyBytes:new Uint8Array([0,255,128]).buffer});
    "#,
    );
    rule.dialect = ScriptDialect::QuantumultX;
    rule.mode = HookMode::Echo;
    let engine = engine(vec![rule]);
    let outcome = engine
        .run_request_hooks(
            uuid::Uuid::new_v4(),
            &mut request().url,
            "GET",
            &mut vec![],
            &mut None,
        )
        .await;
    let HookOutcome::Mock(response) = outcome else {
        panic!("expected mock: {outcome:?}")
    };
    assert_eq!(response.status, 307);
    assert_eq!(response.body, Some(vec![0, 255, 128]));
    assert_eq!(
        response
            .headers
            .iter()
            .filter(|(name, _)| name == "Set-Cookie")
            .count(),
        2
    );
}

#[tokio::test]
async fn typed_binary_body_and_full_headers_round_trip_duplicates() {
    let mut rule = rule(
        "binary",
        ScriptKind::HttpResponse,
        r#"
        if (!($response.body instanceof Uint8Array) || !($response.bodyBytes instanceof ArrayBuffer)) throw Error('not typed binary');
        if (!Array.isArray($response.headers)) throw Error('not full headers');
        $response.body[1] = 254;
        $done({headers:$response.headers, body:$response.body});
    "#,
    );
    rule.requires_body = true;
    rule.binary = true;
    rule.full_headers = true;
    let engine = engine(vec![rule]);
    let mut headers = vec![
        ("Set-Cookie".into(), "a=1".into()),
        ("set-cookie".into(), "b=2".into()),
    ];
    let original = headers.clone();
    let mut body = Some(vec![0, 255, 128]);
    engine
        .run_response_hooks(&request(), &mut 200, &mut headers, &mut body)
        .await;
    assert_eq!(headers, original);
    assert_eq!(body, Some(vec![0, 254, 128]));
}

#[tokio::test]
async fn ordinary_header_object_preserves_duplicate_cookie_fields() {
    let engine = engine(vec![rule(
        "headers",
        ScriptKind::HttpResponse,
        "$done({headers:$response.headers})",
    )]);
    let mut headers = vec![
        ("Set-Cookie".into(), "a=1".into()),
        ("set-cookie".into(), "b=2".into()),
    ];
    engine
        .run_response_hooks(&request(), &mut 200, &mut headers, &mut None)
        .await;
    assert_eq!(headers.len(), 2);
    assert_eq!(headers[0].1, "a=1");
    assert_eq!(headers[1].1, "b=2");
}

#[tokio::test]
async fn json_looking_string_completion_is_not_parsed_as_mutation() {
    let mut rule = rule(
        "string",
        ScriptKind::HttpResponse,
        r#"$done('{"status":418,"body":"not a mutation"}')"#,
    );
    rule.requires_body = true;
    let engine = engine(vec![rule]);
    let mut status = 200;
    let mut body = Some(b"original".to_vec());
    engine
        .run_response_hooks(&request(), &mut status, &mut vec![], &mut body)
        .await;
    assert_eq!(status, 200);
    assert_eq!(body.unwrap(), br#"{"status":418,"body":"not a mutation"}"#);
}

#[tokio::test]
async fn completion_presence_and_dialect_control_pass_through_or_abort() {
    for dialect in [
        ScriptDialect::Surge,
        ScriptDialect::QuantumultX,
        ScriptDialect::Loon,
        ScriptDialect::Stash,
    ] {
        for source in ["$done()", "$done(null)", "$done({})"] {
            let mut rule = rule("completion", ScriptKind::HttpRequest, source);
            rule.dialect = dialect;
            let engine = engine(vec![rule]);
            let outcome = engine
                .run_request_hooks(
                    uuid::Uuid::new_v4(),
                    &mut request().url,
                    "GET",
                    &mut vec![],
                    &mut None,
                )
                .await;
            let abort = matches!(dialect, ScriptDialect::Loon | ScriptDialect::Stash)
                && source != "$done({})";
            assert_eq!(
                outcome,
                if abort {
                    HookOutcome::Abort
                } else {
                    HookOutcome::Continue
                },
                "{dialect:?} {source}"
            );
        }
    }
}

#[tokio::test]
async fn unavailable_oversized_or_non_utf8_text_body_skips_without_mutation() {
    for body in [None, Some(vec![1; 9]), Some(vec![255])] {
        let mut rule = rule(
            "body",
            ScriptKind::HttpResponse,
            "$done({status:418,body:'changed'})",
        );
        rule.requires_body = true;
        rule.max_size = 8;
        let engine = engine(vec![rule]);
        let mut actual = body.clone();
        let mut status = 200;
        engine
            .run_response_hooks(&request(), &mut status, &mut vec![], &mut actual)
            .await;
        assert_eq!(status, 200);
        assert_eq!(actual, body);
    }
}

#[tokio::test]
async fn header_only_hook_does_not_replace_unavailable_body() {
    let engine = engine(vec![rule(
        "headers-only",
        ScriptKind::HttpResponse,
        "$done({status:201,body:'must not collect/replace'})",
    )]);
    let mut status = 200;
    let mut body = None;
    engine
        .run_response_hooks(&request(), &mut status, &mut vec![], &mut body)
        .await;
    assert_eq!(status, 201);
    assert_eq!(body, None);
}

#[tokio::test]
async fn imported_timeout_fails_open_and_next_run_survives() {
    let mut rule = rule("timeout", ScriptKind::HttpRequest, "while(true){}");
    rule.timeout_ms = Some(40);
    let engine = engine(vec![rule]);
    let started = std::time::Instant::now();
    let mut url = request().url;
    let original = url.clone();
    let outcome = engine
        .run_request_hooks(
            uuid::Uuid::new_v4(),
            &mut url,
            "GET",
            &mut vec![],
            &mut None,
        )
        .await;
    assert_eq!(outcome, HookOutcome::Continue);
    assert_eq!(url, original);
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
}

#[tokio::test]
async fn surge_execution_keeps_declared_names_and_source_scoped_storage_across_modules() {
    let mut writer = rule(
        "request-display",
        ScriptKind::HttpRequest,
        "$persistentStore.write($script.name); $done({})",
    );
    writer.source_id = "https://assets.example/shared.js".into();
    writer.module_id = Some("request-module".into());
    let mut reader = rule(
        "response-display",
        ScriptKind::HttpResponse,
        "$done({headers:{'x-stored':$persistentStore.read(),'x-name':$script.name,'x-binary':String($script.binaryBodyMode)}})",
    );
    reader.source_id = writer.source_id.clone();
    reader.module_id = Some("response-module".into());
    let engine = engine(vec![writer, reader]);
    let request = request();
    assert_eq!(
        engine
            .run_request_hooks(
                request.id,
                &mut request.url.clone(),
                "GET",
                &mut vec![],
                &mut None
            )
            .await,
        HookOutcome::Continue
    );
    let mut headers = vec![];
    assert_eq!(
        engine
            .run_response_hooks(&request, &mut 200, &mut headers, &mut None)
            .await,
        HookOutcome::Continue
    );
    for (name, value) in [
        ("x-stored", "request-display"),
        ("x-name", "response-display"),
        ("x-binary", "false"),
    ] {
        assert!(
            headers
                .iter()
                .any(|(field, actual)| field == name && actual == value)
        );
    }
}

#[tokio::test]
async fn surge_unavailable_or_oversized_required_request_body_is_not_forwardable() {
    let mut surge = rule("body", ScriptKind::HttpRequest, "$done({})");
    surge.requires_body = true;
    surge.max_size = 8;
    let engine = engine(vec![surge]);
    for mut body in [None, Some(vec![b'x'; 9])] {
        assert_eq!(
            engine
                .run_request_hooks(
                    request().id,
                    &mut request().url,
                    "POST",
                    &mut vec![],
                    &mut body
                )
                .await,
            HookOutcome::Abort
        );
    }
}
