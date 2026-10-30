//! The manifest and explicit local collection registry are the only activation authorities.
use super::{CachedRemoteConfig, MergedRemoteConfig, RemoteKind, RemoteManager, RemoteResource};
use lanpath_common::LanpathResult;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

type ActivePart = (String, Option<RemoteResource>, CachedRemoteConfig);

pub(crate) fn remote_module_id(remote: &RemoteResource) -> String {
    let mut digest = Sha256::new();
    digest.update(remote.name.as_bytes());
    digest.update([0]);
    digest.update(remote.url.as_bytes());
    digest.update([0]);
    digest.update(format!("{:?}", remote.dialect).as_bytes());
    format!("remote:{:x}", digest.finalize())
}
/// Current manifest defaults override cached declarations; explicit user values
/// override both. Local collections use only their original declared defaults.
pub(crate) fn module_argument_values(
    meta: Option<&crate::import::ConfigMeta>,
    remote: Option<&RemoteResource>,
) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut defaults: HashMap<_, _> = meta
        .into_iter()
        .flat_map(|meta| &meta.arguments)
        .map(|arg| (arg.key.clone(), arg.default_value.clone()))
        .collect();
    let mut user_values = HashMap::new();
    if let Some(remote) = remote {
        defaults.extend(
            remote
                .arguments
                .iter()
                .map(|arg| (arg.key.clone(), arg.default_value.clone())),
        );
        user_values.extend(remote.argument_values.iter().cloned());
    }
    (user_values, defaults)
}

impl RemoteManager {
    pub(crate) fn active_cached_parts(&self) -> LanpathResult<(Vec<ActivePart>, Vec<String>)> {
        let mut parts = Vec::new();
        let mut warnings = Vec::new();
        let cache_dir = self.data_dir.join("remote_cache");
        for remote in self
            .load()?
            .into_iter()
            .filter(|r| r.enabled && r.kind == RemoteKind::Snippet)
        {
            let id = remote_module_id(&remote);
            let current = cache_dir.join(format!("{}.json", id.replace(':', "-")));
            let legacy = cache_dir.join(format!("{}.json", super::safe_name(&remote.name)));
            let path = if current.exists() { current } else { legacy };
            if !path.exists() {
                continue;
            }
            match read_cache(&path) {
                Some(cache) if cache.module_id == id || cache.version == 0 => {
                    parts.push((remote.name.clone(), Some(remote), cache));
                }
                Some(_) => warnings.push("cache owner differs from enabled manifest; retained inactive; refetch the resource".into()),
                None => warnings.push("remote cache is unreadable; retained inactive; refetch the resource".into()),
            }
        }
        let index_path = cache_dir.join("local_imports.json");
        if index_path.exists() {
            let collections: Vec<String> = match std::fs::read_to_string(&index_path)
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
            {
                Some(collections) => collections,
                None => {
                    warnings.push("local collection registry is unreadable; retained inactive; recover the registry or reimport source".into());
                    Vec::new()
                }
            };
            for id in collections {
                // The registry is data, not an arbitrary file path capability.
                if uuid::Uuid::parse_str(&id).is_err() {
                    warnings.push("invalid local collection identity retained inactive".into());
                    continue;
                }
                let path = cache_dir.join("local").join(format!("{id}.json"));
                match read_cache(&path) {
                    Some(cache) if cache.module_id == format!("local:{id}") => parts.push(("本地导入".into(), None, cache)),
                    _ => warnings.push("local collection is unreadable or has mismatched identity; retained inactive; reimport source".into()),
                }
            }
        }
        if cache_dir.join("imported.json").exists() {
            warnings.push("legacy local import cache retained inactive: original module identities cannot be recovered; explicitly reimport the original snippets".into());
        }
        Ok((parts, warnings))
    }

    pub fn load_cached(&self) -> LanpathResult<MergedRemoteConfig> {
        let (parts, warnings) = self.active_cached_parts()?;
        let mut merged = MergedRemoteConfig {
            warnings,
            ..Default::default()
        };
        for (_, remote, cached) in parts {
            let part = match cached.into_merged_for_remote(remote.as_ref()) {
                Ok(part) => part,
                Err(error) => {
                    merged.warnings.push(error.to_string());
                    continue;
                }
            };
            #[cfg(feature = "mitm")]
            {
                merged.rewrites.extend(part.rewrites);
                merged.scripts.extend(part.scripts);
            }
            merged.task_scripts.extend(part.task_scripts);
            merged.metas.extend(part.metas);
            merged.warnings.extend(part.warnings);
            for hostname in part.hostnames {
                if !merged.hostnames.contains(&hostname) {
                    merged.hostnames.push(hostname);
                }
            }
        }
        Ok(merged)
    }
}

fn read_cache(path: &std::path::Path) -> Option<CachedRemoteConfig> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}
