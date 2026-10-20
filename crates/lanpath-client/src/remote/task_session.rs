use std::{path::PathBuf, sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use lanpath_common::{LanpathError, LanpathResult};
use lanpath_script::{
    FilePersistentStore, Notifier, ScriptHost, ScriptLimits, ScriptScheduler, TaskScriptView,
};
use tokio::task::JoinHandle;

use crate::http_exec::ReqwestHttpExecutor;

use super::RemoteManager;

/// Runtime presentation of one scheduled script task.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RemoteTaskView {
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

/// One running scheduler session. Dropping it without [`Self::stop`] detaches the task.
pub struct ScriptTaskSession {
    scheduler: Arc<ScriptScheduler>,
    handle: JoinHandle<()>,
}

impl ScriptTaskSession {
    /// Build and start a scheduler from cached remote snippets.
    ///
    /// Returns `None` when no enabled task has a valid cron expression.
    pub async fn start(
        data_dir: PathBuf,
        notifier: Arc<dyn Notifier>,
    ) -> LanpathResult<Option<Self>> {
        let tasks = RemoteManager::new(data_dir.clone())
            .load_cached()?
            .task_scripts;
        let host = Arc::new(ScriptHost::new(
            Arc::new(ReqwestHttpExecutor::new()),
            Arc::new(FilePersistentStore::new(data_dir.join("script_store"))),
            notifier,
        ));
        let mut scheduler = ScriptScheduler::new(host, ScriptLimits::default());
        let mut registered = 0usize;
        for task in tasks.into_iter().filter(|task| task.enabled) {
            match scheduler.add_task(task) {
                Ok(()) => registered += 1,
                Err(error) => {
                    tracing::warn!(%error, "skip scheduled task with invalid cron expression")
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

    pub async fn run_now(&self, name: &str) -> LanpathResult<String> {
        if name.trim().is_empty() {
            return Err(LanpathError::Validation("任务名不能为空".into()));
        }
        self.scheduler
            .run_now(name)
            .await
            .map(|output| output.0.to_string())
    }

    pub async fn stop(self) {
        self.scheduler.stop().await;
        let mut handle = self.handle;
        if tokio::time::timeout(Duration::from_secs(5), &mut handle)
            .await
            .is_err()
        {
            handle.abort();
            let _ = handle.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use lanpath_script::{RecordingNotifier, ScriptDialect, TaskScript};

    use super::*;
    use crate::remote::CachedRemoteConfig;

    #[tokio::test]
    async fn cached_tasks_start_list_run_and_stop() {
        let root = tempfile::tempdir().unwrap();
        let cache_dir = root.path().join("remote_cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let cache = CachedRemoteConfig {
            task_scripts: vec![TaskScript {
                name: "fixture-task".into(),
                cron_expr: "0 0 0 1 1 * 2099".into(),
                source: "$done({ok: true})".into(),
                dialect: ScriptDialect::Surge,
                enabled: true,
            }],
            ..CachedRemoteConfig::default()
        };
        std::fs::write(
            cache_dir.join("fixture.json"),
            serde_json::to_vec(&cache).unwrap(),
        )
        .unwrap();

        let session = ScriptTaskSession::start(
            root.path().to_path_buf(),
            Arc::new(RecordingNotifier::new()),
        )
        .await
        .unwrap()
        .unwrap();
        let tasks = session.list_tasks();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "fixture-task");
        let output = session.run_now("fixture-task").await.unwrap();
        assert!(output.contains("\"ok\":true"), "{output}");
        assert!(session.list_tasks()[0].last_run.is_some());
        session.stop().await;
    }
}
