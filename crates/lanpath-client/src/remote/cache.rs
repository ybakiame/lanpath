//! Versioned cache records. Legacy records remain recoverable but never invent hook identity.
use crate::import::ConfigMeta;
#[cfg(feature = "mitm")]
use crate::import::ImportedConfig;
use lanpath_common::{LanpathError, LanpathResult};
use lanpath_script::TaskScript;
use serde::{Deserialize, Serialize};

pub const REMOTE_CACHE_VERSION: u32 = 2;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CachedRemoteConfig {
    pub version: u32,
    /// Exact enabled manifest identity, or an explicitly registered local collection ID.
    pub module_id: String,
    #[cfg(feature = "mitm")]
    pub rewrites: Vec<super::CachedRewriteRule>,
    #[cfg(feature = "mitm")]
    pub scripts: Vec<super::CachedScriptRule>,
    pub task_scripts: Vec<TaskScript>,
    pub hostnames: Vec<String>,
    /// Raw parameterized hostname declarations; absent in older v2 caches.
    /// Static hostnames remain authoritative only for non-parameterized lines.
    #[serde(default)]
    pub mitm_templates: Vec<String>,
    pub meta: Option<ConfigMeta>,
    /// Durable, editable GPUI local-resource source; absent in legacy imports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local: Option<super::LocalSource>,
}

impl CachedRemoteConfig {
    #[cfg(feature = "mitm")]
    pub fn from_imported(imported: &ImportedConfig) -> Self {
        Self {
            version: REMOTE_CACHE_VERSION,
            rewrites: imported
                .rewrites
                .iter()
                .map(super::CachedRewriteRule::from)
                .collect(),
            scripts: imported
                .scripts
                .iter()
                .map(super::CachedScriptRule::from)
                .collect(),
            task_scripts: imported
                .task_scripts
                .iter()
                .map(|(task, _)| task.clone())
                .collect(),
            hostnames: imported.hostnames.clone(),
            mitm_templates: imported.mitm_templates.clone(),
            meta: (imported.meta != ConfigMeta::default()).then(|| imported.meta.clone()),
            module_id: String::new(),
            local: None,
        }
    }

    /// Bind only freshly parsed content to its known owner, never legacy cache records.
    pub(crate) fn bind_module(&mut self, module_id: &str) {
        self.module_id = module_id.to_string();
        #[cfg(feature = "mitm")]
        for rule in &mut self.scripts {
            rule.module_id = Some(module_id.to_string());
        }
        for task in &mut self.task_scripts {
            task.module_id = Some(module_id.to_string());
        }
    }

    pub fn into_merged(self) -> LanpathResult<super::MergedRemoteConfig> {
        self.into_merged_for_remote(None)
    }

    #[cfg(feature = "mitm")]
    pub(crate) fn resolved_hostnames(
        &self,
        remote: Option<&super::RemoteResource>,
    ) -> (Vec<String>, Vec<String>) {
        let (user_values, defaults) =
            super::activation::module_argument_values(self.meta.as_ref(), remote);
        let (derived, warnings) =
            crate::import::resolve_mitm_templates(&self.mitm_templates, &user_values, &defaults);
        let mut hostnames = self.hostnames.clone();
        for hostname in derived {
            if !hostnames.contains(&hostname) {
                hostnames.push(hostname);
            }
        }
        (hostnames, warnings)
    }

    /// Runtime aggregation and source provenance share this activation path.
    pub(crate) fn into_merged_for_remote(
        self,
        remote: Option<&super::RemoteResource>,
    ) -> LanpathResult<super::MergedRemoteConfig> {
        if self.version != REMOTE_CACHE_VERSION || self.module_id.is_empty() {
            return Err(LanpathError::Client("legacy or unsupported cache retained; refetch the remote resource or explicitly reimport the local snippet".into()));
        }
        let (user_values, defaults) =
            super::activation::module_argument_values(self.meta.as_ref(), remote);
        #[cfg(feature = "mitm")]
        let (derived, warnings) =
            crate::import::resolve_mitm_templates(&self.mitm_templates, &user_values, &defaults);
        #[cfg(not(feature = "mitm"))]
        let warnings = Vec::new();
        #[cfg(feature = "mitm")]
        let mut hostnames = self.hostnames;
        #[cfg(not(feature = "mitm"))]
        let hostnames = self.hostnames;
        #[cfg(feature = "mitm")]
        for hostname in derived {
            if !hostnames.contains(&hostname) {
                hostnames.push(hostname);
            }
        }
        #[cfg(feature = "mitm")]
        let rewrites = self
            .rewrites
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, lanpath_mitm::PatternError>>()
            .map_err(|error| {
                LanpathError::Client(format!(
                    "cached rewrite not activated: {error}; refetch or reimport"
                ))
            })?;
        #[cfg(feature = "mitm")]
        let scripts = self
            .scripts
            .into_iter()
            .filter(|rule| rule.enabled)
            .map(TryInto::try_into)
            .collect::<LanpathResult<Vec<_>>>()?;
        let mut merged = super::MergedRemoteConfig {
            #[cfg(feature = "mitm")]
            rewrites,
            #[cfg(feature = "mitm")]
            scripts,
            task_scripts: self
                .task_scripts
                .into_iter()
                .filter(|task| task.enabled)
                .collect(),
            hostnames,
            metas: self.meta.into_iter().collect(),
            warnings,
        };
        #[cfg(feature = "mitm")]
        {
            merged
                .scripts
                .retain(|rule| rule.module_id.as_deref() == Some(self.module_id.as_str()));
            for rule in &mut merged.scripts {
                if let Some(argument) = &mut rule.argument {
                    *argument = super::resolve_argument_template(argument, &user_values, &defaults);
                }
            }
        }
        merged.task_scripts.retain(|task| {
            task.module_id.as_deref() == Some(self.module_id.as_str()) && !task.source_id.is_empty()
        });
        for task in &mut merged.task_scripts {
            if let Some(argument) = &mut task.argument {
                *argument = super::resolve_argument_template(argument, &user_values, &defaults);
            }
        }
        Ok(merged)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedHostname {
    pub hostname: String,
    pub sources: Vec<String>,
}

/// Provenance uses exactly the active aggregate selection, never directory enumeration.
pub fn collect_hostname_sources(data_dir: &std::path::Path) -> Vec<DerivedHostname> {
    let manager = super::RemoteManager::new(data_dir.to_path_buf());
    let Ok((parts, _)) = manager.active_cached_parts() else {
        return Vec::new();
    };
    let mut map = std::collections::BTreeMap::<String, Vec<String>>::new();
    for (source, remote, cached) in parts {
        let Ok(active) = cached.into_merged_for_remote(remote.as_ref()) else {
            continue;
        };
        for hostname in active.hostnames {
            let sources = map.entry(hostname).or_default();
            if !sources.contains(&source) {
                sources.push(source.clone());
            }
        }
    }
    map.into_iter()
        .map(|(hostname, sources)| DerivedHostname { hostname, sources })
        .collect()
}
