use std::collections::HashSet;

use lanpath_client::local_override::{LocalOverride, LocalOverrideService, RuleSetUpdateOutcome};
use lanpath_common::LanpathResult;

use super::BusinessService;

/// Persisted rule document plus filesystem-derived cache state.
#[derive(Clone)]
pub(crate) struct LocalOverrideDocument {
    pub(crate) value: LocalOverride,
    pub(crate) cached_rule_sets: HashSet<String>,
}

impl BusinessService {
    pub(crate) fn local_override(&self) -> LanpathResult<LocalOverrideDocument> {
        let service = self.local_override_service();
        let value = service.load()?;
        Ok(document(&service, value))
    }

    pub(crate) fn save_local_override(
        &self,
        value: LocalOverride,
    ) -> LanpathResult<LocalOverrideDocument> {
        let service = self.local_override_service();
        let value = service.save(value)?;
        Ok(document(&service, value))
    }

    pub(crate) async fn update_rule_sets(
        &self,
    ) -> LanpathResult<(RuleSetUpdateOutcome, LocalOverrideDocument)> {
        let service = self.local_override_service();
        let (outcome, value) = service.update_all().await?;
        Ok((outcome, document(&service, value)))
    }

    pub(crate) async fn update_rule_set(
        &self,
        id: &str,
    ) -> LanpathResult<(RuleSetUpdateOutcome, LocalOverrideDocument)> {
        let service = self.local_override_service();
        let (outcome, value) = service.update_one(id).await?;
        Ok((outcome, document(&service, value)))
    }

    fn local_override_service(&self) -> LocalOverrideService {
        LocalOverrideService::new(self.root.clone())
    }
}

fn document(service: &LocalOverrideService, value: LocalOverride) -> LocalOverrideDocument {
    let cached_rule_sets = value
        .custom_rule_sets
        .iter()
        .filter(|rule_set| service.is_cached(rule_set))
        .map(|rule_set| rule_set.id.clone())
        .collect();
    LocalOverrideDocument {
        value,
        cached_rule_sets,
    }
}
