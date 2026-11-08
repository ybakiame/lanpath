//! Remote resource manager: manifest I/O, periodic fetching, and cache reading.

use std::path::PathBuf;

use lanpath_common::{LanpathError, LanpathResult};

#[cfg(feature = "mitm")]
use crate::import::{ConfigMeta, ImportedConfig, parse_import};

use super::{FetchReport, RemoteFetchContext, RemoteKind, RemoteResource, safe_name};

/// Remote resource manager: responsible for remotes manifest I/O, periodic fetching, and cache reading.
#[derive(Debug, Clone)]
pub struct RemoteManager {
    pub(super) data_dir: PathBuf,
}

impl RemoteManager {
    /// Create manager from data directory.
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    /// GPUI manifest document (`app.toml`); other shells use `remotes.json`.
    pub fn remotes_file(&self) -> PathBuf {
        if crate::resource_storage::unified(&self.data_dir) {
            crate::settings::app_settings_path(&self.data_dir)
        } else {
            self.data_dir.join("remotes.json")
        }
    }

    /// Read remote resource manifest; returns empty list when file does not exist.
    pub fn load(&self) -> LanpathResult<Vec<RemoteResource>> {
        if crate::resource_storage::unified(&self.data_dir) {
            return Ok(crate::resource_storage::load(&self.data_dir)?
                .remotes()
                .to_vec());
        }
        let path = self.remotes_file();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let text = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Save the manifest through the selected shell storage layout.
    pub fn save(&self, remotes: &[RemoteResource]) -> LanpathResult<()> {
        self.save_with_context(remotes, None)
    }

    fn save_with_context(
        &self,
        remotes: &[RemoteResource],
        context: Option<&RemoteFetchContext>,
    ) -> LanpathResult<()> {
        if crate::resource_storage::unified(&self.data_dir) {
            crate::resource_storage::load(&self.data_dir)?;
            return crate::settings::update_app_section::<crate::resource_storage::ResourceSettings>(
                &self.data_dir,
                "resources",
                |settings| {
                    settings.set_remotes(remotes.to_vec());
                    Ok(())
                },
                || match context {
                    Some(context) => context.check_write(&self.remotes_file()),
                    None => Ok(()),
                },
            );
        }
        let text = serde_json::to_string_pretty(remotes)?;
        super::refresh::atomic_write_with_context(&self.remotes_file(), text.as_bytes(), context)?;
        Ok(())
    }

    /// Locate and fully update a remote resource by name (url / kind / dialect / update_interval_secs /
    /// enabled / description / argument_values / icon / arguments), keeping existing cache
    /// (`remote_cache/<name>.json` / `scripts/<name>.js` not deleted, overwritten on next fetch).
    /// Errors when resource does not exist.
    pub fn update_resource(&self, name: &str, updated: RemoteResource) -> LanpathResult<()> {
        let mut remotes = self.load()?;
        let idx = remotes
            .iter()
            .position(|r| r.name == name)
            .ok_or_else(|| LanpathError::Client(format!("远程资源 '{name}' 不存在")))?;
        // name is the lookup key, keep original name (do not rename on update).
        let mut entry = updated;
        entry.name = name.to_string();
        remotes[idx] = entry;
        self.save(&remotes)
    }

    /// Fetch enabled resources; diagnostics deliberately omit URLs and downloaded content.
    pub async fn fetch_all(&self, remotes: &[RemoteResource]) -> FetchReport {
        let mut report = FetchReport::default();
        for remote in remotes.iter().filter(|resource| resource.enabled) {
            let fetched = self
                .fetch_one(remote, None)
                .await
                .unwrap_or_else(|_| FetchReport {
                    warnings: vec!["resource update refused; previous cache retained".into()],
                    ..Default::default()
                });
            report.fetched += fetched.fetched;
            report.scripts += fetched.scripts;
            report.rewrites += fetched.rewrites;
            report.tasks += fetched.tasks;
            report.hostnames += fetched.hostnames;
            report.warnings.extend(fetched.warnings);
        }
        report
    }

    pub(super) async fn fetch_one(
        &self,
        remote: &RemoteResource,
        context: Option<&RemoteFetchContext>,
    ) -> LanpathResult<FetchReport> {
        let automatic = context.is_some();
        self.check_publication(remote, context)?;
        // Decoration must not consume the automatic resource's entire deadline.
        // Icon warmup remains part of explicit save/manual-update operations.
        let mut report = FetchReport::default();
        // All network awaits precede the final script/snippet publication, so
        // cancellation during an auxiliary asset request cannot publish a partial module.
        if !automatic
            && let Some(icon) = &remote.icon
            && self.cache_icon(&remote.name, icon).await.is_err()
        {
            report
                .warnings
                .push("resource icon could not be cached".into());
        }
        match remote.kind {
            RemoteKind::Script => match self.fetch_text_with_context(&remote.url, context).await {
                Ok(text) => {
                    self.check_publication(remote, context)?;
                    match self.write_script(&remote.name, &text, context) {
                        Ok(_) => {
                            report.fetched += 1;
                            report.scripts += 1;
                        }
                        Err(_) => report
                            .warnings
                            .push("script cache could not be written; resource not updated".into()),
                    }
                }
                Err(_) => report
                    .warnings
                    .push("script download failed; previous cache retained".into()),
            },
            #[cfg(feature = "mitm")]
            RemoteKind::Snippet => match self.fetch_snippet(remote, context).await {
                Ok((imported, complete)) => {
                    report.warnings.extend(imported.warnings.iter().cloned());
                    if automatic && !complete {
                        report.warnings.push(
                            "automatic module refresh incomplete; previous complete cache retained"
                                .into(),
                        );
                        if let Some(context) = context {
                            context.check_live()?;
                        }
                        return Ok(report);
                    }
                    self.check_publication(remote, context)?;
                    let (hostname_count, warnings) = match self
                        .write_cache(remote, &imported, context)
                    {
                        Ok(result) => result,
                        Err(_) => {
                            report.warnings.push(
                                "snippet cache could not be written; resource not updated".into(),
                            );
                            return Ok(report);
                        }
                    };
                    report.rewrites += imported.rewrites.len();
                    report.scripts += imported.scripts.iter().filter(|rule| rule.enabled).count();
                    report.tasks += imported
                        .task_scripts
                        .iter()
                        .filter(|(task, _)| task.enabled)
                        .count();
                    report.hostnames += hostname_count;
                    report.warnings.extend(warnings);
                    if self
                        .backfill_arguments(&remote.name, &imported.meta, context)
                        .is_err()
                    {
                        report
                            .warnings
                            .push("module parameter declarations could not be saved".into());
                    }
                    report.fetched += 1;
                }
                Err(_) => report
                    .warnings
                    .push("snippet download or parse failed; previous cache retained".into()),
            },
            #[cfg(not(feature = "mitm"))]
            RemoteKind::Snippet => report.warnings.push(
                "snippet parsing requires the desktop MITM feature; resource not activated".into(),
            ),
        }
        if let Some(context) = context {
            context.check_live()?;
        }
        Ok(report)
    }

    pub(super) fn check_publication(
        &self,
        remote: &RemoteResource,
        context: Option<&RemoteFetchContext>,
    ) -> LanpathResult<()> {
        if let Some(context) = context {
            context.check_write(&crate::resource_storage::cache_path(
                &self.data_dir,
                "remote_refresh.json",
            ))?;
            match remote.kind {
                RemoteKind::Script => context.check_write(
                    &crate::resource_storage::cache_path(&self.data_dir, "scripts")
                        .join(format!("{}.js", safe_name(&remote.name))),
                )?,
                RemoteKind::Snippet => {
                    context.check_write(&self.remotes_file())?;
                    #[cfg(feature = "mitm")]
                    context.check_write(
                        &crate::resource_storage::cache_path(&self.data_dir, "remote_cache").join(
                            format!(
                                "{}.json",
                                super::activation::remote_module_id(remote).replace(':', "-"),
                            ),
                        ),
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Fetch single URL text content; non-2xx treated as failure.
    ///
    /// Delegates to [`crate::fetch_resource_text`]: GitHub blob/raw link normalization,
    /// GitHub proxy prefix拼接 and "use local proxy" settings are all handled uniformly (30s timeout).
    pub(super) async fn fetch_text(&self, url: &str) -> LanpathResult<String> {
        crate::fetch_resource_text(&self.data_dir, url, std::time::Duration::from_secs(30)).await
    }

    pub(super) async fn fetch_text_with_context(
        &self,
        url: &str,
        context: Option<&RemoteFetchContext>,
    ) -> LanpathResult<String> {
        match context {
            Some(context) => context.fetch_text(url).await,
            None => self.fetch_text(url).await,
        }
    }

    /// Fetch backfill resource parameter declarations (auxiliary path): when manifest resource `arguments`
    /// is empty but remote meta declares parameters, backfill `arguments` and pre-fill `argument_values`
    /// with defaults (existing values in `argument_values` are not overwritten).
    #[cfg(feature = "mitm")]
    fn backfill_arguments(
        &self,
        name: &str,
        meta: &ConfigMeta,
        context: Option<&RemoteFetchContext>,
    ) -> LanpathResult<()> {
        if meta.arguments.is_empty() {
            return Ok(());
        }
        let mut remotes = self.load()?;
        let Some(remote) = remotes.iter_mut().find(|r| r.name == name) else {
            // Resource has been deleted, skip backfill.
            return Ok(());
        };
        if !remote.arguments.is_empty() {
            // Already has parameter declarations (passed in when adding), do not overwrite.
            return Ok(());
        }
        for spec in &meta.arguments {
            if !remote.argument_values.iter().any(|(k, _)| k == &spec.key) {
                remote
                    .argument_values
                    .push((spec.key.clone(), spec.default_value.clone()));
            }
        }
        remote.arguments = meta.arguments.clone();
        self.save_with_context(&remotes, context)
    }

    /// Fetch Snippet: parse → backfill script hooks/task sources one by one (failure recorded in warning and skipped).
    #[cfg(feature = "mitm")]
    async fn fetch_snippet(
        &self,
        remote: &RemoteResource,
        context: Option<&RemoteFetchContext>,
    ) -> LanpathResult<(ImportedConfig, bool)> {
        let text = self.fetch_text_with_context(&remote.url, context).await?;
        let mut imported = parse_import(&text, remote.dialect).map_err(|e| {
            LanpathError::Client(format!("snippet '{}' parse failed: {e}", remote.name))
        })?;
        // Source backfill removes only failed enabled assets, not unsupported
        // directives rejected by the parser. Do not treat unrelated warnings as fatal.
        let hooks = imported.scripts.iter().filter(|rule| rule.enabled).count();
        let tasks = imported
            .task_scripts
            .iter()
            .filter(|(task, _)| task.enabled)
            .count();
        self.fill_script_sources_with_context(&mut imported, context)
            .await;
        // Empty downloads are not complete module updates; explicit manual
        // refresh keeps its existing diagnostic/partial-import semantics.
        let complete = !text.trim().is_empty()
            && hooks == imported.scripts.iter().filter(|rule| rule.enabled).count()
            && tasks
                == imported
                    .task_scripts
                    .iter()
                    .filter(|(task, _)| task.enabled)
                    .count();
        Ok((imported, complete))
    }

    /// Destination for a downloaded pure-JS resource (not a local user script).
    pub fn script_file(&self, name: &str) -> PathBuf {
        crate::resource_storage::cache_path(&self.data_dir, "scripts")
            .join(format!("{}.js", safe_name(name)))
    }

    /// Write pure JS script to `data_dir/scripts/<name>.js`.
    fn write_script(
        &self,
        name: &str,
        content: &str,
        context: Option<&RemoteFetchContext>,
    ) -> LanpathResult<PathBuf> {
        let dir = crate::resource_storage::cache_path(&self.data_dir, "scripts");
        let path = dir.join(format!("{}.js", safe_name(name)));
        super::refresh::atomic_write_with_context(&path, content.as_bytes(), context)?;
        Ok(path)
    }

    /// Serialize Snippet aggregation result to cache `data_dir/remote_cache/<name>.json`.
    #[cfg(feature = "mitm")]
    fn write_cache(
        &self,
        remote: &RemoteResource,
        imported: &ImportedConfig,
        context: Option<&RemoteFetchContext>,
    ) -> LanpathResult<(usize, Vec<String>)> {
        let dir = crate::resource_storage::cache_path(&self.data_dir, "remote_cache");
        let module_id = super::activation::remote_module_id(remote);
        let mut cached = super::CachedRemoteConfig::from_imported(imported);
        cached.bind_module(&module_id);
        let (hostnames, warnings) = cached.resolved_hostnames(Some(remote));
        let text = serde_json::to_string_pretty(&cached)?;
        let path = dir.join(format!("{}.json", module_id.replace(':', "-")));
        super::refresh::atomic_write_with_context(&path, text.as_bytes(), context)?;
        Ok((hostnames.len(), warnings))
    }

    /// Find `<safe_name(name)>.<ext>` icon cache under `data_dir/icons/`; return path when exists,
    /// otherwise `None`. ext must be in [`ICON_EXTENSIONS`] whitelist (case-insensitive).
    pub fn icon_file(&self, name: &str) -> Option<PathBuf> {
        let dir = crate::resource_storage::cache_path(&self.data_dir, "icons");
        let base = safe_name(name);
        for entry in std::fs::read_dir(&dir).ok()?.flatten() {
            let path = entry.path();
            if path.file_stem().and_then(|s| s.to_str()) != Some(base.as_str()) {
                continue;
            }
            let Some(ext) = path.extension().and_then(|x| x.to_str()) else {
                continue;
            };
            if super::ICON_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) {
                return Some(path);
            }
        }
        None
    }

    /// Download remote resource icon to local cache `data_dir/icons/<safe_name>.<ext>`.
    ///
    /// Delegates to [`crate::fetch_resource_bytes`] (GitHub proxy prefix / use local proxy /
    /// 30s timeout consistent with text fetch). Extension priority: URL path suffix ([`ICON_EXTENSIONS`]
    /// whitelist) → infer image format from response bytes ([`icon_ext_from_bytes`]) → `.img`.
    /// Before writing, delete other old extension files for this name (no残留 after icon source/format change).
    ///
    /// Returns written path; returns `None` when response is empty (no icon content). Caller should
    /// best-effort handle return value, failure does not block main flow.
    pub async fn cache_icon(&self, name: &str, url: &str) -> LanpathResult<Option<PathBuf>> {
        let bytes =
            crate::fetch_resource_bytes(&self.data_dir, url, std::time::Duration::from_secs(30))
                .await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        let ext = super::icon_ext_from_url(url)
            .or_else(|| super::icon_ext_from_bytes(&bytes))
            .unwrap_or("img");

        let dir = crate::resource_storage::cache_path(&self.data_dir, "icons");
        std::fs::create_dir_all(&dir)?;
        let base = safe_name(name);
        let path = dir.join(format!("{base}.{ext}"));
        super::refresh::atomic_write(&path, &bytes)?;
        // Delete other old extension files for this name (keep this write target).
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.file_stem().and_then(|s| s.to_str()) == Some(base.as_str())
                    && path.extension().and_then(|x| x.to_str()) != Some(ext)
                {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        Ok(Some(path))
    }
}
