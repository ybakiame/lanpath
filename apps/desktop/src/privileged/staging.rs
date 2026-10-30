use super::{
    protocol::{Resource, failure},
    security,
};
use lanpath_common::LanpathResult;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
};

pub(super) struct TrustedBundle {
    pub root: PathBuf,
    pub image_sha256: String,
    pub core_sha256: String,
    pub core: File,
    // Hold every verified input and its complete directory chain through execution.
    _image: File,
    manifest: crate::distribution::signature::Manifest,
    payload: Vec<u8>,
    signature: Vec<u8>,
    _directories: crate::windows_file::DirectoryLocks,
    _files: std::collections::BTreeMap<String, File>,
}
impl TrustedBundle {
    /// Existing final payloads are never overwritten. An interrupted copy uses
    /// a separate reserved name until this exact content check succeeds.
    pub fn verify_file(&self, relative: &Path, actual: &Path) -> LanpathResult<()> {
        let mut file = locked(actual)?;
        let (size, digest) = if relative == Path::new("bundle.json") {
            (
                self.payload.len() as u64,
                format!("{:x}", Sha256::digest(&self.payload)),
            )
        } else if relative == Path::new("bundle.json.sig") {
            (
                self.signature.len() as u64,
                format!("{:x}", Sha256::digest(&self.signature)),
            )
        } else {
            let entry = self
                .manifest
                .files
                .iter()
                .find(|entry| Path::new(&entry.path) == relative)
                .ok_or_else(|| failure("unshipped helper payload"))?;
            (entry.size, entry.sha256.to_ascii_lowercase())
        };
        if hash(&mut file, size)? != (size, digest) {
            return Err(failure(
                "protected helper payload differs from its authenticated source; no existing payload was overwritten",
            ));
        }
        Ok(())
    }
}

fn hash(file: &mut File, limit: u64) -> LanpathResult<(u64, String)> {
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > limit {
            return Err(failure("trusted artifact exceeds size bound"));
        }
        hash.update(&buffer[..count]);
    }
    file.seek(SeekFrom::Start(0))?;
    Ok((total, format!("{:x}", hash.finalize())))
}

fn locked(path: &Path) -> LanpathResult<File> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    use std::os::windows::fs::MetadataExt;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || crate::windows_file::link_count(&file)? != 1
    {
        return Err(failure(
            "trusted artifact must be a regular unshared non-reparse file",
        ));
    }
    Ok(file)
}
pub(super) fn validate_file_shape(path: &Path) -> LanpathResult<()> {
    drop(locked(path)?);
    Ok(())
}

pub(super) fn trusted_bundle() -> LanpathResult<TrustedBundle> {
    trusted_bundle_at(&std::env::current_exe()?)
}

pub(super) fn trusted_bundle_at(executable: &Path) -> LanpathResult<TrustedBundle> {
    let executable = executable.to_path_buf();
    let root = executable
        .parent()
        .ok_or_else(|| failure("application image has no parent"))?
        .to_path_buf();
    let mut directories = crate::windows_file::DirectoryLocks::acquire(&root)?;
    let mut payload_file = locked(&root.join("bundle.json"))?;
    let payload = read_open_bounded(&mut payload_file, 1024 * 1024)?;
    let mut signature_file = locked(&root.join("bundle.json.sig"))?;
    let signature = read_open_bounded(&mut signature_file, 4096)?;
    let signature_text =
        std::str::from_utf8(&signature).map_err(|_| failure("bundle signature is not UTF-8"))?;
    let manifest = crate::distribution::signature::verify_manifest(
        &payload,
        signature_text.trim(),
        crate::product::UPDATE_PUBLIC_KEY,
    )?;
    if manifest.version != crate::product::VERSION {
        return Err(failure("privileged bundle version mismatch"));
    }
    let declared: std::collections::BTreeSet<&str> = manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let required: std::collections::BTreeSet<&str> = crate::product::TRUSTED_BUNDLE_FILES
        .iter()
        .copied()
        .collect();
    if declared != required {
        return Err(failure("signed privileged bundle file inventory mismatch"));
    }
    for entry in &manifest.files {
        let path = root.join(&entry.path);
        directories.extend(
            path.parent()
                .ok_or_else(|| failure("bundle payload has no parent"))?,
        )?;
    }
    let app = manifest
        .files
        .iter()
        .find(|file| file.path == "lanpath-gpui.exe")
        .ok_or_else(|| failure("signed bundle does not identify the application image"))?;
    let core_entry = manifest
        .files
        .iter()
        .find(|file| file.path == "resources/seed/sing-box.exe")
        .ok_or_else(|| failure("signed bundle does not identify the supported seed core"))?;
    if executable.file_name().and_then(|s| s.to_str()) != Some("lanpath-gpui.exe") {
        return Err(failure("privileged application image name mismatch"));
    }
    let mut image = locked(&executable)?;
    if hash(&mut image, 1024 * 1024 * 1024)? != (app.size, app.sha256.to_ascii_lowercase()) {
        return Err(failure(
            "privileged application image does not match signed bundle",
        ));
    }
    let mut core = locked(&root.join(&core_entry.path))?;
    if hash(&mut core, 256 * 1024 * 1024)?
        != (core_entry.size, core_entry.sha256.to_ascii_lowercase())
    {
        return Err(failure("seed core does not match signed bundle"));
    }
    // Every shipped payload is verified and held before elevation or loader use.
    let mut files = std::collections::BTreeMap::from([
        ("bundle.json".into(), payload_file),
        ("bundle.json.sig".into(), signature_file),
    ]);
    let mut seed = None;
    for entry in &manifest.files {
        if entry.path == "lanpath-gpui.exe" || entry.path == "resources/seed/sing-box.exe" {
            continue;
        }
        let mut file = locked(&root.join(&entry.path))?;
        let limit = if entry.path == "resources/seed/manifest.json" {
            1024 * 1024
        } else {
            32 * 1024 * 1024
        };
        let (size, digest) = hash(&mut file, limit)?;
        if size != entry.size || !digest.eq_ignore_ascii_case(&entry.sha256) {
            return Err(failure("signed bundle payload digest mismatch"));
        }
        if entry.path == "resources/seed/manifest.json" {
            let mut bytes = Vec::with_capacity(size as usize);
            std::io::Read::by_ref(&mut file).read_to_end(&mut bytes)?;
            seed = Some(bytes);
        }
        files.insert(entry.path.clone(), file);
    }
    let seed: serde_json::Value =
        serde_json::from_slice(&seed.ok_or_else(|| failure("signed seed manifest missing"))?)?;
    let expected_arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    };
    if seed.get("schema").and_then(serde_json::Value::as_u64) != Some(1)
        || seed
            .get("sing_box_version")
            .and_then(serde_json::Value::as_str)
            != Some(crate::product::CORE_MANAGED_VERSION)
        || seed
            .get("upstream_version")
            .and_then(serde_json::Value::as_str)
            != Some(crate::product::CORE_UPSTREAM_VERSION)
        || seed.get("go_version").and_then(serde_json::Value::as_str)
            != Some(crate::product::CORE_GO_VERSION)
        || seed
            .get("managed_protocol")
            .and_then(serde_json::Value::as_u64)
            != Some(u64::from(crate::product::CORE_MANAGED_PROTOCOL))
        || seed
            .get("managed_source_digest")
            .and_then(serde_json::Value::as_str)
            != Some(crate::product::CORE_MANAGED_SOURCE_DIGEST)
        || !seed
            .get("core_sha256")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|digest| digest.eq_ignore_ascii_case(&core_entry.sha256))
        || seed.get("arch").and_then(serde_json::Value::as_str) != Some(expected_arch)
    {
        return Err(failure(
            "privileged TUN requires the exact product-bound managed core source/protocol/compiler/architecture/digest tuple",
        ));
    }
    let image_sha256 = app.sha256.clone();
    let core_sha256 = core_entry.sha256.clone();
    Ok(TrustedBundle {
        root,
        image_sha256,
        core_sha256,
        core,
        _image: image,
        manifest,
        payload,
        signature,
        _directories: directories,
        _files: files,
    })
}

pub(super) fn read_bounded(path: &Path, limit: usize) -> LanpathResult<Vec<u8>> {
    read_open_bounded(&mut locked(path)?, limit)
}

fn read_open_bounded(file: &mut File, limit: usize) -> LanpathResult<Vec<u8>> {
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(failure("file exceeds admission bound"));
    }
    Ok(bytes)
}

pub(super) struct Stage {
    pub path: PathBuf,
    pub core_path: PathBuf,
    pub firewall_needed: bool,
    pub cache_enabled: bool,
    core: Option<File>,
    directories: Option<crate::windows_file::DirectoryLocks>,
    cleanup_on_drop: bool,
}
impl Stage {
    pub fn retain_for_recovery(&mut self) {
        self.cleanup_on_drop = false;
    }
    pub fn create(
        root: &Path,
        session: &str,
        interface: &str,
        trusted: &mut TrustedBundle,
        mut config: serde_json::Value,
        resources: &[Resource],
    ) -> LanpathResult<Self> {
        if uuid::Uuid::parse_str(session).is_err() {
            return Err(failure("invalid session identity"));
        }
        let path = root.join(session);
        security::private_directory(&path)?;
        // Own the fresh directory immediately. Failure cannot leave an executable
        // stage behind, and the core image is closed before removing it on Windows.
        let firewall_needed = config
            .get("inbounds")
            .and_then(serde_json::Value::as_array)
            .and_then(|items| items.iter().find(|item| item["type"] == "tun"))
            .and_then(|item| item.get("stack"))
            .and_then(serde_json::Value::as_str)
            != Some("gvisor");
        let cache_enabled = config
            .pointer("/experimental/cache_file/enabled")
            .and_then(serde_json::Value::as_bool)
            == Some(true);
        let mut stage = Self {
            core_path: PathBuf::new(),
            path,
            firewall_needed,
            cache_enabled,
            core: None,
            directories: None,
            cleanup_on_drop: true,
        };
        stage.path = std::fs::canonicalize(&stage.path)?;
        stage.directories = Some(crate::windows_file::DirectoryLocks::acquire(&stage.path)?);
        stage.core_path = stage.path.join("sing-box.exe");
        super::config::admit(&mut config, resources, &stage.path, interface)?;
        for directory in ["resources", "logs", "cache"] {
            security::private_directory(&stage.path.join(directory))?;
        }
        for resource in resources {
            write_new(
                &stage.path.join("resources").join(&resource.name),
                &resource.bytes,
            )?;
        }
        // Independent authority: unknown/case-aliased clock, proxy, API, filesystem,
        // outbound and other capabilities fail before the core is copied or run.
        crate::configuration::check_privileged_document(&stage.path, &config)?;
        copy_verified(&mut trusted.core, &stage.core_path, &trusted.core_sha256)?;
        write_new(
            &stage.path.join("config.json"),
            &serde_json::to_vec(&config)?,
        )?;
        stage.core = Some(locked(&stage.core_path)?);
        Ok(stage)
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        self.core.take();
        self.directories.take();
        if self.cleanup_on_drop {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

fn copy_verified(input: &mut File, output: &Path, expected: &str) -> LanpathResult<()> {
    use std::io::{Seek, SeekFrom};
    input.seek(SeekFrom::Start(0))?;
    let mut destination = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    std::io::copy(input, &mut destination)?;
    destination.sync_all()?;
    drop(destination);
    let mut verify = locked(output)?;
    if hash(&mut verify, 1024 * 1024 * 1024)?.1 != expected.to_ascii_lowercase() {
        return Err(failure("protected copy digest mismatch"));
    }
    Ok(())
}

pub(super) fn write_new(path: &Path, bytes: &[u8]) -> LanpathResult<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

pub(super) fn lock_parent_image(path: &Path, expected: &str) -> LanpathResult<File> {
    let mut image = locked(path)?;
    if hash(&mut image, 1024 * 1024 * 1024)?.1 != expected.to_ascii_lowercase() {
        return Err(failure("pipe server is not the signed product image"));
    }
    Ok(image)
}
