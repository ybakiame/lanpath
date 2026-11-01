//! Download-related private methods and free functions for core management.

use std::path::{Path, PathBuf};

use lanpath_common::{CoreType, LanpathError, LanpathResult};

use super::version::{binary_file_name, binary_name_on_disk_for};

/// Extract `.tar.gz` and retrieve target binary.
pub(super) fn extract_tgz(
    archive: &Path,
    dest_dir: &Path,
    target_name: &str,
) -> LanpathResult<PathBuf> {
    let file = std::fs::File::open(archive)?;
    let tar = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(tar);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if file_name == target_name || file_name == format!("{target_name}.exe") {
            if !entry.header().entry_type().is_file() {
                return Err(super::safety::invalid(
                    "Core archive binary must be a regular file",
                ));
            }
            let dest = dest_dir.join(binary_file_name(target_name));
            entry.unpack(&dest)?;
            return Ok(dest);
        }
    }
    Err(LanpathError::Core(format!(
        "Binary {target_name} not found in archive"
    )))
}

/// Extract `.zip` and retrieve target binary.
pub(super) fn extract_zip(
    archive: &Path,
    dest_dir: &Path,
    target_name: &str,
    core_type: CoreType,
) -> LanpathResult<PathBuf> {
    let file = std::fs::File::open(archive)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| LanpathError::Core(format!("Invalid zip archive: {e}")))?;
    let mut binary_dest: Option<PathBuf> = None;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| LanpathError::Core(format!("Zip entry error: {e}")))?;
        let file_name = std::path::Path::new(entry.name())
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let matches_binary = match core_type {
            CoreType::SingBox => {
                file_name.eq_ignore_ascii_case(target_name)
                    || file_name.eq_ignore_ascii_case(&format!("{target_name}.exe"))
            }
            CoreType::Mihomo => {
                file_name.to_ascii_lowercase().starts_with("mihomo")
                    && file_name.to_ascii_lowercase().ends_with(".exe")
            }
        };
        if matches_binary {
            if binary_dest.is_some() {
                return Err(super::safety::invalid(
                    "Core archive contains multiple executable candidates",
                ));
            }
            if entry.is_dir()
                || entry
                    .unix_mode()
                    .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                return Err(super::safety::invalid(
                    "Core archive binary must be a regular file",
                ));
            }
            let dest = dest_dir.join(binary_file_name(target_name));
            let mut out = std::fs::File::create(&dest)?;
            std::io::copy(&mut entry, &mut out)?;
            binary_dest = Some(dest);
        }
    }
    binary_dest
        .ok_or_else(|| LanpathError::Core(format!("Binary {target_name} not found in archive")))
}

/// Extract a standalone gzip-compressed executable, as published by Mihomo on Unix.
pub(super) fn extract_gz(
    archive: &Path,
    dest_dir: &Path,
    target_name: &str,
) -> LanpathResult<PathBuf> {
    let file = std::fs::File::open(archive)?;
    let mut decoder = flate2::read::GzDecoder::new(file);
    let dest = dest_dir.join(binary_file_name(target_name));
    let mut output = std::fs::File::create(&dest)?;
    std::io::copy(&mut decoder, &mut output)?;
    Ok(dest)
}

/// chmod 755 (Unix).
pub(super) fn set_executable(path: &Path) -> LanpathResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(perms.mode() | 0o755);
        std::fs::set_permissions(path, perms)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Extract a single file from a zip archive by suffix path match (e.g. `bin/amd64/wintun.dll`)
/// to `dest`.
#[cfg(target_os = "windows")]
pub(super) fn extract_zip_entry(archive: &Path, suffix: &str, dest: &Path) -> LanpathResult<()> {
    let file = std::fs::File::open(archive)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| LanpathError::Core(format!("Invalid zip archive: {e}")))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| LanpathError::Core(format!("Zip entry error: {e}")))?;
        // zip 内路径统一为正斜杠；按后缀匹配避免依赖顶层目录名（wintun-x.y.z/ 前缀随版本变）。
        let name = entry.name().replace('\\', "/");
        if name.ends_with(suffix) {
            let mut out = std::fs::File::create(dest)?;
            std::io::copy(&mut entry, &mut out)?;
            return Ok(());
        }
    }
    Err(LanpathError::Core(format!(
        "Entry {suffix} not found in archive"
    )))
}

/// Windows：确保核心目录内存在 `wintun.dll`（sing-box TUN 依赖，核心不内嵌）。
///
/// 已存在则跳过；否则从 wintun.net 官方发布下载 zip（版本固定，避免最新版链接漂移），
/// 按当前架构提取 `bin/<arch>/wintun.dll` 落到核心目录。
#[cfg(target_os = "windows")]
pub(super) async fn ensure_wintun(client: &reqwest::Client, core_dir: &Path) -> LanpathResult<()> {
    const WINTUN_VERSION: &str = "0.14.1";
    let dest = core_dir.join("wintun.dll");
    if dest.is_file() {
        return Ok(());
    }
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => {
            return Err(LanpathError::Core(format!(
                "Unsupported Windows arch for wintun: {other}"
            )));
        }
    };
    let url = format!("https://www.wintun.net/builds/wintun-{WINTUN_VERSION}.zip");
    let tmp = core_dir.join(".wintun-download.zip");
    download_to(client, &url, &tmp).await?;
    let tmp_clone = tmp.clone();
    let dest_clone = dest.clone();
    let suffix = format!("bin/{arch}/wintun.dll");
    let result =
        tokio::task::spawn_blocking(move || extract_zip_entry(&tmp_clone, &suffix, &dest_clone))
            .await
            .map_err(|e| LanpathError::Core(format!("wintun extraction task failed: {e}")))?;
    let _ = std::fs::remove_file(&tmp);
    result
}

/// Download `url` to `dest`（分块流式落盘；供 ensure_wintun 复用，独立于
/// `ClientCoreInventory::download_to` 以保持本模块自包含）。
#[cfg(target_os = "windows")]
async fn download_to(client: &reqwest::Client, url: &str, dest: &Path) -> LanpathResult<()> {
    use tokio::io::AsyncWriteExt;

    let mut resp = client
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

/// Publish a verified core without replacing an installed binary or removing
/// other files left in an older, incomplete version directory.
pub(super) fn publish_core(
    staging: &Path,
    destination: &Path,
    core_type: CoreType,
) -> LanpathResult<PathBuf> {
    let name = binary_name_on_disk_for(core_type);
    let binary = destination.join(&name);
    if !destination.exists() {
        std::fs::rename(staging, destination)?;
        return Ok(binary);
    }
    super::safety::unlinked_path(destination)?;
    if !destination.is_dir() {
        return Err(super::safety::invalid(
            "Core version path is not a directory",
        ));
    }
    #[cfg(windows)]
    {
        let library = staging.join("wintun.dll");
        let installed = destination.join("wintun.dll");
        if library.is_file()
            && !installed.exists()
            && let Err(error) =
                tempfile::TempPath::try_from_path(library)?.persist_noclobber(installed)
        {
            tracing::warn!(error = %error.error, "wintun.dll 发布失败，TUN 模式将不可用");
        }
    }
    tempfile::TempPath::try_from_path(staging.join(name))?
        .persist_noclobber(&binary)
        .map_err(|error| error.error)?;
    Ok(binary)
}
