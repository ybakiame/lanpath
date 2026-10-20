//! Scheduler-related methods for [`ClientState`].

use lanpath_common::LanpathResult;

use crate::{remote::ScriptTaskSession, state::ClientState};

impl ClientState {
    /// Restart the scheduled-task session from the current remote cache.
    ///
    /// The scheduler is independent from MITM, but follows the client lifecycle. Invalid cron
    /// entries are skipped by [`ScriptTaskSession`]; no valid enabled tasks leaves it stopped.
    pub async fn reload_scheduler_from_cache(&mut self) -> LanpathResult<()> {
        self.stop_scheduler().await;
        self.scheduler = ScriptTaskSession::start(
            self.config.data_dir.clone(),
            std::sync::Arc::clone(&self.notifier),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn start_scheduler_from_cache(&mut self) -> LanpathResult<()> {
        self.reload_scheduler_from_cache().await
    }

    /// Stop the scheduler and wait for its background loop to exit.
    pub(crate) async fn stop_scheduler(&mut self) {
        if let Some(session) = self.scheduler.take() {
            session.stop().await;
        }
    }
}
