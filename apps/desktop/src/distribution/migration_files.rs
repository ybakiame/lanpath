use crate::distribution::{
    invalid,
    signature::{digest, safe_relative},
};
use lanpath_common::LanpathResult;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

pub(super) const CONFIG: &[&str] = &[
    "client.json",
    "subscriptions.json",
    "profiles.json",
    "local_override.json",
    "config_slices.json",
    "remotes.json",
    "subscription_cache",
    "profile_cache",
    "scripts",
    "remote_cache",
    "script_store",
    "rulesets",
];
pub(super) const HISTORY: &[&str] = &[
    "stats.db",
    "stats.db-wal",
    "stats.db-shm",
    "stats.db-journal",
];
const MAX_FILES: usize = 16384;
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FileStamp {
    pub size: u64,
    pub sha256: String,
}
pub(super) fn inventory(
    root: &Path,
    roots: &[String],
) -> LanpathResult<BTreeMap<String, FileStamp>> {
    crate::data_root::validate_tree(root)?;
    let mut result = BTreeMap::new();
    let mut pending: Vec<PathBuf> = roots.iter().map(PathBuf::from).collect();
    let mut folded = HashSet::new();
    let mut bytes = 0u64;
    while let Some(relative) = pending.pop() {
        let name = relative
            .to_str()
            .ok_or_else(|| invalid("migration path is not UTF-8"))?
            .replace('\\', "/");
        safe_relative(&name)?;
        let path = root.join(&relative);
        let meta = match std::fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        if !folded.insert(name.to_ascii_lowercase()) {
            return Err(invalid("migration has case-insensitive filename conflicts"));
        }
        if meta.is_dir() {
            for entry in std::fs::read_dir(path)? {
                pending.push(relative.join(entry?.file_name()));
            }
        } else if meta.is_file() {
            // The SHM file contains transient read locks, not durable history.
            if name.ends_with(".db-shm") {
                continue;
            }
            let (size, sha256) = digest(&path)?;
            bytes = bytes
                .checked_add(size)
                .ok_or_else(|| invalid("migration size overflow"))?;
            if bytes > MAX_BYTES || result.len() >= MAX_FILES {
                return Err(invalid("migration exceeds 2 GiB or 16384 files"));
            }
            result.insert(name, FileStamp { size, sha256 });
        } else {
            return Err(invalid("migration source contains special files"));
        }
    }
    Ok(result)
}
pub(super) fn copy_tree(from: &Path, to: &Path) -> LanpathResult<()> {
    if !from.try_exists()? {
        return Ok(());
    }
    let meta = std::fs::symlink_metadata(from)?;
    if meta.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else if meta.is_file() && !meta.file_type().is_symlink() {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(from, to)?;
        std::fs::OpenOptions::new()
            .write(true)
            .open(to)?
            .sync_all()?;
    } else {
        return Err(invalid("migration copy refuses links and special files"));
    }
    Ok(())
}
pub(super) fn remove(path: &Path) -> LanpathResult<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path)?,
        Ok(_) => std::fs::remove_file(path)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
pub(super) fn atomic_json<T: Serialize>(path: &Path, value: &T) -> LanpathResult<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| invalid("transaction path has no parent"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut temporary, value)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|e| e.error)?;
    #[cfg(unix)]
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}
pub(super) fn normalize_history(stage: &Path) -> LanpathResult<()> {
    let file = stage.join("stats.db");
    if !file.try_exists()? {
        if stage.join("stats.db-wal").try_exists()? {
            return Err(invalid("history WAL has no matching database"));
        }
        return Ok(());
    }
    // SQLite only opens this hash-verified PRIVATE copy. Opening the original
    // read-only could still create/change SHM or recover a hot journal.
    let connection = rusqlite::Connection::open(&file).map_err(sqlite_error)?;
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(sqlite_error)?;
    if integrity != "ok" {
        return Err(invalid(format!("history is corrupt: {integrity}")));
    }
    let snapshot = tempfile::NamedTempFile::new_in(stage)?;
    let mut output = rusqlite::Connection::open(snapshot.path()).map_err(sqlite_error)?;
    {
        let backup =
            rusqlite::backup::Backup::new(&connection, &mut output).map_err(sqlite_error)?;
        backup
            .run_to_completion(128, std::time::Duration::from_millis(5), None)
            .map_err(sqlite_error)?;
    }
    output
        .execute_batch("PRAGMA journal_mode=DELETE;")
        .map_err(sqlite_error)?;
    drop(output);
    drop(connection);
    snapshot.as_file().sync_all()?;
    // Persist after all SQLite handles are closed (also required on Windows).
    snapshot.persist(&file).map_err(|e| e.error)?;
    remove(&stage.join("stats.db-wal"))?;
    remove(&stage.join("stats.db-shm"))?;
    Ok(())
}
fn sqlite_error(error: rusqlite::Error) -> lanpath_common::LanpathError {
    invalid(format!("history snapshot failed: {error}"))
}
pub(super) fn relocate(
    stage: &Path,
    source: &Path,
    target: &Path,
    import: bool,
) -> LanpathResult<()> {
    let inventory = inventory(
        stage,
        &CONFIG.iter().map(|s| (*s).into()).collect::<Vec<_>>(),
    )?;
    let source = source.to_string_lossy();
    let target_path = target.to_string_lossy();
    use lanpath_client::backup::path_relocation::{document_has_paths, relocate_document_paths};
    for name in inventory.keys().filter(|name| document_has_paths(name)) {
        let path = stage.join(name);
        let mut value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        if relocate_document_paths(name, &mut value, &source, &target_path) {
            atomic_json(&path, &value)?;
        }
    }
    let path = stage.join("client.json");
    if path.try_exists()? {
        let mut raw: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        if raw.get("request_proxy_mode").is_none() {
            raw["request_proxy_mode"] = if raw["fetch_via_local_proxy"] == true {
                "core"
            } else {
                "none"
            }
            .into();
        }
        let mut config: lanpath_client::ClientConfig = serde_json::from_value(raw)?;
        // Do not call ClientConfig::load: its legacy migrations write the input.
        config.data_dir = target.into();
        if import {
            // Import never replaces the target user's selected core with a
            // source binary; owned-root moves already rewrote managed paths.
            config.core_binary = match std::fs::read(target.join("client.json")) {
                Ok(bytes) => {
                    serde_json::from_slice::<lanpath_client::ClientConfig>(&bytes)?.core_binary
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => PathBuf::new(),
                Err(e) => return Err(e.into()),
            };
        }
        config.mitm.ca_dir = target.join("certs");
        if import {
            config.system_proxy_enabled = false;
            config.tun_enabled = false;
            config.mitm_enabled = false;
        }
        config.validate_request_proxy()?;
        atomic_json(&path, &config)?;
    }
    // Validate typed configuration and associations without writing to the source.
    lanpath_client::backup::capture(stage, None)?;
    Ok(())
}

pub(super) fn sync_directory(path: &Path) -> LanpathResult<()> {
    #[cfg(unix)]
    std::fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
