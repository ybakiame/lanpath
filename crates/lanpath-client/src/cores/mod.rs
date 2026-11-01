//! Core version management: download local cores and list installed versions.
//!
//! Each inventory instance manages one core type under
//! `data_dir/cores/<type>/<version>/`.
//!
//! # Reuse conclusion
//!
//! Does not directly reuse `lanpath-engine::installer`'s `ensure_core_binary`:
//!
//! - Its semantics are for agent's "single-directory disk + environment variable/latest version
//!   resolution", which differs from this module's versioned directory + explicit version model;
//! - Its GitHub domain is fixed, cannot inject mock services for local acceptance testing;
//! - This module retains its asset naming and extraction ideas, implementing a simplified version
//!   for the client scenario.
//!
//! Remote versions and assets are always based on GitHub Release API. Assets are matched against
//! the selected upstream's exact platform naming rule rather than constructing download URLs.

use std::path::{Path, PathBuf};

use lanpath_common::{CoreType, LanpathError, LanpathResult};
use tokio::io::AsyncWriteExt;

use crate::config::ClientConfig;

mod channel;
mod download;
pub(crate) mod safety;
mod seed;
#[cfg(test)]
mod tests;
pub(crate) mod version;

pub use channel::{CoreChannel, RemoteChannelVersion, channel_of_version};
pub use seed::seed_bundled_core;
pub use version::infer_core_type;

/// GitHub API request timeout (seconds) — applied per-request to release metadata
/// calls only; the binary download itself has no total timeout (see below).
const HTTP_TIMEOUT_SECS: u64 = 30;

/// TCP connect timeout (seconds) for all requests (fast fail on dead networks).
const HTTP_CONNECT_TIMEOUT_SECS: u64 = 10;

/// A locally available core under `data_dir/cores/<type>/<version>/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalCore {
    pub core_type: CoreType,
    pub version: String,
    pub path: PathBuf,
}

/// Client core inventory: manages download directory scanning, remote version listing,
/// download installation, and deletion for one core type.
#[derive(Debug, Clone)]
pub struct ClientCoreInventory {
    data_dir: PathBuf,
    api_base: String,
    core_type: CoreType,
}

/// Actual installation stages; observers must not perform blocking work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreDownloadStage {
    ResolvingRelease,
    Downloading,
    Extracting,
    Verifying,
    Selecting,
}

impl ClientCoreInventory {
    /// No legacy-directory migration is allowed in a leased experimental root.
    pub(crate) fn for_experiment(data_dir: PathBuf, api_base: Option<&str>) -> Self {
        Self {
            data_dir,
            api_base: api_base.unwrap_or("https://api.github.com").into(),
            core_type: CoreType::SingBox,
        }
    }

    /// Select the upstream managed by this inventory.
    pub fn for_core(mut self, core_type: CoreType) -> Self {
        self.core_type = core_type;
        self
    }

    pub fn core_type(&self) -> CoreType {
        self.core_type
    }

    /// Create inventory based on data directory (GitHub API uses `https://api.github.com`).
    pub fn new(data_dir: PathBuf) -> Self {
        Self::with_api_base(data_dir, "https://api.github.com")
    }

    /// Specify GitHub API base (for injecting mock service addresses in tests).
    pub fn with_api_base(data_dir: PathBuf, api_base: impl Into<String>) -> Self {
        Self {
            data_dir,
            api_base: api_base.into(),
            core_type: CoreType::SingBox,
        }
    }

    /// Read the latest global policy per operation; downloads have no total timeout.
    fn http_client(&self) -> LanpathResult<reqwest::Client> {
        let cfg = ClientConfig::load_for_requests(&self.data_dir)?;
        crate::request_client_builder(&cfg)?
            .connect_timeout(std::time::Duration::from_secs(HTTP_CONNECT_TIMEOUT_SECS))
            .build()
            .map_err(|e| {
                LanpathError::Core(format!(
                    "Failed to build download client: {}",
                    e.without_url()
                ))
            })
    }

    /// GitHub proxy prefix (from settings page "GitHub Access"): best-effort read from
    /// `data_dir/client.json`, falls back to empty string (direct connection) on missing/corrupt file.
    ///
    /// Core version queries and downloads share the same GitHub access strategy (URL prefix
    /// concatenation) with remote resource fetching; the user-configured proxy prefix applies to
    /// both `api.github.com` release queries and `github.com` release asset downloads.
    fn github_proxy_prefix(&self) -> String {
        ClientConfig::load(&self.data_dir)
            .unwrap_or_default()
            .github_proxy_prefix
    }

    /// Download directory: `data_dir/cores`.
    pub fn cores_dir(&self) -> PathBuf {
        self.data_dir.join("cores")
    }

    /// Specific core version directory: `data_dir/cores/<type>/<version>`.
    pub(super) fn core_dir(&self, version: &str) -> PathBuf {
        self.cores_dir()
            .join(self.core_type.binary_name())
            .join(version)
    }

    /// Scan `cores_dir/<type>/<version>/` and list downloaded cores.
    pub fn list_installed(&self) -> Vec<LocalCore> {
        let mut out = Vec::new();
        let type_dir = self.cores_dir().join(self.core_type.binary_name());
        if let Ok(entries) = std::fs::read_dir(&type_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let version = entry.file_name().to_string_lossy().into_owned();
                // Interrupted installations are staging, not installed versions.
                if version.starts_with(".core-download-") {
                    continue;
                }
                let bin = path.join(version::binary_name_on_disk_for(self.core_type));
                if bin.is_file() {
                    out.push(LocalCore {
                        core_type: self.core_type,
                        version,
                        path: bin,
                    });
                }
            }
        }
        out.sort_by(|a, b| a.version.cmp(&b.version).then_with(|| a.path.cmp(&b.path)));
        out
    }

    /// Scan version directories, list downloaded version numbers sorted by semantic version
    /// descending (latest first, prerelease lower than same-base stable).
    pub fn list_downloaded_versions(&self) -> Vec<String> {
        let mut versions: Vec<String> = self
            .list_installed()
            .into_iter()
            .map(|c| c.version)
            .collect();
        versions.sort_by(|a, b| version::compare_core_versions(b, a));
        versions
    }

    /// Download specified core version and save to `cores_dir/sing-box/<version>/`.
    ///
    /// Reuses existing download if same version already present; after download extracts,
    /// chmod 755, and verifies version probe output (`version` / `--version` / `-v` tried in
    /// sequence) exactly matches the target version.
    pub async fn download(&self, version: &str) -> LanpathResult<LocalCore> {
        self.download_with_tun(version, true, &|_| {}).await
    }

    /// Experimental shells that cannot enable TUN do not fetch its optional DLL.
    pub(crate) async fn download_with_tun(
        &self,
        version: &str,
        _include_tun: bool,
        progress: &(dyn Fn(CoreDownloadStage) + Send + Sync),
    ) -> LanpathResult<LocalCore> {
        let version = safety::normalized_version_for(self.core_type, version)?.to_string();
        let (arch_hint, is_windows) = version::target_spec_for(self.core_type)?;

        let dir = self.core_dir(&version);
        let on_disk = dir.join(version::binary_name_on_disk_for(self.core_type));

        safety::unlinked_path(&dir)?;
        safety::unlinked_path(&on_disk)?;
        // Never overwrite a previously installed binary, including a broken active selection.
        if on_disk.exists() {
            progress(CoreDownloadStage::Verifying);
            safety::executable(&on_disk)?;
            version::verify_version(&on_disk, &version)?;
            return Ok(LocalCore {
                core_type: self.core_type,
                version,
                path: on_disk,
            });
        }
        let parent = dir
            .parent()
            .ok_or_else(|| safety::invalid("Invalid core directory"))?;
        std::fs::create_dir_all(parent)?;
        let staging = tempfile::Builder::new()
            .prefix(".core-download-")
            .tempdir_in(parent)?;
        let destination = dir;
        let dir = staging.path().to_path_buf();

        // Match current platform asset via GitHub Release API (real naming may deviate from
        // convention, do not construct URL directly).
        progress(CoreDownloadStage::ResolvingRelease);
        let (asset_url, asset_name) = self
            .resolve_asset_url(&version, arch_hint, is_windows)
            .await?;
        safety::asset_name(&asset_name)?;
        // Asset download initial URL is `github.com/<owner>/<repo>/releases/download/...`,
        // wrapped by configured proxy prefix; 302 redirect to `objects.githubusercontent.com`
        // is followed by the gh proxy side (not wrapped again here).
        let asset_url = crate::apply_github_proxy_prefix(&asset_url, &self.github_proxy_prefix());
        tracing::info!(
            version = %version,
            url = %asset_url,
            "Downloading core"
        );

        let tmp_archive = dir.join(format!(".download-{asset_name}"));
        progress(CoreDownloadStage::Downloading);
        self.download_to(&asset_url, &tmp_archive).await?;

        progress(CoreDownloadStage::Extracting);
        let binary_inside = self.core_type.binary_name();
        let core_type = self.core_type;
        let (dir_clone, archive_clone) = (dir.clone(), tmp_archive.clone());
        let result = tokio::task::spawn_blocking(move || {
            if asset_name.ends_with(".tar.gz") {
                download::extract_tgz(&archive_clone, &dir_clone, binary_inside)
            } else if asset_name.ends_with(".zip") {
                download::extract_zip(&archive_clone, &dir_clone, binary_inside, core_type)
            } else if asset_name.ends_with(".gz") {
                let path = download::extract_gz(&archive_clone, &dir_clone, binary_inside)?;
                if path.file_name()
                    != Some(std::ffi::OsStr::new(&version::binary_name_on_disk_for(
                        core_type,
                    )))
                {
                    std::fs::rename(
                        &path,
                        dir_clone.join(version::binary_name_on_disk_for(core_type)),
                    )?;
                }
                Ok(dir_clone.join(version::binary_name_on_disk_for(core_type)))
            } else {
                Err(LanpathError::Core(format!(
                    "Unknown asset format: {asset_name}"
                )))
            }
        })
        .await
        .map_err(|e| LanpathError::Core(format!("Extraction task failed: {e}")))??;
        // Best-effort cleanup of download archive.
        let _ = std::fs::remove_file(&tmp_archive);

        let path = result;
        download::set_executable(&path)?;

        // Windows：sing-box TUN 依赖 wintun.dll 与核心可执行文件同目录（核心不内嵌）。
        // 核心下载成功后随附下载对应架构的 wintun.dll；失败不阻塞核心安装（TUN
        // 启动时会报「Unable to load library」，错误信息足够明确）。
        #[cfg(target_os = "windows")]
        if self.core_type == CoreType::SingBox
            && _include_tun
            && let Err(e) = download::ensure_wintun(&self.http_client()?, &dir).await
        {
            tracing::warn!(error = %e, "wintun.dll 下载失败，TUN 模式将不可用");
        }

        progress(CoreDownloadStage::Verifying);
        safety::executable(&path)?;
        version::verify_version_for(&path, self.core_type, &version)?;
        let path = download::publish_core(&dir, &destination, self.core_type)?;

        tracing::info!(
            core = %self.core_type,
            version = %version,
            path = %path.display(),
            "Core download complete"
        );
        Ok(LocalCore {
            core_type: self.core_type,
            version,
            path,
        })
    }

    /// Delete a downloaded core (only cores within `cores_dir`).
    ///
    /// Deletes the entire `cores/sing-box/<version>/` version directory; cleans up type directory
    /// if empty after deletion.
    /// Errors: path outside `cores_dir` (system core) / path does not exist / core is
    /// `active_binary` (in use).
    ///
    /// Safety constraint: `canonicalize` path ownership check before actual deletion, prevents
    /// deleting anything outside `cores_dir` (directory traversal / symlink escape protection).
    pub fn delete(&self, path: &Path, active_binary: &Path) -> LanpathResult<()> {
        // Anti-traversal: after canonicalize confirm target is inside download directory.
        // If download directory does not exist, there are no downloaded cores to delete.
        let cores_dir = std::fs::canonicalize(self.cores_dir()).map_err(|_| {
            LanpathError::Core("Core download directory does not exist".to_string())
        })?;
        let bin = std::fs::canonicalize(path)
            .map_err(|e| LanpathError::Core(format!("Core binary does not exist: {e}")))?;
        if !bin.starts_with(&cores_dir) {
            return Err(LanpathError::Core(
                "System core cannot be deleted: only cores in download directory are supported"
                    .to_string(),
            ));
        }
        // Structure validation: target must be of form `cores/sing-box/<version>/<binary>`,
        // avoid accidentally deleting type directory or entire download directory.
        if !bin.is_file() {
            return Err(LanpathError::Core("Invalid core binary path".to_string()));
        }
        let version_dir = bin.parent().ok_or_else(|| {
            LanpathError::Core("Cannot locate core version directory".to_string())
        })?;
        let type_dir = version_dir
            .parent()
            .ok_or_else(|| LanpathError::Core("Cannot locate core type directory".to_string()))?;
        if type_dir.parent() != Some(cores_dir.as_path())
            || type_dir.file_name() != Some(std::ffi::OsStr::new(self.core_type.binary_name()))
        {
            return Err(LanpathError::Core(
                "Invalid core binary path for selected core type".to_string(),
            ));
        }
        // Core in use cannot be deleted.
        if paths_equal(&bin, active_binary) {
            return Err(LanpathError::Core(
                "Active core cannot be deleted: please switch to another core first".to_string(),
            ));
        }
        tracing::info!(
            path = %bin.display(),
            version_dir = %version_dir.display(),
            "Deleting local core"
        );
        std::fs::remove_dir_all(version_dir)
            .map_err(|e| LanpathError::Core(format!("Failed to delete core: {e}")))?;
        // Clean up type directory (`cores/sing-box/`) if empty.
        if std::fs::read_dir(type_dir)
            .map(|mut it| it.next().is_none())
            .unwrap_or(false)
        {
            let _ = std::fs::remove_dir(type_dir);
        }
        Ok(())
    }

    /// Download `url` to `dest` (chunked streaming to disk).
    async fn download_to(&self, url: &str, dest: &Path) -> LanpathResult<()> {
        let mut resp = self
            .http_client()?
            .get(url)
            .header("User-Agent", "lanpath-client")
            .send()
            .await
            .map_err(|e| LanpathError::Core(format!("Download request failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(LanpathError::Core(format!(
                "Download returned status {} ({})",
                resp.status(),
                url
            )));
        }
        let mut file = tokio::fs::File::create(dest).await?;
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| LanpathError::Core(format!("Download stream error: {e}")))?
        {
            file.write_all(&chunk).await?;
        }
        Ok(())
    }

    /// Match asset from GitHub release `assets` by platform / architecture, returns
    /// (download URL, asset name).
    async fn resolve_asset_url(
        &self,
        version: &str,
        arch_hint: &str,
        is_windows: bool,
    ) -> LanpathResult<(String, String)> {
        let tag = version::github_tag_for(self.core_type, version);
        let (owner, repo) = self.core_type.github_repo();
        let url = format!(
            "{}/repos/{}/{}/releases/tags/{}",
            self.api_base, owner, repo, tag
        );
        // GitHub API URL is wrapped by configured proxy prefix (shares GitHub access strategy
        // with remote resource fetching); injected mock service addresses (non-GitHub domains)
        // are not affected.
        let url = crate::apply_github_proxy_prefix(&url, &self.github_proxy_prefix());
        let resp = self
            .http_client()?
            .get(&url)
            .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
            .header("User-Agent", "lanpath-client")
            .send()
            .await
            .map_err(|e| LanpathError::Core(format!("GitHub release request failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(LanpathError::Core(format!(
                "GitHub release returned status {}",
                resp.status()
            )));
        }
        let release: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| LanpathError::Core(format!("Failed to parse GitHub release: {e}")))?;
        let assets = release
            .get("assets")
            .and_then(|v| v.as_array())
            .ok_or_else(|| LanpathError::Core("release has no assets field".to_string()))?;

        for asset in assets {
            let name = asset.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if version::asset_matches(self.core_type, version, arch_hint, is_windows, name)
                && let Some(url) = asset.get("browser_download_url").and_then(|v| v.as_str())
            {
                safety::asset_name(name)?;
                return Ok((url.to_string(), name.to_string()));
            }
        }
        Err(LanpathError::Core(format!(
            "No asset found for {core} on platform {arch_hint} in release {tag}",
            core = self.core_type
        )))
    }
}

/// Lenient path equality: direct comparison, or compare after resolving real paths.
fn paths_equal(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}
