use std::{path::PathBuf, sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use lanpath_common::{LanpathError, LanpathResult};
use lanpath_script::{
    FilePersistentStore, HttpExecutor, Notifier, ScriptHost, ScriptLimits, ScriptScheduler,
    TaskScriptView,
};
use tokio::task::JoinHandle;

use super::RemoteManager;

/// Runtime presentation of one scheduled script task.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RemoteTaskView {
    pub id: String,
    pub name: String,
    pub cron_expr: String,
    pub dialect: String,
    pub enabled: bool,
    pub next_run: Option<DateTime<Utc>>,
    pub last_run: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

impl From<TaskScriptView> for RemoteTaskView {
    fn from(value: TaskScriptView) -> Self {
        Self {
            id: value.id,
            name: value.name,
            cron_expr: value.cron_expr,
            dialect: format!("{:?}", value.dialect),
            enabled: value.enabled,
            next_run: value.next_run,
            last_run: value.last_run,
            last_error: value.last_error,
        }
    }
}

/// One running scheduler session. Dropping it cancels the background task and its in-flight run.
pub struct ScriptTaskSession {
    scheduler: Arc<ScriptScheduler>,
    handle: JoinHandle<()>,
}

impl ScriptTaskSession {
    /// Build and start a scheduler from cached remote snippets.
    ///
    /// Returns `None` when no enabled task can be registered.
    pub async fn start(
        data_dir: PathBuf,
        notifier: Arc<dyn Notifier>,
        http: Arc<dyn HttpExecutor>,
    ) -> LanpathResult<Option<Self>> {
        let cached = RemoteManager::new(data_dir.clone()).load_cached()?;
        for warning in cached.warnings {
            tracing::warn!(%warning, "scheduled task cache activation warning");
        }
        let tasks = cached.task_scripts;
        let host = Arc::new(ScriptHost::new(
            http,
            Arc::new(FilePersistentStore::new(data_dir.join("script_store"))),
            notifier,
        ));
        let mut scheduler = ScriptScheduler::new(host, ScriptLimits::default());
        let mut registered = 0usize;
        for task in tasks.into_iter().filter(|task| task.enabled) {
            match scheduler.add_task(task) {
                Ok(()) => registered += 1,
                Err(error) => {
                    tracing::warn!(%error, "skip scheduled task after registration failure")
                }
            }
        }
        if registered == 0 {
            return Ok(None);
        }
        let scheduler = Arc::new(scheduler);
        let handle = Arc::clone(&scheduler).start().await?;
        Ok(Some(Self { scheduler, handle }))
    }

    pub fn scheduler(&self) -> &ScriptScheduler {
        &self.scheduler
    }

    pub fn scheduler_handle(&self) -> Arc<ScriptScheduler> {
        Arc::clone(&self.scheduler)
    }

    pub fn list_tasks(&self) -> Vec<RemoteTaskView> {
        self.scheduler
            .list_tasks()
            .into_iter()
            .map(RemoteTaskView::from)
            .collect()
    }

    pub async fn run_now(&self, id: &str) -> LanpathResult<String> {
        if id.trim().is_empty() {
            return Err(LanpathError::Validation("任务 ID 不能为空".into()));
        }
        self.scheduler
            .run_now(id)
            .await
            .map(|output| output.0.to_string())
    }

    pub async fn stop(mut self) {
        self.scheduler.stop().await;
        if tokio::time::timeout(Duration::from_secs(5), &mut self.handle)
            .await
            .is_err()
        {
            self.handle.abort();
            let _ = (&mut self.handle).await;
        }
    }
}

impl Drop for ScriptTaskSession {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use lanpath_script::{RecordingNotifier, ScriptDialect, TaskScript};

    use super::*;
    use crate::import::ImportedConfig;

    #[tokio::test]
    async fn cached_tasks_start_list_run_and_stop() {
        let root = tempfile::tempdir().unwrap();
        let source_id = "https://scripts.example/fixture.js";
        RemoteManager::new(root.path().to_path_buf())
            .merge_imported(&ImportedConfig {
                task_scripts: vec![(
                    TaskScript {
                        name: "fixture-task".into(),
                        cron_expr: "0 0 0 1 1 * 2099".into(),
                        source: "$done({ok: $argument, name: $script.name})".into(),
                        dialect: ScriptDialect::Surge,
                        enabled: true,
                        argument: Some("local-value".into()),
                        timeout_ms: Some(1000),
                        source_id: source_id.into(),
                        module_id: None,
                    },
                    source_id.into(),
                )],
                ..ImportedConfig::default()
            })
            .unwrap();

        let session = ScriptTaskSession::start(
            root.path().to_path_buf(),
            Arc::new(RecordingNotifier::new()),
            Arc::new(lanpath_script::MockHttpExecutor::with_responses(vec![])),
        )
        .await
        .unwrap()
        .unwrap();
        let tasks = session.list_tasks();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "fixture-task");
        let output = session.run_now(&tasks[0].id).await.unwrap();
        assert!(output.contains("\"ok\":\"local-value\""), "{output}");
        assert!(session.list_tasks()[0].last_run.is_some());
        session.stop().await;
    }

    #[cfg(feature = "mitm")]
    #[tokio::test]
    async fn imported_modules_with_shared_task_and_asset_survive_cache_and_run_independently() {
        let root = tempfile::tempdir().unwrap();
        let manager = RemoteManager::new(root.path().to_path_buf());
        let source = r#"
            const previous = $persistentStore.read();
            $persistentStore.write($argument);
            $done({argument: $argument, name: $script.name, previous});
        "#;
        for argument in ["first", "second"] {
            let mut imported = crate::import::parse_import(
                &format!(
                    "#!arguments=token:{argument}\n[Script]\nshared-task=type=cron,cronexp=\"0 0 0 1 1 *\",script-path=https://scripts.example/shared.js,argument={{token}},timeout=1"
                ),
                ScriptDialect::Surge,
            )
            .unwrap();
            imported.task_scripts[0].0.source = source.into();
            assert_eq!(manager.merge_imported(&imported).unwrap().tasks, 1);
        }
        let cached = manager.load_cached().unwrap();
        assert!(cached.warnings.is_empty());
        assert_eq!(cached.task_scripts.len(), 2);
        assert_eq!(cached.task_scripts[0].name, cached.task_scripts[1].name);
        assert_eq!(
            cached.task_scripts[0].source_id,
            cached.task_scripts[1].source_id
        );
        assert_ne!(
            cached.task_scripts[0].module_id,
            cached.task_scripts[1].module_id
        );
        let ids: Vec<_> = cached.task_scripts.iter().map(TaskScript::id).collect();
        assert_ne!(ids[0], ids[1]);
        let reloaded_ids: Vec<_> = RemoteManager::new(root.path().to_path_buf())
            .load_cached()
            .unwrap()
            .task_scripts
            .iter()
            .map(TaskScript::id)
            .collect();
        assert_eq!(ids, reloaded_ids);

        let session = ScriptTaskSession::start(
            root.path().to_path_buf(),
            Arc::new(RecordingNotifier::new()),
            Arc::new(lanpath_script::MockHttpExecutor::with_responses(vec![])),
        )
        .await
        .unwrap()
        .unwrap();
        // Exercise manual selection without the real background loop's initial catch-up run.
        session.scheduler().stop().await;
        let views = session.list_tasks();
        assert_eq!(
            views.iter().map(|task| &task.id).collect::<Vec<_>>(),
            ids.iter().collect::<Vec<_>>()
        );
        assert!(session.run_now("shared-task").await.is_err());
        for (index, argument) in ["first", "second"].into_iter().enumerate() {
            let output: serde_json::Value =
                serde_json::from_str(&session.run_now(&ids[index]).await.unwrap()).unwrap();
            assert_eq!(output["argument"], argument);
            assert_eq!(output["name"], "shared-task");
            if index == 1 {
                assert_eq!(output["previous"], "first");
            }
            let records = session.list_tasks();
            assert!(records[index].last_run.is_some());
            assert!(records[index].last_error.is_none());
            if index == 0 {
                assert!(records[1].last_run.is_none());
            }
        }
        session.stop().await;
    }
}
