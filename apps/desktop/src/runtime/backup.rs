use super::DesktopRuntime;
use lanpath_client::backup::Appearance;

impl DesktopRuntime {
    /// Caller has locked editing and settled accepted UI operations. Consuming the
    /// process-local token must succeed before stopping any running service.
    pub(crate) fn prepare_restore(
        &self,
        preview_id: &str,
        appearance: Option<Appearance>,
    ) -> Result<String, String> {
        let input = self
            .backup()?
            .take_preview(preview_id)
            .map_err(|e| e.to_string())?;
        let _ = self.stop()?;
        input
            .prepare(self.business()?.data_dir(), appearance)
            .map_err(|e| e.to_string())
    }
}
