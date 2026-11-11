use super::invalid;
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use lanpath_common::LanpathResult;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    io::Read,
    path::{Component, Path},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub schema: u32,
    pub product: String,
    pub channel: String,
    pub version: String,
    pub arch: String,
    pub artifact: Artifact,
    pub files: Vec<BundleFile>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Artifact {
    pub url: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BundleFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

pub(crate) fn verify_manifest(
    payload: &[u8],
    signature_base64: &str,
    public_key_base64: &str,
) -> LanpathResult<Manifest> {
    if payload.len() > 128 * 1024 {
        return Err(invalid("signed manifest exceeds 128 KiB"));
    }
    let key = decode_base64(public_key_base64, "update public key")?;
    let key: [u8; 32] = key
        .try_into()
        .map_err(|_| invalid("update public key must contain 32 bytes"))?;
    let signature = decode_base64(signature_base64.trim(), "update signature")?;
    let signature = Signature::from_slice(&signature)
        .map_err(|_| invalid("invalid update signature length"))?;
    VerifyingKey::from_bytes(&key)
        .map_err(|_| invalid("invalid update public key"))?
        .verify_strict(payload, &signature)
        .map_err(|_| invalid("update signature verification failed"))?;
    let manifest: Manifest = serde_json::from_slice(payload)?;
    if manifest.schema != 1
        || manifest.product != crate::product::APP_ID
        || manifest.channel != crate::product::UPDATE_CHANNEL
        || manifest.arch != std::env::consts::ARCH
    {
        return Err(invalid(
            "signed update has wrong schema, product, channel or architecture",
        ));
    }
    semver::Version::parse(&manifest.version)
        .map_err(|_| invalid("invalid signed update version"))?;
    validate_artifact_identity(&manifest)?;
    validate_digest(&manifest.artifact.sha256)?;
    if manifest.artifact.size == 0 || manifest.artifact.size > 1024 * 1024 * 1024 {
        return Err(invalid("update artifact size is outside 1 byte..1 GiB"));
    }
    let mut names = HashSet::new();
    if manifest.files.len() != crate::product::TRUSTED_BUNDLE_FILES.len() {
        return Err(invalid(
            "signed bundle must contain the exact GPUI product file list",
        ));
    }
    for file in &manifest.files {
        safe_relative(&file.path)?;
        if !names.insert(file.path.to_ascii_lowercase()) {
            return Err(invalid("duplicate signed bundle path"));
        }
        if !crate::product::TRUSTED_BUNDLE_FILES.contains(&file.path.as_str()) {
            return Err(invalid("signed bundle contains a foreign product file"));
        }
        validate_digest(&file.sha256)?;
        if file.size == 0 || file.size > 1024 * 1024 * 1024 {
            return Err(invalid("signed bundle file is empty or exceeds limit"));
        }
    }
    Ok(manifest)
}

fn decode_base64(value: &str, description: &str) -> LanpathResult<Vec<u8>> {
    let bytes = STANDARD
        .decode(value)
        .map_err(|_| invalid(format!("invalid {description} encoding")))?;
    if STANDARD.encode(&bytes) != value {
        return Err(invalid(format!("noncanonical {description} encoding")));
    }
    Ok(bytes)
}

fn validate_artifact_identity(manifest: &Manifest) -> LanpathResult<()> {
    let url = https_url(&manifest.artifact.url)?;
    if url.query().is_some() {
        return Err(invalid("signed artifact URL must not contain a query"));
    }
    let prefix = format!(
        "{}/releases/download/gpui-v{}/",
        crate::product::REPOSITORY,
        manifest.version
    );
    let name = manifest
        .artifact
        .url
        .strip_prefix(&prefix)
        .ok_or_else(|| invalid("signed artifact is outside the GPUI version release"))?;
    let stem = format!("LanpathGPUI_{}_windows_{}", manifest.version, manifest.arch);
    if name != crate::product::BINARY_NAME && name != format!("{stem}-portable.zip") {
        return Err(invalid(
            "signed artifact name does not match the GPUI version and architecture",
        ));
    }
    Ok(())
}

pub(crate) fn https_url(value: &str) -> LanpathResult<reqwest::Url> {
    let url = reqwest::Url::parse(value).map_err(|_| invalid("invalid update URL"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid(
            "update URLs require HTTPS without credentials or fragments",
        ));
    }
    Ok(url)
}

pub(crate) fn safe_relative(name: &str) -> LanpathResult<()> {
    if name.is_empty()
        || name.contains(['\\', ':'])
        || name.len() > 1024
        || Path::new(name)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(invalid("unsafe bundle path"));
    }
    for part in name.split('/') {
        let stem = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if part.is_empty()
            || matches!(part, "." | "..")
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|c| c.is_control() || "<>\"|?*".contains(c))
            || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {
            return Err(invalid("unsafe Windows bundle filename"));
        }
    }
    Ok(())
}
fn validate_digest(value: &str) -> LanpathResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(invalid("invalid SHA-256 digest"));
    }
    Ok(())
}
pub(crate) fn digest(path: &Path) -> LanpathResult<(u64, String)> {
    digest_reader(&mut std::fs::File::open(path)?)
}

pub(crate) fn digest_reader(file: &mut impl Read) -> LanpathResult<(u64, String)> {
    let mut hash = Sha256::new();
    let mut total = 0;
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        hash.update(&buffer[..n]);
    }
    Ok((total, format!("{:x}", hash.finalize())))
}

/// Descriptor artifact fields differ, but both layers must authorize the same
/// independent release and exact file set.
#[cfg(any(windows, test))]
pub(crate) fn bundle_identity(manifest: &Manifest) -> LanpathResult<String> {
    let bytes = serde_json::to_vec(&(
        &manifest.product,
        &manifest.channel,
        &manifest.arch,
        &manifest.version,
        &manifest.files,
    ))?;
    Ok(digest_reader(&mut bytes.as_slice())?.1)
}

#[cfg(test)]
#[path = "signature_tests.rs"]
mod tests;
