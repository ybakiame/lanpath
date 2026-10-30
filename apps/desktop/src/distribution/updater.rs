use super::{
    invalid,
    signature::{self, Manifest},
};
use crate::data_root::DataRoot;
use lanpath_client::ClientConfig;
use lanpath_common::LanpathResult;
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

const DESCRIPTOR_LIMIT: usize = 128 * 1024;
const SIGNATURE_LIMIT: usize = 1024;

#[derive(Debug, Clone)]
pub(crate) struct AvailableUpdate {
    pub version: String,
    pub release_url: String,
    signed: Arc<SignedManifest>,
}
#[derive(Debug, Clone)]
pub(crate) enum UpdateCheck {
    Current { version: String },
    Available(AvailableUpdate),
}
#[derive(Debug, Clone)]
pub(crate) struct DownloadedUpdate {
    path: PathBuf,
    signed: Arc<SignedManifest>,
}
impl DownloadedUpdate {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn version(&self) -> &str {
        &self.signed.manifest.version
    }
}
#[derive(Debug)]
struct SignedManifest {
    payload: Vec<u8>,
    signature: String,
    manifest: Manifest,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedManifest {
    payload: String,
    signature: String,
}

pub(crate) struct UpdateService<'a> {
    root: &'a DataRoot,
    client: reqwest::Client,
}
impl<'a> UpdateService<'a> {
    /// Keep one accepted saved-policy snapshot for metadata, signature and artifact requests.
    pub(crate) fn new(root: &'a DataRoot, config: &ClientConfig) -> LanpathResult<Self> {
        let client = lanpath_client::request_client_builder(config)?
            .https_only(true)
            .user_agent(format!(
                "{}/{}",
                crate::product::APP_ID,
                crate::product::VERSION
            ))
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(600))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5
                    || attempt.url().scheme() != "https"
                    || !attempt.url().username().is_empty()
                    || attempt.url().password().is_some()
                {
                    attempt.error("unsafe or excessive update redirect")
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(http_error)?;
        Ok(Self { root, client })
    }
    fn endpoint(&self) -> LanpathResult<String> {
        let endpoint = crate::product::UPDATE_ENDPOINT.replace("{arch}", std::env::consts::ARCH);
        signature::https_url(&endpoint)?;
        Ok(endpoint)
    }
    pub(crate) async fn check(&self) -> LanpathResult<UpdateCheck> {
        let endpoint = self.endpoint()?;
        let payload = self.small_get(&endpoint, DESCRIPTOR_LIMIT).await?;
        let signature = self
            .small_get(&format!("{endpoint}.sig"), SIGNATURE_LIMIT)
            .await?;
        let signature =
            String::from_utf8(signature).map_err(|_| invalid("update signature is not UTF-8"))?;
        let manifest =
            signature::verify_manifest(&payload, &signature, crate::product::UPDATE_PUBLIC_KEY)?;
        let signed = SignedManifest {
            payload,
            signature,
            manifest,
        };
        validate_kind(&signed.manifest)?;
        let precedence = version_precedence(&signed.manifest.version, crate::product::VERSION)?;
        if precedence.is_lt() {
            return Err(invalid("update server offered a downgrade"));
        }
        self.check_high_watermark(&signed.manifest)?;
        if precedence.is_eq() {
            return Ok(UpdateCheck::Current {
                version: crate::product::VERSION.into(),
            });
        }
        Ok(UpdateCheck::Available(AvailableUpdate {
            version: signed.manifest.version.clone(),
            release_url: format!(
                "{}/releases/tag/gpui-v{}",
                crate::product::REPOSITORY,
                signed.manifest.version
            ),
            signed: Arc::new(signed),
        }))
    }
    async fn get(&self, url: &str, timeout: Duration) -> LanpathResult<reqwest::Response> {
        signature::https_url(url)?;
        self.client
            .get(url)
            .timeout(timeout)
            .send()
            .await
            .map_err(http_error)?
            .error_for_status()
            .map_err(http_error)
    }
    async fn small_get(&self, url: &str, limit: usize) -> LanpathResult<Vec<u8>> {
        let mut response = self.get(url, Duration::from_secs(30)).await?;
        if response.content_length().is_some_and(|n| n > limit as u64) {
            return Err(invalid("update metadata exceeds limit"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(http_error)? {
            if bytes.len().saturating_add(chunk.len()) > limit {
                return Err(invalid("update metadata exceeds limit"));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
    /// Both data modes download the same authenticated ZIP. Replacement happens
    /// only after exit, through the native payload transaction.
    pub(crate) async fn download(
        &self,
        available: &AvailableUpdate,
    ) -> LanpathResult<DownloadedUpdate> {
        let manifest = self.revalidate(&available.signed)?;
        crate::data_root::validate_owned_root(self.root.path())?;
        let updates = self.root.path().join("updates");
        std::fs::create_dir_all(&updates)?;
        let directory = tempfile::Builder::new()
            .prefix("verified-")
            .tempdir_in(&updates)?;
        let filename = manifest
            .artifact
            .url
            .rsplit('/')
            .next()
            .ok_or_else(|| invalid("artifact has no filename"))?;
        signature::safe_relative(filename)?;
        let path = directory.path().join(filename);
        let mut file = tempfile::NamedTempFile::new_in(directory.path())?;
        let mut response = self
            .get(&manifest.artifact.url, Duration::from_secs(600))
            .await?;
        if response
            .content_length()
            .is_some_and(|n| n != manifest.artifact.size)
        {
            return Err(invalid(
                "download content length differs from signed artifact size",
            ));
        }
        let mut received = 0u64;
        while let Some(chunk) = response.chunk().await.map_err(http_error)? {
            received = received
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| invalid("download size overflow"))?;
            if received > manifest.artifact.size {
                return Err(invalid("download exceeds signed artifact size"));
            }
            file.write_all(&chunk)?;
        }
        file.as_file().sync_all()?;
        if signature::digest(file.path())?
            != (manifest.artifact.size, manifest.artifact.sha256.clone())
        {
            return Err(invalid(
                "download is truncated or its signed digest does not match",
            ));
        }
        // Check again after the network wait; another accepted descriptor must
        // not turn a long-running download into a rollback.
        self.revalidate(&available.signed)?;
        file.persist_noclobber(&path).map_err(|e| e.error)?;
        let descriptor = format!("gpui-{}.json", manifest.arch);
        let signature_name = format!("{descriptor}.sig");
        for (name, bytes) in [
            (descriptor.as_str(), available.signed.payload.as_slice()),
            (
                signature_name.as_str(),
                available.signed.signature.as_bytes(),
            ),
        ] {
            let mut metadata = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(directory.path().join(name))?;
            metadata.write_all(bytes)?;
            metadata.sync_all()?;
        }
        self.save_high_watermark(&available.signed)?;
        let _ = directory.keep();
        Ok(DownloadedUpdate {
            path,
            signed: available.signed.clone(),
        })
    }
    fn revalidate(&self, signed: &SignedManifest) -> LanpathResult<Manifest> {
        let manifest = signature::verify_manifest(
            &signed.payload,
            &signed.signature,
            crate::product::UPDATE_PUBLIC_KEY,
        )?;
        validate_kind(&manifest)?;
        if !version_precedence(&manifest.version, crate::product::VERSION)?.is_gt() {
            return Err(invalid(
                "update must be newer than the running GPUI version",
            ));
        }
        self.check_high_watermark(&manifest)?;
        Ok(manifest)
    }
    fn state_path(&self) -> PathBuf {
        self.root.path().join("updates/highest-signed.json")
    }
    fn check_high_watermark(&self, manifest: &Manifest) -> LanpathResult<()> {
        crate::data_root::validate_owned_root(self.root.path())?;
        let bytes = match std::fs::read(self.state_path()) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        if bytes.len() > DESCRIPTOR_LIMIT * 2 {
            return Err(invalid("update state exceeds limit"));
        }
        let saved: SavedManifest = serde_json::from_slice(&bytes)?;
        let previous = signature::verify_manifest(
            saved.payload.as_bytes(),
            &saved.signature,
            crate::product::UPDATE_PUBLIC_KEY,
        )?;
        if version_precedence(&manifest.version, &previous.version)?.is_lt() {
            return Err(invalid(
                "update is older than a previously verified download",
            ));
        }
        Ok(())
    }
    fn save_high_watermark(&self, signed: &SignedManifest) -> LanpathResult<()> {
        self.check_high_watermark(&signed.manifest)?;
        super::migration::write_transaction_json(
            &self.state_path(),
            &SavedManifest {
                payload: String::from_utf8(signed.payload.clone())
                    .map_err(|_| invalid("manifest is not UTF-8"))?,
                signature: signed.signature.clone(),
            },
        )
    }
}
// SemVer build metadata identifies an artifact, never upgrade/downgrade order.
// All three gates (offer, download and saved high-watermark) use this policy,
// matching the native payload transaction's cmp_precedence comparison.
fn version_precedence(proposed: &str, current: &str) -> LanpathResult<std::cmp::Ordering> {
    let proposed =
        semver::Version::parse(proposed).map_err(|_| invalid("invalid signed version"))?;
    let current =
        semver::Version::parse(current).map_err(|_| invalid("invalid comparison version"))?;
    Ok(proposed.cmp_precedence(&current))
}
fn validate_kind(manifest: &Manifest) -> LanpathResult<()> {
    let suffix = "-portable.zip";
    let expected = format!(
        "{}/releases/download/gpui-v{}/LanpathGPUI_{}_windows_{}{suffix}",
        crate::product::REPOSITORY,
        manifest.version,
        manifest.version,
        manifest.arch
    );
    if manifest.artifact.url != expected {
        return Err(invalid("signed update artifact must be the GPUI ZIP"));
    }
    Ok(())
}
fn http_error(error: reqwest::Error) -> lanpath_common::LanpathError {
    invalid(format!(
        "GPUI update request failed: {}",
        error.without_url()
    ))
}

#[cfg(test)]
#[path = "updater_tests.rs"]
mod tests;
