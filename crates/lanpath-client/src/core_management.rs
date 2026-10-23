//! Framework-free core inventory, verified selection, and atomic selection persistence.

use std::{
    io::Write as _,
    path::{Path, PathBuf},
    sync::Mutex,
};

use lanpath_common::LanpathResult;
use serde::{Deserialize as _, Serialize};

use crate::{
    ClientConfig, ClientCoreInventory,
    cores::{safety, version},
};

#[cfg(test)]
#[path = "core_management/tests.rs"]
mod tests;

static SELECTION_WRITE: Mutex<()> = Mutex::new(());

/// Installed core presentation shared by desktop shells. Paths are serialized as strings.
#[derive(Debug, Clone, Serialize)]
pub struct ManagedCore {
    version: String,
    path: PathBuf,
    active: bool,
}

impl ManagedCore {
    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn is_active(&self) -> bool {
        self.active
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
}

impl CoreManagementService {
    /// Stable desktop policy: explicit executable paths outside inventory remain permitted.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            api_base: None,
            confined: false,
        }
    }

    /// Experimental policy: only real, unlinked binaries inside this root's inventory.
    pub fn experimental(root: PathBuf) -> Self {
        Self {
            root,
            api_base: None,
            confined: true,
        }
    }

    /// Inject release metadata API for local fixtures; production defaults to GitHub.
    pub fn with_api_base(mut self, base: impl Into<String>) -> Self {
        self.api_base = Some(base.into());
        self
    }

    /// Validate an existing managed binary without executing it or changing configuration.
    /// Useful when normalizing an experimental saved selection.
    pub fn validate_managed_binary(root: &Path, path: &Path) -> LanpathResult<()> {
        managed_version(root, path)?;
        safety::unlinked_path(path)?;
        safety::executable(path)
    }

    fn inventory(&self) -> LanpathResult<ClientCoreInventory> {
        if self.confined {
            safety::unlinked_path(&self.root.join("cores"))?;
            safety::unlinked_path(&self.root.join("client.json"))?;
            safety::unlinked_path(&self.root.join("cores/sing-box"))?;
            return Ok(ClientCoreInventory::for_experiment(
                self.root.clone(),
                self.api_base.as_deref(),
            ));
        }
        Ok(match &self.api_base {
            Some(base) => ClientCoreInventory::with_api_base(self.root.clone(), base),
            None => ClientCoreInventory::new(self.root.clone()),
        })
    }

    fn active(&self) -> LanpathResult<PathBuf> {
        if self.confined {
            safety::unlinked_path(&self.root.join("client.json"))?;
        }
        Ok(ClientConfig::load_for_requests(&self.root)?.core_binary)
    }

    pub fn list_local(&self) -> LanpathResult<Vec<ManagedCore>> {
        let active = self.active()?;
        let inventory = self.inventory()?;
        match std::fs::read_dir(self.root.join("cores/sing-box")) {
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
                Self::validate_managed_binary(&self.root, &core.path)?;
            }
            result.push(ManagedCore {
                version: core.version,
                active: paths_equal(&core.path, &active),
                path: core.path,
            });
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
                let value = safety::normalized_version(&remote.version)?;
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
        safety::normalized_version(requested)?;
        let core = self
            .inventory()?
            .download_with_tun(requested, !self.confined, progress)
            .await?;
        progress(crate::cores::CoreDownloadStage::Selecting);
        self.persist_selection(&core.path)?;
        Ok(ManagedCore {
            version: core.version,
            path: core.path,
            active: true,
        })
    }

    pub fn select(&self, path: &Path) -> LanpathResult<()> {
        if self.confined {
            Self::validate_managed_binary(&self.root, path)?;
        }
        safety::executable(path)?;
        match managed_version(&self.root, path) {
            Ok(requested) => version::verify_version(path, requested)?,
            Err(error) if self.confined => return Err(error),
            Err(_) => {
                let reported = version::parse_version_from_output(&version::binary_output(path))
                    .ok_or_else(|| safety::invalid("Core does not report a sing-box version"))?;
                safety::normalized_version(&reported)?;
            }
        }
        self.persist_selection(path)
    }

    fn persist_selection(&self, binary: &Path) -> LanpathResult<()> {
        let _guard = SELECTION_WRITE
            .lock()
            .map_err(|_| safety::invalid("Core selection lock poisoned"))?;
        // Probing runs before the lock; a concurrent delete must not publish a
        // now-missing selection. Downloads already performed their version probe.
        if self.confined {
            Self::validate_managed_binary(&self.root, binary)?;
        } else {
            safety::executable(binary)?;
        }
        let file = self.root.join("client.json");
        if self.confined {
            safety::unlinked_path(&file)?;
        }
        // Read after probing/network work: stale service snapshots never replace other settings.
        let mut raw = match std::fs::read(&file) {
            Ok(bytes) => {
                let value: serde_json::Value = serde_json::from_slice(&bytes)?;
                if !value.is_object() {
                    return Err(safety::invalid("client.json must be an object"));
                }
                // Validate the latest persisted schema without rewriting its unrelated fields.
                let _ = ClientConfig::deserialize(&value)?;
                value
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                serde_json::to_value(ClientConfig::new(self.root.clone(), "", "", PathBuf::new()))?
            }
            Err(error) => return Err(error.into()),
        };
        raw["core_binary"] = serde_json::to_value(binary)?;
        std::fs::create_dir_all(&self.root)?;
        let mut temporary = tempfile::NamedTempFile::new_in(&self.root)?;
        serde_json::to_writer_pretty(&mut temporary, &raw)?;
        temporary.write_all(b"\n")?;
        temporary.as_file().sync_all()?;
        temporary.persist(&file).map_err(|error| error.error)?;
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
        // Deletion is always inventory-only, including for the stable external-path policy.
        Self::validate_managed_binary(&self.root, path)?;
        self.inventory()?.delete(path, &active)
    }
}

fn managed_version<'a>(root: &Path, path: &'a Path) -> LanpathResult<&'a str> {
    let relative = path
        .strip_prefix(root.join("cores/sing-box"))
        .map_err(|_| safety::invalid("Core is outside this inventory"))?;
    let mut parts = relative.components();
    let name = match parts.next() {
        Some(std::path::Component::Normal(name)) => name.to_str(),
        _ => None,
    }
    .ok_or_else(|| safety::invalid("Invalid core version directory"))?;
    if safety::normalized_version(name)? != name
        || parts.next()
            != Some(std::path::Component::Normal(std::ffi::OsStr::new(
                &version::binary_name_on_disk(),
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
