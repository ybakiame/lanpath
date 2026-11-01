//! Framework-free typed core inventory, verified selection, and atomic persistence.

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use lanpath_common::{CoreType, LanpathError, LanpathResult};
use serde::{Deserialize as _, Serialize};

use crate::{
    ClientConfig, ClientCoreInventory,
    cores::{channel_of_version, safety, version},
};

#[cfg(test)]
#[path = "core_management/tests.rs"]
mod tests;

static SELECTION_WRITE: Mutex<()> = Mutex::new(());

/// Installed core presentation shared by desktop shells. Paths are serialized as strings.
#[derive(Debug, Clone, Serialize)]
pub struct ManagedCore {
    core_type: CoreType,
    version: String,
    environment: Option<String>,
    tags: Vec<String>,
    path: PathBuf,
    active: bool,
}

impl ManagedCore {
    pub fn core_type(&self) -> CoreType {
        self.core_type
    }
    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn environment(&self) -> Option<&str> {
        self.environment.as_deref()
    }
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn is_active(&self) -> bool {
        self.active
    }

    fn from_details(
        core_type: CoreType,
        path: PathBuf,
        active: bool,
        details: Option<version::CoreVersionDetails>,
        fallback_version: &str,
    ) -> Self {
        let (version, environment, tags) = match details {
            Some(details) => (details.version, details.environment, details.tags),
            None => (fallback_version.to_string(), None, Vec::new()),
        };
        Self {
            core_type,
            version,
            environment,
            tags,
            path,
            active,
        }
    }
}

/// Latest actual release version in a channel, with the existing IPC field names.
#[derive(Debug, Clone, Serialize)]
pub struct RemoteChannel {
    channel: String,
    version: String,
}

impl RemoteChannel {
    pub fn channel(&self) -> &str {
        &self.channel
    }
    pub fn version(&self) -> &str {
        &self.version
    }
}

/// Owns core operations; callers serialize these with running-core/configuration changes.
#[derive(Debug, Clone)]
pub struct CoreManagementService {
    root: PathBuf,
    api_base: Option<String>,
    confined: bool,
    core_type: CoreType,
}

impl CoreManagementService {
    /// Stable desktop policy: explicit executable paths outside inventory remain permitted.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            api_base: None,
            confined: false,
            core_type: CoreType::SingBox,
        }
    }

    /// Experimental policy: only real, unlinked binaries inside this root's inventory.
    pub fn experimental(root: PathBuf) -> Self {
        Self {
            root,
            api_base: None,
            confined: true,
            core_type: CoreType::SingBox,
        }
    }

    /// Select which upstream inventory this handle manages.
    pub fn for_core(&self, core_type: CoreType) -> Self {
        let mut service = self.clone();
        service.core_type = core_type;
        service
    }

    /// Inject release metadata API for local fixtures; production defaults to GitHub.
    pub fn with_api_base(mut self, base: impl Into<String>) -> Self {
        self.api_base = Some(base.into());
        self
    }

    /// Validate an existing managed binary without executing it or changing configuration.
    pub fn validate_managed_binary(
        root: &Path,
        core_type: CoreType,
        path: &Path,
    ) -> LanpathResult<()> {
        managed_version(root, core_type, path)?;
        safety::unlinked_path(path)?;
        safety::executable(path)
    }

    fn inventory(&self) -> LanpathResult<ClientCoreInventory> {
        if self.confined {
            safety::unlinked_path(&self.root.join("cores"))?;
            safety::unlinked_path(&crate::settings::client_settings_path(&self.root))?;
            safety::unlinked_path(&self.root.join("cores").join(self.core_type.binary_name()))?;
            return Ok(ClientCoreInventory::for_experiment(
                self.root.clone(),
                self.api_base.as_deref(),
            )
            .for_core(self.core_type));
        }
        Ok(match &self.api_base {
            Some(base) => ClientCoreInventory::with_api_base(self.root.clone(), base),
            None => ClientCoreInventory::new(self.root.clone()),
        }
        .for_core(self.core_type))
    }

    fn active(&self) -> LanpathResult<PathBuf> {
        if self.confined {
            safety::unlinked_path(&crate::settings::client_settings_path(&self.root))?;
        }
        Ok(ClientConfig::load_for_requests(&self.root)?.core_binary)
    }

    /// Actual selection, independent from the inventory type currently being managed.
    pub fn active_core(&self) -> LanpathResult<ManagedCore> {
        let active = self.active()?;
        if active.as_os_str().is_empty() {
            return Err(LanpathError::Core("No active core is selected".into()));
        }
        let core_type = version::infer_core_type(&active)
            .ok_or_else(|| LanpathError::Core("Active core type cannot be inferred".into()))?;
        if self.confined {
            safety::unlinked_path(&active)?;
            safety::executable(&active)?;
        }
        let fallback = managed_version(&self.root, core_type, &active)
            .map(str::to_string)
            .unwrap_or_else(|_| {
                active
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string()
            });
        Ok(ManagedCore::from_details(
            core_type,
            active.clone(),
            true,
            version::probe_details(&active, core_type),
            &fallback,
        ))
    }

    pub fn list_local(&self) -> LanpathResult<Vec<ManagedCore>> {
        let active = self.active()?;
        let inventory = self.inventory()?;
        let type_dir = self.root.join("cores").join(self.core_type.binary_name());
        match std::fs::read_dir(&type_dir) {
            Ok(entries) => {
                for entry in entries {
                    entry?;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let mut result = Vec::new();
        for core in inventory.list_installed() {
            if self.confined {
                Self::validate_managed_binary(&self.root, self.core_type, &core.path)?;
            }
            let is_active = paths_equal(&core.path, &active);
            let details = is_active
                .then(|| version::probe_details(&core.path, self.core_type))
                .flatten();
            result.push(ManagedCore::from_details(
                self.core_type,
                core.path,
                is_active,
                details,
                &core.version,
            ));
        }
        Ok(result)
    }

    pub fn list_downloaded_versions(&self) -> LanpathResult<Vec<String>> {
        let mut versions: Vec<_> = self
            .list_local()?
            .into_iter()
            .map(|core| core.version)
            .collect();
        versions.sort_by(|a, b| version::compare_core_versions(b, a));
        Ok(versions)
    }

    pub async fn list_remote_channels(&self) -> LanpathResult<Vec<RemoteChannel>> {
        self.inventory()?
            .list_remote_channels()
            .await?
            .into_iter()
            .map(|remote| {
                let value = safety::normalized_version_for(self.core_type, &remote.version)?;
                Ok(RemoteChannel {
                    channel: remote.channel.as_str().into(),
                    version: value.into(),
                })
            })
            .collect()
    }

    /// Download, verify, then select. Any failure leaves the previous selection unchanged.
    pub async fn download(&self, requested: &str) -> LanpathResult<ManagedCore> {
        self.download_with_progress(requested, &|_| {}).await
    }

    /// Report real backend stages without inventing byte progress.
    pub async fn download_with_progress(
        &self,
        requested: &str,
        progress: &(dyn Fn(crate::cores::CoreDownloadStage) + Send + Sync),
    ) -> LanpathResult<ManagedCore> {
        safety::normalized_version_for(self.core_type, requested)?;
        let core = self
            .inventory()?
            .download_with_tun(requested, !self.confined, progress)
            .await?;
        progress(crate::cores::CoreDownloadStage::Selecting);
        self.persist_selection(&core.path)?;
        self.prune_old_versions(&core.version)?;
        Ok(ManagedCore::from_details(
            self.core_type,
            core.path.clone(),
            true,
            version::probe_details(&core.path, self.core_type),
            &core.version,
        ))
    }

    pub fn select(&self, path: &Path) -> LanpathResult<()> {
        if self.confined {
            Self::validate_managed_binary(&self.root, self.core_type, path)?;
        }
        safety::executable(path)?;
        match managed_version(&self.root, self.core_type, path) {
            Ok(requested) => version::verify_version_for(path, self.core_type, requested)?,
            Err(error) if self.confined => return Err(error),
            Err(_) => {
                let reported = version::parse_version_for(
                    self.core_type,
                    &version::binary_output_for(self.core_type, path),
                )
                .ok_or_else(|| {
                    safety::invalid(format!("Core does not report a {} version", self.core_type))
                })?;
                safety::normalized_version_for(self.core_type, &reported)?;
            }
        }
        self.persist_selection(path)
    }

    fn persist_selection(&self, binary: &Path) -> LanpathResult<()> {
        let _guard = SELECTION_WRITE
            .lock()
            .map_err(|_| safety::invalid("Core selection lock poisoned"))?;
        if self.confined {
            Self::validate_managed_binary(&self.root, self.core_type, binary)?;
        } else {
            safety::executable(binary)?;
        }
        let file = if self.confined {
            crate::settings::ensure_app_settings(&self.root)?;
            crate::settings::app_settings_path(&self.root)
        } else {
            crate::settings::client_settings_path(&self.root)
        };
        if self.confined {
            safety::unlinked_path(&file)?;
        }
        let mut raw = match std::fs::read_to_string(&file) {
            Ok(text) => {
                let value = crate::settings::client_from_text(&file, &text)?;
                if !value.is_object() {
                    return Err(safety::invalid("client settings must contain an object"));
                }
                let _ = ClientConfig::deserialize(&value)?;
                value
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                serde_json::to_value(ClientConfig::new(self.root.clone(), "", "", PathBuf::new()))?
            }
            Err(error) => return Err(error.into()),
        };
        raw["core_binary"] = serde_json::to_value(binary)?;
        raw["core_type"] = serde_json::to_value(self.core_type)?;
        std::fs::create_dir_all(&self.root)?;
        crate::settings::persist_client(&file, &raw)
    }

    fn prune_old_versions(&self, current_version: &str) -> LanpathResult<()> {
        if !self.confined {
            return Ok(());
        }
        let active = self.active()?;
        for core in prune_candidates(self.list_local()?, current_version) {
            self.inventory()?.delete(&core.path, &active)?;
        }
        Ok(())
    }

    pub fn delete(&self, path: &Path) -> LanpathResult<()> {
        let _guard = SELECTION_WRITE
            .lock()
            .map_err(|_| safety::invalid("Core selection lock poisoned"))?;
        let active = self.active()?;
        if paths_equal(path, &active) {
            return Err(safety::invalid("正在使用的核心不可删除：请先切换其他核心"));
        }
        Self::validate_managed_binary(&self.root, self.core_type, path)?;
        self.inventory()?.delete(path, &active)
    }
}

fn prune_candidates(local: Vec<ManagedCore>, current_version: &str) -> Vec<ManagedCore> {
    let channel = channel_of_version(current_version);
    let mut installed: Vec<_> = local
        .into_iter()
        .filter(|core| channel_of_version(core.version()) == channel)
        .collect();
    installed.sort_by(|a, b| version::compare_core_versions(&b.version, &a.version));
    installed.into_iter().skip(2).collect()
}

fn managed_version<'a>(root: &Path, core_type: CoreType, path: &'a Path) -> LanpathResult<&'a str> {
    let relative = path
        .strip_prefix(root.join("cores").join(core_type.binary_name()))
        .map_err(|_| safety::invalid("Core is outside this inventory"))?;
    let mut parts = relative.components();
    let name = match parts.next() {
        Some(std::path::Component::Normal(name)) => name.to_str(),
        _ => None,
    }
    .ok_or_else(|| safety::invalid("Invalid core version directory"))?;
    if safety::normalized_version_for(core_type, name)? != name
        || parts.next()
            != Some(std::path::Component::Normal(std::ffi::OsStr::new(
                &version::binary_name_on_disk_for(core_type),
            )))
        || parts.next().is_some()
    {
        return Err(safety::invalid(
            "Core path must identify an inventory platform binary",
        ));
    }
    Ok(name)
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    left == right || matches!((left.canonicalize(), right.canonicalize()), (Ok(a), Ok(b)) if a == b)
}
