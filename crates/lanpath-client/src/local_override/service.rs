//! Shared local-rule and rule-set business operations.
//!
//! Shells own presentation state, but all validation, metadata preservation,
//! backing-file synchronization, category writes, and smart-update merges live
//! here so Tauri and GPUI cannot diverge.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Mutex,
};

use lanpath_common::{LanpathError, LanpathResult};

use super::{
    CustomRuleSetSource, LocalOverride, LocalOverrideStore, RuleMatchType, RuleSetManager,
    RuleSetUpdateOutcome, RuleSetUpdateStatus,
};

static WRITE_LOCK: Mutex<()> = Mutex::new(());
static CHECK_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Storage-backed service for user route rules and rule-set resources.
#[derive(Debug, Clone)]
pub struct LocalOverrideService {
    data_dir: PathBuf,
}

impl LocalOverrideService {
    /// Bind the service to an application data directory.
    #[must_use]
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    /// Load and migrate the current document.
    pub fn load(&self) -> LanpathResult<LocalOverride> {
        LocalOverrideStore::new(self.data_dir.clone()).load()
    }

    /// Whether a rule set currently has a non-empty backing file.
    #[must_use]
    pub fn is_cached(&self, rule_set: &super::CustomRuleSet) -> bool {
        RuleSetManager::new(self.data_dir.clone()).has_custom_rule_set_file(rule_set)
    }

    /// Save a full replacement while retaining backend-owned update metadata.
    pub fn save(&self, mut requested: LocalOverride) -> LanpathResult<LocalOverride> {
        let _write = write_guard()?;
        let store = LocalOverrideStore::new(self.data_dir.clone());
        let existing = store.load()?;

        requested.ruleset_metadata_version = 1;
        requested.rule_set_subscriptions.clear();
        requested.applied_templates.clear();
        requested.custom_templates.clear();
        requested.builtins_seeded = existing.builtins_seeded;
        for rule_set in &mut requested.custom_rule_sets {
            rule_set.category = rule_set.category.trim().to_owned();
            if let Some(previous) = existing
                .custom_rule_sets
                .iter()
                .find(|previous| previous.id == rule_set.id && previous.source == rule_set.source)
            {
                if matches!(rule_set.source, CustomRuleSetSource::Remote { .. }) {
                    rule_set.last_updated = previous.last_updated;
                }
                rule_set.remote_updated_at = previous.remote_updated_at;
                rule_set.remote_state = previous.remote_state.clone();
            } else {
                if matches!(rule_set.source, CustomRuleSetSource::Remote { .. }) {
                    rule_set.last_updated = 0;
                }
                rule_set.remote_updated_at = 0;
                rule_set.remote_state = Default::default();
            }
        }
        validate_local_override(&requested)?;
        RuleSetManager::new(self.data_dir.clone())
            .sync_custom_rule_set_files(&requested.custom_rule_sets)?;
        store.save(&requested)?;
        store.load()
    }

    /// Assign one category without rewriting resource files or unrelated rules.
    pub fn set_categories(&self, ids: &[String], category: &str) -> LanpathResult<LocalOverride> {
        validate_category(category)?;
        let _write = write_guard()?;
        let store = LocalOverrideStore::new(self.data_dir.clone());
        let mut current = store.load()?;
        if ids.iter().any(|id| {
            !current
                .custom_rule_sets
                .iter()
                .any(|rule_set| &rule_set.id == id)
        }) {
            return Err(client_error("规则集列表已变化，请刷新后重新选择"));
        }
        for rule_set in &mut current.custom_rule_sets {
            if ids.contains(&rule_set.id) {
                rule_set.category = category.trim().to_owned();
            }
        }
        store.save(&current)?;
        Ok(current)
    }

    /// Smart-update every remote rule set and merge only update metadata back.
    pub async fn update_all(&self) -> LanpathResult<(RuleSetUpdateOutcome, LocalOverride)> {
        let _check = CHECK_LOCK.lock().await;
        let mut checked = self.load()?;
        let outcome = RuleSetManager::new(self.data_dir.clone())
            .update_custom_remotes(&mut checked)
            .await?;
        let current = self.save_checked_metadata(&checked)?;
        Ok((outcome, current))
    }

    /// Smart-update one remote rule set and merge only update metadata back.
    pub async fn update_one(
        &self,
        id: &str,
    ) -> LanpathResult<(RuleSetUpdateOutcome, LocalOverride)> {
        let _check = CHECK_LOCK.lock().await;
        let mut checked = self.load()?;
        let rule_set = checked
            .custom_rule_sets
            .iter_mut()
            .find(|rule_set| rule_set.id == id)
            .ok_or_else(|| client_error(format!("rule set not found: {id}")))?;
        let status = RuleSetManager::new(self.data_dir.clone())
            .update_custom_rule_set(rule_set)
            .await;
        let current = self.save_checked_metadata(&checked)?;
        Ok((single_outcome(status), current))
    }

    fn save_checked_metadata(&self, checked: &LocalOverride) -> LanpathResult<LocalOverride> {
        let _write = write_guard()?;
        let store = LocalOverrideStore::new(self.data_dir.clone());
        let mut current = store.load()?;
        for entry in &mut current.custom_rule_sets {
            if let Some(result) = checked
                .custom_rule_sets
                .iter()
                .find(|result| result.id == entry.id && result.source == entry.source)
            {
                entry.last_updated = result.last_updated;
                entry.remote_updated_at = result.remote_updated_at;
                entry.remote_state = result.remote_state.clone();
            }
        }
        store.save(&current)?;
        Ok(current)
    }
}

/// Validate the complete persisted override document.
pub fn validate_local_override(override_: &LocalOverride) -> LanpathResult<()> {
    let mut rule_ids = HashSet::new();
    for rule in &override_.singbox.rules {
        if !rule_ids.insert(rule.id.as_str()) {
            return Err(validation(format!("duplicate rule id '{}'", rule.id)));
        }
        if !matches!(rule.match_type, RuleMatchType::Final) && rule.target.trim().is_empty() {
            return Err(validation(format!("rule '{}' has empty target", rule.id)));
        }
        if matches!(rule.match_type, RuleMatchType::RuleSet)
            && rule
                .target
                .split(',')
                .any(|segment| segment.trim().is_empty())
        {
            return Err(validation(format!(
                "rule '{}' has an empty rule set tag in '{}'",
                rule.id, rule.target
            )));
        }
        if let super::RuleAction::Outbound { tag } = &rule.action
            && tag.trim().is_empty()
        {
            return Err(validation(format!(
                "rule '{}' has an empty outbound tag",
                rule.id
            )));
        }
    }

    let mut ids = HashSet::new();
    let mut tags = HashSet::new();
    for rule_set in &override_.custom_rule_sets {
        if !ids.insert(rule_set.id.as_str()) {
            return Err(validation(format!(
                "duplicate custom rule set id '{}'",
                rule_set.id
            )));
        }
        validate_category(&rule_set.category)?;
        let tag = rule_set.tag.trim();
        if tag.is_empty() {
            return Err(validation(format!(
                "custom rule set '{}' has an empty tag",
                rule_set.id
            )));
        }
        if !tags.insert(tag) {
            return Err(validation(format!("duplicate custom rule set tag '{tag}'")));
        }
        match &rule_set.source {
            CustomRuleSetSource::Remote { url, .. } if url.trim().is_empty() => {
                return Err(validation(format!(
                    "custom rule set '{}' has an empty URL",
                    rule_set.id
                )));
            }
            CustomRuleSetSource::Manual { content } if content.trim().is_empty() => {
                return Err(validation(format!(
                    "custom rule set '{}' has empty content",
                    rule_set.id
                )));
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_category(category: &str) -> LanpathResult<()> {
    if category.chars().count() > 64 || category.chars().any(char::is_control) {
        return Err(validation("分类名称最多 64 个字符，不能包含控制字符"));
    }
    Ok(())
}

fn single_outcome(status: RuleSetUpdateStatus) -> RuleSetUpdateOutcome {
    match status {
        RuleSetUpdateStatus::Updated => RuleSetUpdateOutcome {
            updated: 1,
            ..Default::default()
        },
        RuleSetUpdateStatus::Skipped => RuleSetUpdateOutcome {
            skipped: 1,
            ..Default::default()
        },
        RuleSetUpdateStatus::Failed => RuleSetUpdateOutcome {
            failed: 1,
            ..Default::default()
        },
    }
}

fn write_guard() -> LanpathResult<std::sync::MutexGuard<'static, ()>> {
    WRITE_LOCK
        .lock()
        .map_err(|_| client_error("规则配置写入锁不可用"))
}

fn validation(message: impl Into<String>) -> LanpathError {
    LanpathError::Validation(message.into())
}

fn client_error(message: impl Into<String>) -> LanpathError {
    LanpathError::Client(message.into())
}

/// Build a service without forcing callers to clone a path manually.
#[must_use]
pub fn local_override_service(data_dir: &Path) -> LocalOverrideService {
    LocalOverrideService::new(data_dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_override::{CustomRuleSet, RuleSetFormat};

    fn remote(id: &str) -> CustomRuleSet {
        CustomRuleSet {
            id: id.into(),
            name: "Demo".into(),
            tag: "demo".into(),
            category: String::new(),
            source: CustomRuleSetSource::Remote {
                url: "https://example.invalid/demo.srs".into(),
                format: RuleSetFormat::Binary,
            },
            last_updated: 1,
            remote_updated_at: 0,
            remote_state: Default::default(),
            builtin: false,
        }
    }

    #[test]
    fn checked_metadata_merge_preserves_edits_and_does_not_resurrect_deletions() {
        let dir = tempfile::tempdir().unwrap();
        let service = LocalOverrideService::new(dir.path().into());
        let store = LocalOverrideStore::new(dir.path().into());
        let mut checked = LocalOverride {
            builtins_seeded: true,
            custom_rule_sets: vec![remote("demo")],
            ..Default::default()
        };
        checked.custom_rule_sets[0].remote_state.checked_at = 10;
        let mut current = checked.clone();
        current.custom_rule_sets[0].category = "期间编辑的分类".into();
        current.custom_rule_sets[0].remote_state.checked_at = 99_999_999_999;
        store.save(&current).unwrap();

        service.save_checked_metadata(&checked).unwrap();
        let result = store.load().unwrap();
        assert_eq!(result.custom_rule_sets[0].category, "期间编辑的分类");
        assert_eq!(result.custom_rule_sets[0].remote_state.checked_at, 10);

        current.custom_rule_sets.clear();
        store.save(&current).unwrap();
        service.save_checked_metadata(&checked).unwrap();
        assert!(store.load().unwrap().custom_rule_sets.is_empty());
    }
}
