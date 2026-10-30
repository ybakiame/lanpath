use super::*;
use crate::host::{MemoryPersistentStore, MockHttpExecutor, PersistentStore, RecordingNotifier};
use crate::types::{HttpResponseData, ScriptDialect, ScriptLimits};

fn test_host() -> (
    Arc<ScriptHost>,
    Arc<MockHttpExecutor>,
    Arc<RecordingNotifier>,
    Arc<MemoryPersistentStore>,
) {
    let http = Arc::new(MockHttpExecutor::with_responses(vec![
        HttpResponseData {
            status: 200,
            headers: vec![("content-type".into(), "application/json".into())],
            body: r#"{"code":0,"token":"tok123"}"#.into(),
        },
        HttpResponseData {
            status: 200,
            headers: vec![],
            body: r#"{"code":0}"#.into(),
        },
    ]));
    let store = Arc::new(MemoryPersistentStore::new());
    let notifier = Arc::new(RecordingNotifier::new());
    let host = Arc::new(ScriptHost::new(
        http.clone(),
        store.clone(),
        notifier.clone(),
    ));
    (host, http, notifier, store)
}

#[tokio::test(flavor = "current_thread")]
async fn qx_signin_style() {
    let (host, _http, notifier, store) = test_host();
    let mut engine = QuickJsEngine::new(
        host,
        ScriptDialect::QuantumultX,
        ScriptLimits::default(),
        "qx_signin".to_string(),
    )
    .unwrap();

    let source = r#"
            (async () => {
                const resp = await $task.fetch({url: "https://example.com/api/signin", method: "POST", body: "u=1"});
                const data = JSON.parse(resp.body);
                $prefs.setValueForKey(data.token, "token");
                $notify("签到成功", "token=" + data.token, resp.body);
                $done(JSON.stringify({code: data.code, token: data.token}));
            })();
        "#;
    let out = engine
        .run_script(source, ScriptKind::Generic, None, None)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(out.0.as_str().unwrap()).unwrap();
    assert_eq!(v["code"], 0);
    assert_eq!(v["token"], "tok123");
    assert_eq!(
        store.read("shared", "token").unwrap(),
        Some("tok123".to_string())
    );
    // notifier 记录 1 条
    let calls = notifier.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].title, "签到成功");
}

#[tokio::test(flavor = "current_thread")]
async fn surge_http_response_rewrite() {
    let (host, _http, _notifier, _store) = test_host();
    let mut engine = QuickJsEngine::new(
        host,
        ScriptDialect::Surge,
        ScriptLimits::default(),
        "rewrite".to_string(),
    )
    .unwrap();

    let arg = serde_json::json!({
        "status": 200,
        "headers": {"content-type": "application/json"},
        "body": r#"{"a":1,"b":2}"#,
    });
    let source = r#"
            const data = JSON.parse($response.body);
            data.a = 99;
            data.newField = "added";
            $done({body: JSON.stringify(data)});
        "#;
    let out = engine
        .run_script(source, ScriptKind::HttpResponse, Some(arg), None)
        .await
        .unwrap();
    let body = out.0["body"].as_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(parsed["a"], 99);
    assert_eq!(parsed["newField"], "added");
    assert_eq!(parsed["b"], 2);
}

#[tokio::test(flavor = "current_thread")]
async fn loon_documented_callback_apis() {
    let (host, _http, _notifier, store) = test_host();
    let mut engine = QuickJsEngine::new(
        host,
        ScriptDialect::Loon,
        ScriptLimits::default(),
        "loon_test".to_string(),
    )
    .unwrap();

    let source = r#"
            $persistentStore.write("pv1", "k1");
            $httpClient.get({url: "https://example.com/", headers: {"x-a": "1"}}, (error, response, data) => {
                if (error) {
                    $done({err: error});
                } else {
                    $done({status: response.status, body: data});
                }
            });
        "#;
    let out = engine
        .run_script(source, ScriptKind::Generic, None, None)
        .await
        .unwrap();
    assert_eq!(out.0["status"], 200);
    // Loon 的 $persistentStore 与 $httpClient 均生效
    assert_eq!(store.read("shared", "k1").unwrap(), Some("pv1".to_string()));
}

#[tokio::test(flavor = "current_thread")]
async fn infinite_loop_timeout() {
    let (host, _http, _notifier, _store) = test_host();
    let mut engine = QuickJsEngine::new(
        host,
        ScriptDialect::QuantumultX,
        ScriptLimits {
            timeout_ms: 500,
            ..ScriptLimits::default()
        },
        "loop".to_string(),
    )
    .unwrap();

    let source = "while(true) {}";
    let err = engine
        .run_script(source, ScriptKind::Generic, None, None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, LanpathError::Script(_)),
        "expected Script error, got {err:?}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn thrown_error_captured() {
    let (host, _http, _notifier, _store) = test_host();
    let mut engine = QuickJsEngine::new(
        host,
        ScriptDialect::QuantumultX,
        ScriptLimits::default(),
        "throw_test".to_string(),
    )
    .unwrap();

    let source = r#"throw new Error("boom");"#;
    let err = engine
        .run_script(source, ScriptKind::Generic, None, None)
        .await
        .unwrap_err();
    let LanpathError::Script(msg) = &err else {
        panic!("expected Script error, got {err:?}");
    };
    assert!(
        msg.contains("boom"),
        "error message should contain boom, got: {msg}"
    );
}

/// `$argument` 注入 e2e：argument 为 `Some` 时脚本内可读全局 `$argument` 字符串。
#[tokio::test(flavor = "current_thread")]
async fn argument_injected_as_global_string() {
    let (host, _http, _notifier, _store) = test_host();
    let mut engine = QuickJsEngine::new(
        host,
        ScriptDialect::Surge,
        ScriptLimits::default(),
        "arg-test".to_string(),
    )
    .unwrap();

    let source = r#"
            if (typeof $argument === "undefined") {
                $done({err: 1});
            } else {
                $done({arg: $argument});
            }
        "#;
    let out = engine
        .run_script(
            source,
            ScriptKind::Generic,
            None,
            Some("api.example.com|abc"),
        )
        .await
        .expect("script with $argument should succeed");
    assert_eq!(out.0["err"], serde_json::Value::Null);
    assert_eq!(out.0["arg"], "api.example.com|abc");

    // argument 为 None 时 $argument 保持 undefined（typeof 判定返回 err:1）。
    let out2 = engine
        .run_script(source, ScriptKind::Generic, None, None)
        .await
        .expect("script without $argument should succeed");
    assert_eq!(out2.0["err"], 1);
}
