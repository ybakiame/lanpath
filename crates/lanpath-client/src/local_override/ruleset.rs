//! Rule set download and cache management for user-defined custom rule sets.
//!
//! 自「废弃内置规则集订阅」起只管理用户自控的 [`CustomRuleSet`]（remote URL
//! 下载 / manual JSON 落盘）；内置社区订阅的下载/缓存/引用生成已全部移除。
//!
//! ADR-0002, section 3.4.

use std::path::PathBuf;

use lanpath_common::LanpathResult;

use super::{CustomRuleSet, CustomRuleSetSource, LocalOverride, RuleSetFormat};

/// Custom rule set storage root under data_dir (`rulesets/custom/`).
pub const CUSTOM_RULE_SET_ROOT_DIR: &str = "rulesets";
/// Custom rule set sub-directory under [`CUSTOM_RULE_SET_ROOT_DIR`].
pub const CUSTOM_RULE_SET_SUB_DIR: &str = "custom";

/// Result of updating a single custom rule set (smart skip).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSetUpdateStatus {
    /// Remote content changed (or no usable local cache): downloaded,
    /// `last_updated` bumped to the download completion time.
    Updated,
    /// Conditional 304 or identical content: cache write skipped.
    Skipped,
    /// Request or download failed (best-effort; never fatal to a batch).
    Failed,
}

/// Aggregated outcome of updating one or more custom rule sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RuleSetUpdateOutcome {
    /// Remote had newer content: downloaded + `last_updated` bumped.
    pub updated: usize,
    /// Conditional 304 or identical content: cache write skipped.
    pub skipped: usize,
    /// Request / download failed (best-effort, per-entry).
    pub failed: usize,
}

/// Rule set manager handles download and file sync of custom rule sets.
#[derive(Debug, Clone)]
pub struct RuleSetManager {
    data_dir: PathBuf,
}

impl RuleSetManager {
    /// Create manager based on data directory.
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    /// Custom rule set storage directory: `data_dir/rulesets/custom/`.
    pub fn custom_rule_set_dir(&self) -> PathBuf {
        self.data_dir
            .join(CUSTOM_RULE_SET_ROOT_DIR)
            .join(CUSTOM_RULE_SET_SUB_DIR)
    }

    /// Backing file path for a custom rule set:
    /// `<custom_dir>/<id>.srs` (binary) or `<custom_dir>/<id>.json` (source).
    pub fn custom_rule_set_file_path(&self, id: &str, format: RuleSetFormat) -> PathBuf {
        let ext = match format {
            RuleSetFormat::Source => "json",
            RuleSetFormat::Binary => "srs",
        };
        self.custom_rule_set_dir().join(format!("{id}.{ext}"))
    }

    /// Whether the backing file of a custom rule set exists on disk.
    ///
    /// - Remote: cache file for its declared format must exist.
    /// - Manual: the persisted `<id>.json` file must exist.
    pub fn has_custom_rule_set_file(&self, rs: &CustomRuleSet) -> bool {
        let format = rs.file_format();
        std::fs::metadata(self.custom_rule_set_file_path(&rs.id, format))
            .is_ok_and(|meta| meta.is_file() && meta.len() > 0)
    }

    /// Persist Manual custom rule set contents to `<custom_dir>/<id>.json`
    /// and best-effort remove backing files of custom rule sets that are no
    /// longer present in `custom_sets` (deleted rule sets or format switches).
    ///
    /// Called on save so the injected local rule_set paths always resolve.
    pub fn sync_custom_rule_set_files(&self, custom_sets: &[CustomRuleSet]) -> LanpathResult<()> {
        let dir = self.custom_rule_set_dir();
        std::fs::create_dir_all(&dir)?;

        // Collect the exact file names the given list should keep on disk.
        let keep: std::collections::HashSet<String> = custom_sets
            .iter()
            .map(|rs| {
                let path = self.custom_rule_set_file_path(&rs.id, rs.file_format());
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();

        // Write manual contents (overwrites on re-save with edited content).
        for rs in custom_sets {
            if let CustomRuleSetSource::Manual { content } = &rs.source {
                let path = self.custom_rule_set_file_path(&rs.id, RuleSetFormat::Source);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&path, content)?;
            }
        }

        // Best-effort cleanup of orphaned backing files (deleted rule sets and
        // stale files left behind by a source kind/format switch).
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !keep.contains(&name)
                    && let Err(e) = std::fs::remove_file(entry.path())
                {
                    tracing::warn!(
                        path = %entry.path().display(),
                        error = %e,
                        "failed to remove orphaned custom rule set file"
                    );
                }
            }
        }

        Ok(())
    }

    /// Recreate a missing manual file from its saved configuration, without deleting other files.
    pub fn restore_missing_manual_rule_set(&self, rs: &CustomRuleSet) -> LanpathResult<()> {
        if let CustomRuleSetSource::Manual { content } = &rs.source
            && !self.has_custom_rule_set_file(rs)
        {
            std::fs::create_dir_all(self.custom_rule_set_dir())?;
            std::fs::write(
                self.custom_rule_set_file_path(&rs.id, RuleSetFormat::Source),
                content,
            )?;
        }
        Ok(())
    }

    /// Download a Remote custom rule set to `<custom_dir>/<id>.<ext>`.
    ///
    /// No-op for Manual sources. On failure preserves any existing file.
    pub async fn download_custom_rule_set(&self, rs: &CustomRuleSet) -> LanpathResult<()> {
        let CustomRuleSetSource::Remote { url, format } = &rs.source else {
            return Ok(());
        };
        let path = self.custom_rule_set_file_path(&rs.id, *format);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        self.download_file(url, &path).await
    }

    /// Check with origin-bound ETag/Last-Modified conditional GET; compare bytes on 200.
    /// A successful unchanged check advances checked_at, not the local write timestamp.
    /// Errors and empty responses preserve the previous cache and metadata.
    pub async fn update_custom_rule_set(&self, rs: &mut CustomRuleSet) -> RuleSetUpdateStatus {
        let path = self.custom_rule_set_file_path(&rs.id, rs.file_format());
        match super::ruleset_update::update(&self.data_dir, &path, rs).await {
            Ok(status) => status,
            Err(error) => {
                tracing::warn!(id = %rs.id, error = %error, "规则集更新失败，保留原缓存和检查信息");
                RuleSetUpdateStatus::Failed
            }
        }
    }

    /// Manually update all Remote custom rule sets now (smart skip).
    ///
    /// 对每个 custom Remote 走 [`Self::update_custom_rule_set`]（条件请求及内容校验），
    /// 汇总 updated / skipped / failed。命令层同步 await 全部完成后才返回，保证前端
    /// invalidate 重拉 `local_override_get` 时即可看到 `cached` / `last_updated` /
    /// `remote_updated_at` 变化。Manual 无远端源不计入；单个失败仅告警，不影响整批
    /// （best-effort）。
    pub async fn update_custom_remotes(
        &self,
        ovr: &mut LocalOverride,
    ) -> LanpathResult<RuleSetUpdateOutcome> {
        let mut outcome = RuleSetUpdateOutcome::default();
        for rs in ovr
            .custom_rule_sets
            .iter_mut()
            .filter(|rs| matches!(rs.source, CustomRuleSetSource::Remote { .. }))
        {
            match self.update_custom_rule_set(rs).await {
                RuleSetUpdateStatus::Updated => outcome.updated += 1,
                RuleSetUpdateStatus::Skipped => outcome.skipped += 1,
                RuleSetUpdateStatus::Failed => outcome.failed += 1,
            }
        }
        Ok(outcome)
    }

    /// Download a file from URL to path.
    async fn download_file(&self, url: &str, path: &std::path::Path) -> LanpathResult<()> {
        let bytes =
            crate::fetch_resource_bytes(&self.data_dir, url, std::time::Duration::from_secs(60))
                .await?;
        std::fs::write(path, bytes)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/ruleset_tests.rs"]
mod tests;
