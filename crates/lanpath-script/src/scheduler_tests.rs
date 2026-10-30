use super::*;
use crate::host::{MemoryPersistentStore, MockHttpExecutor, RecordingNotifier};
use crate::types::HttpResponseData;
use chrono::Duration as ChronoDuration;
use chrono::{Datelike, TimeZone};

fn test_host() -> (
    Arc<ScriptHost>,
    Arc<RecordingNotifier>,
    Arc<MemoryPersistentStore>,
) {
    let http = Arc::new(MockHttpExecutor::with_responses(vec![
        HttpResponseData {
            status: 200,
            headers: vec![("content-type".into(), "application/json".into())],
            body: br#"{"code":0,"token":"tok123"}"#.to_vec(),
        },
        HttpResponseData {
            status: 200,
            headers: vec![],
            body: br#"{"code":0}"#.to_vec(),
        },
    ]));
    let store = Arc::new(MemoryPersistentStore::new());
    let notifier = Arc::new(RecordingNotifier::new());
    let host = Arc::new(ScriptHost::new(http, store.clone(), notifier.clone()));
    (host, notifier, store)
}

fn qx_task(name: &str, cron_expr: &str, source: &str) -> TaskScript {
    TaskScript {
        name: name.into(),
        cron_expr: cron_expr.into(),
        source: source.into(),
        dialect: ScriptDialect::QuantumultX,
        enabled: true,
        argument: None,
        timeout_ms: None,
        source_id: String::new(),
        module_id: None,
    }
}

#[test]
fn legacy_task_deserialization_defaults_import_metadata() {
    let task: TaskScript = serde_json::from_value(serde_json::json!({
        "name": "legacy",
        "cron_expr": "*/5 * * * *",
        "source": "$done();",
        "dialect": "QuantumultX",
        "enabled": true
    }))
    .unwrap();
    assert_eq!(task, qx_task("legacy", "*/5 * * * *", "$done();"));

    let mut imported = task;
    imported.argument = Some("token=a&region=b".into());
    imported.timeout_ms = Some(1250);
    imported.source_id = "https://example.com/signin.js".into();
    imported.module_id = Some("module-a".into());
    let restored: TaskScript =
        serde_json::from_value(serde_json::to_value(&imported).unwrap()).unwrap();
    assert_eq!(restored, imported);
}

#[tokio::test(flavor = "current_thread")]
async fn add_task_rejects_invalid_cron() {
    let (host, _n, _s) = test_host();
    let mut scheduler = ScriptScheduler::new(host, ScriptLimits::default());
    let err = scheduler
        .add_task(qx_task("bad", "not a cron", "$done();"))
        .unwrap_err();
    assert!(matches!(err, LanpathError::Validation(_)));
    assert!(scheduler.list_tasks().is_empty());

    scheduler
        .add_task(qx_task("ok", "0 * * * * *", "$done();"))
        .unwrap();
    let dup = scheduler
        .add_task(qx_task("ok", "0 0 12 * * *", "$done();"))
        .unwrap_err();
    assert!(matches!(dup, LanpathError::Validation(_)));
    assert_eq!(scheduler.list_tasks().len(), 1);
    let id = scheduler.list_tasks()[0].id.clone();
    assert!(scheduler.remove_task(&id));
    assert!(!scheduler.remove_task(&id));
    assert!(scheduler.list_tasks().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn cron_field_counts_preserve_seconds_and_public_expression() {
    let (host, _n, _s) = test_host();
    let mut scheduler = ScriptScheduler::new(host, ScriptLimits::default());
    for (name, expression) in [
        ("five", "*/5 * * * *"),
        ("six", "15 */5 * * * *"),
        ("seven", "30 */5 * * * * 2032"),
    ] {
        scheduler
            .add_task(qx_task(name, expression, "$done();"))
            .unwrap();
    }
    let at_minute = Utc.with_ymd_and_hms(2032, 8, 1, 12, 35, 0).unwrap();
    {
        let tasks = scheduler.lock_tasks();
        let five = &tasks[0].schedule;
        let six = &tasks[1].schedule;
        let seven = &tasks[2].schedule;
        assert!(five.includes(at_minute));
        assert!(!five.includes(at_minute + ChronoDuration::seconds(15)));
        assert!(!six.includes(at_minute));
        assert!(six.includes(at_minute + ChronoDuration::seconds(15)));
        assert!(seven.includes(at_minute + ChronoDuration::seconds(30)));
        assert!(!seven.includes(Utc.with_ymd_and_hms(2031, 8, 1, 12, 35, 30).unwrap()));
        assert!(!ScriptScheduler::is_due(
            six,
            at_minute + ChronoDuration::seconds(14),
            Some(at_minute),
        ));
        assert!(ScriptScheduler::is_due(
            six,
            at_minute + ChronoDuration::seconds(15),
            Some(at_minute),
        ));
    }
    let expressions: Vec<_> = scheduler
        .list_tasks()
        .into_iter()
        .map(|t| t.cron_expr)
        .collect();
    assert_eq!(
        expressions,
        ["*/5 * * * *", "15 */5 * * * *", "30 */5 * * * * 2032"]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn imported_arguments_and_identity_reach_manual_and_due_execution() {
    let (host, _n, _s) = test_host();
    let mut scheduler = ScriptScheduler::new(host, ScriptLimits::default());
    let source = "$done({argument: $argument, identity: $script.name, kind: $script.type});";
    let mut manual = qx_task("display-name", "* * * * *", source);
    manual.argument = Some("token=a&label=签到".into());
    manual.source_id = "https://example.com/signin.js".into();
    manual.module_id = Some("module-a".into());
    manual.enabled = false;
    scheduler.add_task(manual).unwrap();
    let out = scheduler
        .run_now(&scheduler.list_tasks()[0].id)
        .await
        .unwrap();
    assert_eq!(out.0["argument"], "token=a&label=签到");
    assert_eq!(out.0["identity"], "display-name");
    assert_eq!(out.0["kind"], "cron");

    let mut due = qx_task("another-display", "* * * * *", source);
    due.argument = Some("token=b".into());
    due.source_id = "https://example.com/signin.js".into();
    due.module_id = Some("module-b".into());
    scheduler.add_task(due).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 8, 1, 12, 34, 0).unwrap();
    let results = scheduler.run_due(now).await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, scheduler.list_tasks()[1].id);
    let out = results[0].1.as_ref().unwrap();
    assert_eq!(out.0["argument"], "token=b");
    assert_eq!(out.0["identity"], "another-display");
}

#[tokio::test(flavor = "current_thread")]
async fn legacy_identity_and_absent_argument_remain_available() {
    let (host, _n, _s) = test_host();
    let mut scheduler = ScriptScheduler::new(host, ScriptLimits::default());
    scheduler
        .add_task(qx_task(
            "legacy-name",
            "* * * * *",
            "$done({identity: $script.name, argumentType: typeof $argument});",
        ))
        .unwrap();
    let out = scheduler
        .run_now(&scheduler.list_tasks()[0].id)
        .await
        .unwrap();
    assert_eq!(out.0["identity"], "legacy-name");
    assert_eq!(out.0["argumentType"], "undefined");
}

#[tokio::test(flavor = "current_thread")]
async fn surge_default_storage_uses_source_not_display_or_module_identity() {
    let (host, _n, _s) = test_host();
    let mut scheduler = ScriptScheduler::new(host, ScriptLimits::default());
    let mut writer = qx_task(
        "writer-display",
        "* * * * *",
        "$persistentStore.write($argument); $done({name: $script.name});",
    );
    writer.dialect = ScriptDialect::Surge;
    writer.source_id = "https://example.com/shared-task.js".into();
    writer.module_id = Some("module-writer".into());
    writer.argument = Some("persisted-value".into());
    writer.enabled = false;
    scheduler.add_task(writer).unwrap();
    let output = scheduler
        .run_now(&scheduler.list_tasks()[0].id)
        .await
        .unwrap();
    assert_eq!(output.0["name"], "writer-display");

    let source = "$done({value: $persistentStore.read(), name: $script.name});";
    let mut reader = qx_task("reader-display", "* * * * *", source);
    reader.dialect = ScriptDialect::Surge;
    reader.source_id = "https://example.com/shared-task.js".into();
    reader.module_id = Some("module-reader".into());
    scheduler.add_task(reader).unwrap();
    let mut unrelated = qx_task("unrelated-display", "* * * * *", source);
    unrelated.dialect = ScriptDialect::Surge;
    unrelated.source_id = "https://example.com/different-task.js".into();
    scheduler.add_task(unrelated).unwrap();

    let now = Utc.with_ymd_and_hms(2026, 8, 1, 12, 34, 0).unwrap();
    let results = scheduler.run_due(now).await;
    assert_eq!(results.len(), 2);
    let shared = results[0].1.as_ref().unwrap();
    assert_eq!(shared.0["value"], "persisted-value");
    assert_eq!(shared.0["name"], "reader-display");
    assert!(results[1].1.as_ref().unwrap().0["value"].is_null());
}

#[tokio::test(flavor = "current_thread")]
async fn loon_default_storage_uses_display_name_even_for_shared_source() {
    let (host, _n, _s) = test_host();
    let mut scheduler = ScriptScheduler::new(host, ScriptLimits::default());
    let source = r#"
        if (typeof $argument !== "undefined") {
            $persistentStore.write($argument);
        }
        $done({value: $persistentStore.read(), name: $script.name});
    "#;
    let mut writer = qx_task("loon-writer", "* * * * *", source);
    writer.dialect = ScriptDialect::Loon;
    writer.source_id = "https://example.com/shared-loon.js".into();
    writer.argument = Some("private-value".into());
    scheduler.add_task(writer).unwrap();
    let output = scheduler
        .run_now(&scheduler.list_tasks()[0].id)
        .await
        .unwrap();
    assert_eq!(output.0["value"], "private-value");
    assert_eq!(output.0["name"], "loon-writer");

    let mut reader = qx_task("loon-reader", "* * * * *", source);
    reader.dialect = ScriptDialect::Loon;
    reader.source_id = "https://example.com/shared-loon.js".into();
    scheduler.add_task(reader).unwrap();
    let output = scheduler
        .run_now(&scheduler.list_tasks()[1].id)
        .await
        .unwrap();
    assert!(output.0["value"].is_null());
    assert_eq!(output.0["name"], "loon-reader");
}

#[tokio::test(flavor = "current_thread")]
async fn task_timeout_limits_manual_and_due_runs_without_stopping_other_tasks() {
    let (host, _n, _s) = test_host();
    let mut scheduler = ScriptScheduler::new(host, ScriptLimits::default());
    let mut slow = qx_task(
        "slow",
        "* * * * *",
        "setTimeout(() => $done({late: true}), 5000);",
    );
    slow.timeout_ms = Some(50);
    scheduler.add_task(slow).unwrap();
    let error = scheduler
        .run_now(&scheduler.list_tasks()[0].id)
        .await
        .unwrap_err();
    assert!(matches!(error, LanpathError::Script(_)));
    let view = &scheduler.list_tasks()[0];
    assert!(view.last_run.is_some());
    assert!(view.last_error.is_some());

    scheduler
        .add_task(qx_task("fast", "* * * * *", "$done({ok: true});"))
        .unwrap();
    let now = Utc::now() + ChronoDuration::minutes(1);
    let results = scheduler.run_due(now).await;
    assert_eq!(results.len(), 2);
    assert!(results[0].1.is_err());
    assert_eq!(results[1].1.as_ref().unwrap().0["ok"], true);
    assert!(scheduler.list_tasks()[1].last_error.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn task_timeout_can_extend_the_scheduler_default() {
    let (host, _n, _s) = test_host();
    let mut scheduler = ScriptScheduler::new(
        host,
        ScriptLimits {
            timeout_ms: 5,
            ..ScriptLimits::default()
        },
    );
    let mut task = qx_task(
        "extended",
        "* * * * *",
        "setTimeout(() => $done({ok: true}), 30);",
    );
    task.timeout_ms = Some(2000);
    scheduler.add_task(task).unwrap();
    let out = scheduler
        .run_now(&scheduler.list_tasks()[0].id)
        .await
        .unwrap();
    assert_eq!(out.0["ok"], true);
    assert!(scheduler.list_tasks()[0].last_error.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn run_due_only_fires_due_tasks() {
    let (host, _n, _s) = test_host();
    let mut scheduler = ScriptScheduler::new(host, ScriptLimits::default());
    scheduler
        .add_task(qx_task("every_min", "0 * * * * *", "$done({ok: true});"))
        .unwrap();
    let now = Utc.with_ymd_and_hms(2026, 8, 1, 12, 34, 0).unwrap();
    let next_week = now + ChronoDuration::days(7);
    let later_expr = format!(
        "0 0 12 {} {} ? {}",
        next_week.day(),
        next_week.month(),
        next_week.year()
    );
    scheduler
        .add_task(qx_task("next_week", &later_expr, "$done({ok: true});"))
        .unwrap();
    let results = scheduler.run_due(now).await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, scheduler.list_tasks()[0].id);
    assert!(results[0].1.is_ok());
    assert!(scheduler.run_due(now).await.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn start_stop_smoke() {
    let (host, _n, _s) = test_host();
    let mut scheduler = ScriptScheduler::new(host, ScriptLimits::default());
    scheduler
        .add_task(qx_task("minutely", "0 * * * * *", "$done();"))
        .unwrap();
    let scheduler = Arc::new(scheduler);
    let handle = scheduler
        .clone()
        .start_with_interval(Duration::from_millis(20))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!handle.is_finished());
    scheduler.stop().await;
    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("handle should exit after stop")
        .expect("handle join ok");
}

#[tokio::test(flavor = "current_thread")]
async fn qualified_tasks_keep_manual_due_error_and_removal_records_independent() {
    let (host, _n, _s) = test_host();
    let mut scheduler = ScriptScheduler::new(host, ScriptLimits::default());
    let source = r#"
        if ($argument === "fail") throw new Error("module-specific failure");
        $done({argument: $argument, name: $script.name});
    "#;
    let mut first = qx_task("shared-task", "* * * * *", source);
    first.module_id = Some("module-a".into());
    first.source_id = "https://example.com/shared.js".into();
    first.argument = Some("fail".into());
    let mut second = first.clone();
    second.module_id = Some("module-b".into());
    second.argument = Some("second".into());
    let first_id = first.id();
    let second_id = second.id();
    assert_ne!(first_id, second_id);
    scheduler.add_task(first.clone()).unwrap();
    scheduler.add_task(second).unwrap();
    first.argument = Some("changed".into());
    first.cron_expr = "0 9 * * *".into();
    assert!(scheduler.add_task(first).is_err());
    assert!(scheduler.run_now("shared-task").await.is_err());
    assert!(scheduler.run_now(&first_id).await.is_err());
    let after_failure = scheduler.list_tasks();
    assert!(after_failure[0].last_error.is_some());
    assert!(after_failure[1].last_run.is_none());
    let output = scheduler.run_now(&second_id).await.unwrap();
    assert_eq!(output.0["argument"], "second");
    assert_eq!(output.0["name"], "shared-task");
    let after_success = scheduler.list_tasks();
    assert_eq!(after_failure[0].last_run, after_success[0].last_run);
    assert_eq!(after_failure[0].last_error, after_success[0].last_error);
    assert!(after_success[1].last_run.is_some());
    assert!(after_success[1].last_error.is_none());

    let now = Utc::now() + ChronoDuration::minutes(1);
    let results = scheduler.run_due(now).await;
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, first_id);
    assert!(results[0].1.is_err());
    assert_eq!(results[1].0, second_id);
    assert_eq!(results[1].1.as_ref().unwrap().0["argument"], "second");
    let after_due = scheduler.list_tasks();
    assert_eq!(after_due[0].last_run, Some(now));
    assert_eq!(after_due[1].last_run, Some(now));
    assert!(after_due[0].last_error.is_some());
    assert!(after_due[1].last_error.is_none());
    assert!(scheduler.run_due(now).await.is_empty());

    assert!(scheduler.remove_task(&first_id));
    assert!(!scheduler.remove_task(&first_id));
    assert!(scheduler.run_now(&first_id).await.is_err());
    let later = now + ChronoDuration::minutes(1);
    let results = scheduler.run_due(later).await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, second_id);
    assert_eq!(results[0].1.as_ref().unwrap().0["argument"], "second");
    assert_eq!(scheduler.list_tasks()[0].last_run, Some(later));
}

#[tokio::test(flavor = "current_thread")]
async fn structured_identity_does_not_alias_delimiters_or_missing_modules() {
    let (host, _n, _s) = test_host();
    let mut scheduler = ScriptScheduler::new(host, ScriptLimits::default());
    for (module, source, name, argument) in [
        (Some("module:source"), "asset", "task", "first"),
        (Some("module"), "source:asset", "task", "second"),
        (Some("module"), "source", "asset:task", "third"),
        (None, "source", "task", "fourth"),
        (Some(""), "source", "task", "fifth"),
    ] {
        let mut task = qx_task(name, "* * * * *", "$done({argument: $argument});");
        task.module_id = module.map(str::to_owned);
        task.source_id = source.into();
        task.argument = Some(argument.into());
        scheduler.add_task(task).unwrap();
    }
    let tasks = scheduler.list_tasks();
    assert_eq!(tasks.len(), 5);
    for (task, argument) in tasks
        .iter()
        .zip(["first", "second", "third", "fourth", "fifth"])
    {
        assert_eq!(
            scheduler.run_now(&task.id).await.unwrap().0["argument"],
            argument
        );
    }
}
