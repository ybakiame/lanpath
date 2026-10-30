use super::*;
use crate::{
    HttpExecutor, HttpRequestSpec, HttpResponseData, MemoryPersistentStore, MockHttpExecutor,
    RecordingNotifier, ScriptCompletion, ScriptWorker,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn host(http: Arc<dyn HttpExecutor>) -> Arc<ScriptHost> {
    Arc::new(ScriptHost::new(
        http,
        Arc::new(MemoryPersistentStore::new()),
        Arc::new(RecordingNotifier::new()),
    ))
}

#[derive(Default)]
struct CaptureHttp {
    sessions: AtomicUsize,
    requests: std::sync::Mutex<Vec<HttpRequestSpec>>,
}

#[async_trait::async_trait]
impl HttpExecutor for CaptureHttp {
    fn session(self: Arc<Self>) -> Arc<dyn HttpExecutor> {
        self.sessions.fetch_add(1, Ordering::SeqCst);
        self
    }
    async fn execute(&self, request: HttpRequestSpec) -> LanpathResult<HttpResponseData> {
        self.requests.lock().unwrap().push(request);
        Ok(HttpResponseData {
            status: 200,
            headers: vec![
                ("Set-Cookie".into(), "a=1".into()),
                ("Set-Cookie".into(), "b=2".into()),
            ],
            body: vec![0, 255, 128],
        })
    }
}

#[tokio::test]
async fn completion_keeps_undefined_null_and_json_string_distinct_across_reused_engine() {
    let mut engine = QuickJsEngine::new(
        host(Arc::new(MockHttpExecutor::with_responses(vec![]))),
        ScriptDialect::Surge,
        ScriptLimits::default(),
        "completion".into(),
    )
    .unwrap();
    for (source, value, completion) in [
        (
            "$done()",
            serde_json::Value::Null,
            ScriptCompletion::Undefined,
        ),
        (
            "$done(null)",
            serde_json::Value::Null,
            ScriptCompletion::Value,
        ),
        (
            r#"$done('{"body":"literal"}')"#,
            serde_json::json!(r#"{"body":"literal"}"#),
            ScriptCompletion::Value,
        ),
    ] {
        let result = engine
            .run_script(source, ScriptKind::Generic, None, None)
            .await
            .unwrap();
        assert_eq!(result.0, value);
        assert_eq!(result.1, completion);
    }
}

#[tokio::test]
async fn completion_prevents_pending_and_same_batch_timer_store_side_effects() {
    for source in [
        "setTimeout(() => $persistentStore.write('late', 'timer-leak'), 0); $done('accepted');",
        "setTimeout(() => $done('accepted'), 0); setTimeout(() => $persistentStore.write('late', 'timer-leak'), 0);",
    ] {
        let mut engine = QuickJsEngine::new(
            host(Arc::new(MockHttpExecutor::with_responses(vec![]))),
            ScriptDialect::Surge,
            ScriptLimits::default(),
            "timer-barrier".into(),
        )
        .unwrap();
        let completed = engine
            .run_script(source, ScriptKind::Generic, None, None)
            .await
            .unwrap();
        assert_eq!(completed.0, "accepted");
        let stored = engine
            .run_script(
                "$done($persistentStore.read('timer-leak'))",
                ScriptKind::Generic,
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(stored.0, serde_json::Value::Null);
    }
}

#[tokio::test]
async fn stash_clears_due_timeouts_and_stops_a_self_cancelled_interval() {
    let mut engine = QuickJsEngine::new(
        host(Arc::new(MockHttpExecutor::with_responses(vec![]))),
        ScriptDialect::Stash,
        ScriptLimits::default(),
        "stash-timers".into(),
    )
    .unwrap();
    let output = engine
        .run_script(
            r#"
        for (let i = 0; i < 80; i++) {
            const handle = setTimeout(() => $persistentStore.write('cancelled', 'timer-leak'), 0);
            clearTimeout(handle);
        }
        let later;
        setTimeout(() => clearTimeout(later), 0);
        later = setTimeout(() => $persistentStore.write('later', 'timer-leak'), 0);
        let count = 0;
        const interval = setInterval(() => {
            count++;
            if (count === 3) {
                clearInterval(interval);
                setTimeout(() => setTimeout(() => $done({
                    count, leak: $persistentStore.read('timer-leak')
                }), 0), 0);
            }
        }, 0);
    "#,
            ScriptKind::Generic,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(output.0, serde_json::json!({"count":3,"leak":null}));
}

#[tokio::test]
async fn stash_queue_microtask_requires_a_callable_and_returns_undefined() {
    let mut engine = QuickJsEngine::new(
        host(Arc::new(CaptureHttp::default())),
        ScriptDialect::Stash,
        ScriptLimits::default(),
        "microtask-contract".into(),
    )
    .unwrap();
    let output = engine
        .run_script(
            r#"
        let rejected = 0;
        for (const value of [null, undefined, 42, 'callback']) {
            try { queueMicrotask(value); } catch (error) {
                if (error instanceof TypeError) rejected++;
            }
        }
        const returned = queueMicrotask(function() {
            $done({rejected, returned:typeof returned, args:arguments.length});
        });
    "#,
            ScriptKind::Generic,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        output.0,
        serde_json::json!({"rejected":4,"returned":"undefined","args":0})
    );
}

#[tokio::test]
async fn stash_microtasks_share_promise_fifo_and_checkpoint_between_due_timers() {
    let mut engine = QuickJsEngine::new(
        host(Arc::new(CaptureHttp::default())),
        ScriptDialect::Stash,
        ScriptLimits::default(),
        "microtask-order".into(),
    )
    .unwrap();
    let output = engine
        .run_script(
            r#"
        const events = ['sync'];
        queueMicrotask(() => {
            events.push('microtask');
            queueMicrotask(() => events.push('nested'));
        });
        Promise.resolve().then(() => events.push('promise'));
        setTimeout(() => {
            events.push('timer');
            queueMicrotask(() => events.push('timer-microtask'));
            Promise.resolve().then(() => events.push('timer-promise'));
        }, 0);
        setTimeout(() => $done(events), 0);
    "#,
            ScriptKind::Generic,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        output.0,
        serde_json::json!([
            "sync",
            "microtask",
            "promise",
            "nested",
            "timer",
            "timer-microtask",
            "timer-promise"
        ])
    );
}

#[tokio::test]
async fn stash_microtasks_cancel_due_timers_and_a_running_interval_before_its_next_turn() {
    let mut engine = QuickJsEngine::new(
        host(Arc::new(CaptureHttp::default())),
        ScriptDialect::Stash,
        ScriptLimits::default(),
        "microtask-cancel".into(),
    )
    .unwrap();
    let output = engine
        .run_script(
            r#"
        let count = 0;
        let leaked = false;
        const interval = setInterval(() => {
            count++;
            queueMicrotask(() => clearInterval(interval));
        }, 0);
        const cancelled = setTimeout(() => { leaked = true; }, 0);
        queueMicrotask(() => clearTimeout(cancelled));
        const later = setTimeout(() => setTimeout(() => $done({
            count, leaked, handles:interval > 0 && cancelled > interval && later > cancelled
        }), 0), 0);
    "#,
            ScriptKind::Generic,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        output.0,
        serde_json::json!({"count":1,"leaked":false,"handles":true})
    );
}

#[tokio::test]
async fn stash_completion_blocks_sync_microtask_promise_timer_and_host_side_effects() {
    for source in [
        "queueMicrotask(()=>$persistentStore.write('late','microtask-leak')); $done('accepted');",
        "queueMicrotask(()=>$done('accepted')); queueMicrotask(()=>$persistentStore.write('late','microtask-leak'));",
        "queueMicrotask(()=>{$done('accepted');$persistentStore.write('late','microtask-leak')});",
        "queueMicrotask(()=>$done('accepted')); Promise.resolve().then(()=>$persistentStore.write('late','microtask-leak'));",
        "queueMicrotask(()=>$done('accepted')); Promise.resolve().then(()=>$notification.post('late','',''));",
        "queueMicrotask(()=>$done('accepted')); Promise.resolve().then(()=>$httpClient.get('http://fixture.invalid/',()=>{}));",
        "setTimeout(()=>{queueMicrotask(()=>$done('accepted'));queueMicrotask(()=>$persistentStore.write('late','microtask-leak'))},0);",
        "queueMicrotask(()=>$done('accepted')); setTimeout(()=>$persistentStore.write('late','microtask-leak'),0);",
        "$done('accepted'); $persistentStore.write('late','microtask-leak');",
        "queueMicrotask(()=>{$done('accepted');throw Error('late throw')});",
    ] {
        let http = Arc::new(CaptureHttp::default());
        let notifier = Arc::new(RecordingNotifier::new());
        let mut engine = QuickJsEngine::new(
            Arc::new(ScriptHost::new(
                http.clone(),
                Arc::new(MemoryPersistentStore::new()),
                notifier.clone(),
            )),
            ScriptDialect::Stash,
            ScriptLimits::default(),
            "microtask-barrier".into(),
        )
        .unwrap();
        let output = engine
            .run_script(source, ScriptKind::Generic, None, None)
            .await
            .unwrap();
        assert_eq!(output.0, "accepted", "{source}");
        let stored = engine
            .run_script(
                "$done($persistentStore.read('microtask-leak'))",
                ScriptKind::Generic,
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(stored.0, serde_json::Value::Null, "{source}");
        assert!(notifier.calls().is_empty(), "{source}");
        assert!(http.requests.lock().unwrap().is_empty(), "{source}");
    }
}

#[tokio::test]
async fn stash_http_callback_microtasks_run_before_timers_and_completion_blocks_other_http_callbacks()
 {
    let http = Arc::new(CaptureHttp::default());
    let mut engine = QuickJsEngine::new(
        host(http.clone()),
        ScriptDialect::Stash,
        ScriptLimits::default(),
        "http-microtask".into(),
    )
    .unwrap();
    let output = engine
        .run_script(
            r#"
        const events = [];
        $httpClient.get('http://fixture.invalid/', () => {
            events.push('http');
            queueMicrotask(() => events.push('microtask'));
            setTimeout(() => $done(events), 0);
        });
    "#,
            ScriptKind::Generic,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(output.0, serde_json::json!(["http", "microtask"]));
    let output = engine.run_script(r#"
        $httpClient.get('http://fixture.invalid/first', () => $done('accepted'));
        $httpClient.get('http://fixture.invalid/late', () => $persistentStore.write('late','microtask-leak'));
    "#, ScriptKind::Generic, None, None).await.unwrap();
    assert_eq!(output.0, "accepted");
    assert_eq!(http.requests.lock().unwrap().len(), 2);
    let stored = engine
        .run_script(
            "$done($persistentStore.read('microtask-leak'))",
            ScriptKind::Generic,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(stored.0, serde_json::Value::Null);
}

#[tokio::test]
async fn stash_microtask_exceptions_report_original_values_and_stop_later_jobs() {
    for source in [
        "queueMicrotask(()=>{throw Error('microtask boom')}); queueMicrotask(()=>{$persistentStore.write('late','microtask-leak');$done('wrong')});",
        "queueMicrotask(()=>{throw 'microtask boom'}); setTimeout(()=>$done('wrong'),0);",
        "Promise.resolve().then(()=>queueMicrotask(()=>{throw Error('microtask boom')}));",
        "setTimeout(()=>queueMicrotask(()=>{throw Error('microtask boom')}),0);",
        "$httpClient.get('http://fixture.invalid/',()=>queueMicrotask(()=>{throw Error('microtask boom')}));",
        "(async()=>{await Promise.resolve();throw Error('promise boom')})()",
    ] {
        let mut engine = QuickJsEngine::new(
            host(Arc::new(CaptureHttp::default())),
            ScriptDialect::Stash,
            ScriptLimits::default(),
            "microtask-errors".into(),
        )
        .unwrap();
        let started = Instant::now();
        let error = engine
            .run_script(source, ScriptKind::Generic, None, None)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("boom"), "{error}");
        assert!(!error.contains("timeout"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(1));
        let stored = engine
            .run_script(
                "$done($persistentStore.read('microtask-leak'))",
                ScriptKind::Generic,
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(stored.0, serde_json::Value::Null);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stash_microtask_deadline_and_caller_cancellation_release_the_worker() {
    let worker = ScriptWorker::new(
        host(Arc::new(CaptureHttp::default())),
        ScriptLimits::default(),
    );
    let source = "queueMicrotask(function again(){queueMicrotask(again)}); setTimeout(()=>$persistentStore.write('late','microtask-leak'),0);";
    worker
        .run_script_with_timeout(
            source,
            ScriptKind::Generic,
            None,
            None,
            ScriptDialect::Stash,
            crate::ScriptExecutionMetadata {
                name: "microtask-deadline".into(),
                source_id: "microtask-deadline".into(),
                ..Default::default()
            },
            Some(100),
        )
        .await
        .unwrap_err();
    let active = worker.clone();
    let task = tokio::spawn(async move {
        active
            .run_script_with_timeout(
                source,
                ScriptKind::Generic,
                None,
                None,
                ScriptDialect::Stash,
                crate::ScriptExecutionMetadata {
                    name: "microtask-cancel".into(),
                    source_id: "microtask-cancel".into(),
                    ..Default::default()
                },
                Some(10000),
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    task.abort();
    let _ = task.await;
    let recovered = tokio::time::timeout(
        Duration::from_secs(1),
        worker.run_script(
            "$done({leak:$persistentStore.read('microtask-leak')})",
            ScriptKind::Generic,
            None,
            None,
            ScriptDialect::Stash,
            "microtask-recovered",
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(recovered.0, serde_json::json!({"leak":null}));
    worker.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stash_worker_file_store_microtasks_smoke() {
    let root = tempfile::tempdir().unwrap();
    let make_worker = || {
        ScriptWorker::new(
            Arc::new(ScriptHost::new(
                Arc::new(CaptureHttp::default()),
                Arc::new(crate::FilePersistentStore::new(root.path().to_path_buf())),
                Arc::new(RecordingNotifier::new()),
            )),
            ScriptLimits::default(),
        )
    };
    let worker = make_worker();
    let output = worker
        .run_script(
            r#"
        const events = ['sync'];
        queueMicrotask(() => events.push('microtask'));
        Promise.resolve().then(() => events.push('promise'));
        setTimeout(() => {
            events.push('timer');
            queueMicrotask(() => {
                events.push('timer-microtask');
                $persistentStore.write(JSON.stringify(events), 'microtask-order');
                $done(events);
            });
            queueMicrotask(() => $persistentStore.write('late', 'microtask-leak'));
        }, 0);
    "#,
            ScriptKind::Generic,
            None,
            None,
            ScriptDialect::Stash,
            "file-smoke",
        )
        .await
        .unwrap();
    let expected = serde_json::json!(["sync", "microtask", "promise", "timer", "timer-microtask"]);
    assert_eq!(output.0, expected);
    worker.shutdown().await;
    let reopened = make_worker();
    let stored = reopened
        .run_script(
            r#"
        $done({events:JSON.parse($persistentStore.read('microtask-order')),
            leak:$persistentStore.read('microtask-leak')});
    "#,
            ScriptKind::Generic,
            None,
            None,
            ScriptDialect::Stash,
            "file-smoke-reopened",
        )
        .await
        .unwrap();
    assert_eq!(stored.0, serde_json::json!({"events":expected,"leak":null}));
    reopened.shutdown().await;
}

#[tokio::test]
async fn documented_http_units_options_binary_and_full_header_shapes_reach_host() {
    for (dialect, timeout, expected_ms, insecure) in [
        (ScriptDialect::Surge, 2.5, 2500, false),
        (ScriptDialect::Stash, 2.5, 2500, false),
        (ScriptDialect::Loon, 2500.0, 2500, true),
    ] {
        let http = Arc::new(CaptureHttp::default());
        let mut engine = QuickJsEngine::new(
            host(http.clone()),
            dialect,
            ScriptLimits::default(),
            "options".into(),
        )
        .unwrap();
        let source = format!(
            r#"
            $httpClient.post({{url:'http://fixture.invalid/', timeout:{timeout}, body:new Uint8Array([0,255,128]), 'binary-mode':true, 'full-header-mode':true, 'auto-cookie':false, 'auto-redirect':false}}, (error,response,data) => {{
                if (error) throw Error(error);
                if (!(data instanceof Uint8Array) || data[1] !== 255 || response.headers.length !== 2) throw Error('binary or headers lost');
                $done({{bytes:data,headers:response.headers}});
            }});
        "#
        );
        let result = engine
            .run_script(&source, ScriptKind::Generic, None, None)
            .await
            .unwrap();
        assert_eq!(result.0["bytes"], serde_json::json!([0, 255, 128]));
        let requests = http.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].timeout_ms, Some(expected_ms));
        assert_eq!(requests[0].body, Some(vec![0, 255, 128]));
        assert!(!requests[0].auto_cookie);
        assert!(!requests[0].auto_redirect);
        assert_eq!(requests[0].insecure, insecure);
        assert_eq!(http.sessions.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn qx_options_body_buffer_and_error_object_are_not_callback_dialect_globals() {
    let http = Arc::new(CaptureHttp::default());
    let mut engine = QuickJsEngine::new(
        host(http.clone()),
        ScriptDialect::QuantumultX,
        ScriptLimits::default(),
        "qx".into(),
    )
    .unwrap();
    let output = engine.run_script(r#"
        (async()=>{
            const response = await $task.fetch({url:'http://fixture.invalid/',bodyBytes:new Uint8Array([255,0]).buffer,opts:{redirection:false,'skip-cert-verify':true,'auto-cookie':true}});
            if (!(response.bodyBytes instanceof ArrayBuffer) || new Uint8Array(response.bodyBytes)[1] !== 255) throw Error('not buffer');
            try { await $task.fetch({url:'http://fixture.invalid/',opts:{policy:'unsupported'}}); throw Error('accepted policy'); }
            catch(reason) { if(typeof reason.error !== 'string') throw Error('missing reason.error'); }
            $done({callback:typeof $httpClient,prefs:typeof $prefs});
        })()
    "#, ScriptKind::Generic, None, None).await.unwrap();
    assert_eq!(
        output.0,
        serde_json::json!({"callback":"undefined","prefs":"object"})
    );
    let requests = http.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].body, Some(vec![255, 0]));
    assert!(requests[0].auto_cookie && requests[0].insecure && !requests[0].auto_redirect);
}

#[tokio::test]
async fn loon_globals_are_documented_types_and_do_not_select_qx_environment_branch() {
    let mut engine = QuickJsEngine::new(
        host(Arc::new(MockHttpExecutor::with_responses(vec![]))),
        ScriptDialect::Loon,
        ScriptLimits::default(),
        "loon-name".into(),
    )
    .unwrap();
    let output = engine.run_script(r#"
        setInterval((text, count)=>$done({loon:typeof $loon,started:$script.startTime instanceof Date,name:$script.name,qx:typeof $task,prefs:typeof $prefs,notify:typeof $notify,args:[text,count]}),1,'timer',2);
    "#, ScriptKind::Generic, None, None).await.unwrap();
    assert_eq!(
        output.0,
        serde_json::json!({"loon":"string","started":true,"name":"loon-name","qx":"undefined","prefs":"undefined","notify":"undefined","args":["timer",2]})
    );
    let output = engine
        .run_script(
            r#"
        const events = [];
        setInterval(() => {
            events.push('interval');
            Promise.resolve().then(() => events.push('promise'));
        }, 0);
        setTimeout(() => $done(events), 5);
    "#,
            ScriptKind::Generic,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(output.0, serde_json::json!(["interval", "promise"]));
}

#[tokio::test]
async fn callback_and_timer_exceptions_are_immediate_errors_not_deadline_timeouts() {
    let http = Arc::new(CaptureHttp::default());
    let mut engine = QuickJsEngine::new(
        host(http),
        ScriptDialect::Surge,
        ScriptLimits::default(),
        "errors".into(),
    )
    .unwrap();
    for source in [
        "$httpClient.get('http://fixture.invalid/',()=>{throw Error('callback boom')})",
        "setTimeout(()=>{throw Error('timer boom')},1)",
    ] {
        let started = Instant::now();
        let error = engine
            .run_script(source, ScriptKind::Generic, None, None)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("boom"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}

struct LoopbackHttp {
    started: Arc<AtomicUsize>,
    dropped: Arc<AtomicUsize>,
}
struct InFlight(Arc<AtomicUsize>);
impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl HttpExecutor for LoopbackHttp {
    fn session(self: Arc<Self>) -> Arc<dyn HttpExecutor> {
        self
    }
    async fn execute(&self, request: HttpRequestSpec) -> LanpathResult<HttpResponseData> {
        let _in_flight = InFlight(self.dropped.clone());
        self.started.fetch_add(1, Ordering::SeqCst);
        let target = request.url.strip_prefix("http://").unwrap();
        let (authority, path) = target.split_once('/').unwrap();
        let mut stream = tokio::net::TcpStream::connect(authority)
            .await
            .map_err(|error| LanpathError::Internal(error.to_string()))?;
        let message =
            format!("GET /{path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
        stream
            .write_all(message.as_bytes())
            .await
            .map_err(|error| LanpathError::Internal(error.to_string()))?;
        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .await
            .map_err(|error| LanpathError::Internal(error.to_string()))?;
        let separator = response
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
            .ok_or_else(|| LanpathError::Internal("incomplete HTTP fixture response".into()))?;
        Ok(HttpResponseData {
            status: 200,
            headers: vec![],
            body: response[separator + 4..].to_vec(),
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wall_clock_timeout_drops_real_loopback_http_future_and_worker_recovers() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0u8; 1024];
        assert!(stream.read(&mut bytes).await.unwrap() > 0);
        // An HTTP future that is truly dropped closes the connection without waiting for a response.
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), stream.read(&mut bytes))
                .await
                .unwrap()
                .unwrap(),
            0
        );
    });
    let started = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicUsize::new(0));
    let worker = ScriptWorker::new(
        host(Arc::new(LoopbackHttp {
            started: started.clone(),
            dropped: dropped.clone(),
        })),
        ScriptLimits::default(),
    );
    let source = format!("$httpClient.get('http://{address}/hang',()=>{{$done({{}})}})");
    let beginning = Instant::now();
    worker
        .run_script_with_timeout(
            &source,
            ScriptKind::Generic,
            None,
            None,
            ScriptDialect::Surge,
            crate::ScriptExecutionMetadata {
                name: "hang".into(),
                source_id: "fixture".into(),
                ..Default::default()
            },
            Some(150),
        )
        .await
        .unwrap_err();
    assert!(beginning.elapsed() < Duration::from_secs(1));
    let recovered = worker
        .run_script(
            "$done({ok:true})",
            ScriptKind::Generic,
            None,
            None,
            ScriptDialect::Surge,
            "recovered",
        )
        .await
        .unwrap();
    assert_eq!(recovered.0["ok"], true);
    assert_eq!(started.load(Ordering::SeqCst), 1);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    server.await.unwrap();
    worker.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queued_deadline_is_inclusive_and_cancelled_cpu_run_releases_worker() {
    let notifier = Arc::new(RecordingNotifier::new());
    let worker = ScriptWorker::new(
        Arc::new(ScriptHost::new(
            Arc::new(MockHttpExecutor::with_responses(vec![])),
            Arc::new(MemoryPersistentStore::new()),
            notifier.clone(),
        )),
        ScriptLimits::default(),
    );
    let first = worker.clone();
    let active = tokio::spawn(async move {
        first
            .run_script(
                "setTimeout(()=>$done({}),200)",
                ScriptKind::Generic,
                None,
                None,
                ScriptDialect::Surge,
                "active",
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    let beginning = Instant::now();
    assert!(
        worker
            .run_script_with_timeout(
                "$notification.post('should not execute','','');$done({})",
                ScriptKind::Generic,
                None,
                None,
                ScriptDialect::Surge,
                crate::ScriptExecutionMetadata {
                    name: "queued".into(),
                    source_id: "queued".into(),
                    ..Default::default()
                },
                Some(30)
            )
            .await
            .is_err()
    );
    assert!(beginning.elapsed() < Duration::from_millis(150));
    active.await.unwrap().unwrap();
    assert!(notifier.calls().is_empty());
    let cpu = worker.clone();
    let task = tokio::spawn(async move {
        cpu.run_script_with_timeout(
            "while(true){}",
            ScriptKind::Generic,
            None,
            None,
            ScriptDialect::Surge,
            crate::ScriptExecutionMetadata {
                name: "cpu".into(),
                source_id: "cpu".into(),
                ..Default::default()
            },
            Some(10000),
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    task.abort();
    let _ = task.await;
    let recovered = tokio::time::timeout(
        Duration::from_secs(1),
        worker.run_script(
            "$done({ok:true})",
            ScriptKind::Generic,
            None,
            None,
            ScriptDialect::Surge,
            "next",
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(recovered.0["ok"], true);
    worker.shutdown().await;
}
