//! MITM-related methods for [`ClientState`].

use lanpath_common::LanpathResult;

use crate::{core_config, state::ClientState};

impl ClientState {
    pub(crate) async fn start_mitm_chain(
        &mut self,
    ) -> LanpathResult<Option<core_config::MitmChain>> {
        let session = self
            .mitm_service
            .start(&self.config, self.notifier.clone())
            .await?;
        let chain = session.as_ref().map(|session| session.chain());
        self.mitm = session;
        Ok(chain)
    }

    pub(crate) async fn rollback_mitm_started(&mut self) {
        self.stop_mitm().await;
        self.stop_scheduler().await;
    }

    pub(crate) async fn stop_mitm(&mut self) {
        if let Some(session) = self.mitm.take() {
            session.stop();
        }
    }
}
