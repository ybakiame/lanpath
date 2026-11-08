//! Source fetching and explicit, independently scoped local import collections.
use super::{CachedRemoteConfig, ImportSummary, RemoteManager};
use crate::import::{ImportedConfig, parse_import};
use lanpath_common::LanpathResult;
use lanpath_script::ScriptDialect;

impl RemoteManager {
    pub fn merge_imported(&self, imported: &ImportedConfig) -> LanpathResult<ImportSummary> {
        let mut summary = ImportSummary {
            warnings: imported.warnings.clone(),
            meta: imported.meta.clone(),
            ..Default::default()
        };
        let id = uuid::Uuid::new_v4().to_string();
        let mut cached = CachedRemoteConfig::from_imported(imported);
        cached.bind_module(&format!("local:{id}"));
        cached.scripts.retain(|rule| {
            if rule.source.is_empty() || rule.source_id.is_empty() {
                summary
                    .warnings
                    .push("hook source not fetched or unattributed; not activated".into());
                false
            } else {
                true
            }
        });
        cached.task_scripts.retain(|task| {
            if task.source.is_empty() || task.source_id.is_empty() {
                summary
                    .warnings
                    .push("task source not fetched or unattributed; not activated".into());
                false
            } else {
                true
            }
        });
        summary.scripts = cached.scripts.len();
        summary.tasks = cached.task_scripts.len();
        summary.rewrites = cached.rewrites.len();
        summary.hostnames = cached.resolved_hostnames(None).0.len();
        let cache_dir = crate::resource_storage::imports_path(&self.data_dir);
        let collection_dir = cache_dir.join("local");
        std::fs::create_dir_all(&collection_dir)?;
        let index = cache_dir.join("local_imports.json");
        let mut ids: Vec<String> = if index.exists() {
            // Never replace an unreadable registry with a fabricated empty collection.
            serde_json::from_str(&std::fs::read_to_string(&index)?)?
        } else {
            Vec::new()
        };
        std::fs::write(
            collection_dir.join(format!("{id}.json")),
            serde_json::to_string_pretty(&cached)?,
        )?;
        ids.push(id);
        std::fs::write(index, serde_json::to_string_pretty(&ids)?)?;
        Ok(summary)
    }

    pub async fn import_content(
        &self,
        content: &str,
        dialect: ScriptDialect,
    ) -> LanpathResult<ImportSummary> {
        let mut imported = parse_import(content, dialect)?;
        self.fill_script_sources(&mut imported).await;
        let mut summary = self.merge_imported(&imported)?;
        summary.meta = imported.meta;
        Ok(summary)
    }

    pub async fn fill_script_sources(&self, imported: &mut ImportedConfig) {
        self.fill_script_sources_with_context(imported, None).await;
    }

    pub(super) async fn fill_script_sources_with_context(
        &self,
        imported: &mut ImportedConfig,
        context: Option<&super::RemoteFetchContext>,
    ) {
        let mut kept_rules = Vec::new();
        let mut kept_urls = Vec::new();
        // Identity joins, not positional zipping: an unsupported local hook can never
        // steal the source belonging to the next remote hook.
        let urls = std::mem::take(&mut imported.script_urls);
        for mut rule in std::mem::take(&mut imported.scripts) {
            if !rule.enabled {
                if let Some(pair) = urls
                    .iter()
                    .find(|(name, url)| name == &rule.name && url == &rule.source_id)
                {
                    kept_urls.push(pair.clone());
                }
                kept_rules.push(rule);
                continue;
            }
            let associated = urls
                .iter()
                .find(|(name, url)| name == &rule.name && url == &rule.source_id);
            let Some((name, url)) = associated else {
                imported.warnings.push(
                    "hook lacks an unambiguous remote source association; not activated".into(),
                );
                continue;
            };
            match self.fetch_text_with_context(url, context).await {
                Ok(source) if plain_script(&source) => {
                    rule.source = source;
                    kept_urls.push((name.clone(), url.clone()));
                    kept_rules.push(rule);
                }
                _ => imported.warnings.push(
                    "hook source download failed or is not plaintext JavaScript; not activated"
                        .into(),
                ),
            }
        }
        imported.scripts = kept_rules;
        imported.script_urls = kept_urls;
        let mut kept_tasks = Vec::new();
        for (mut task, url) in std::mem::take(&mut imported.task_scripts) {
            if !task.enabled {
                kept_tasks.push((task, url));
                continue;
            }
            if task.source_id != url {
                imported
                    .warnings
                    .push("task source identity does not match its asset; not activated".into());
                continue;
            }
            match self.fetch_text_with_context(&url, context).await {
                Ok(source) if plain_script(&source) => {
                    task.source = source;
                    kept_tasks.push((task, url));
                }
                _ => imported.warnings.push(
                    "task source download failed or is not plaintext JavaScript; not activated"
                        .into(),
                ),
            }
        }
        imported.task_scripts = kept_tasks;
    }
}

fn plain_script(source: &str) -> bool {
    let lower = source.trim_start().to_ascii_lowercase();
    !lower.is_empty()
        && !lower.starts_with("<!doctype html")
        && !lower.starts_with("<html")
        && !lower.starts_with("-----begin")
        && !lower.starts_with("#!encrypted")
}
