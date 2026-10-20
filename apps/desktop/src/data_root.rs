//! An explicitly owned experimental directory, guarded by a lifetime OS file lock.

use lanpath_common::{LanpathError, LanpathResult};
use std::fs::{self, File, Metadata, OpenOptions, TryLockError};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

const OWNER_FILE: &str = ".lanpath-experimental";
const OWNER_CONTENT: &[u8] = b"lanpath-experimental-data-root-v1\n";
const LOCK_FILE: &str = ".lanpath-experimental.lock";

pub(crate) struct DataRoot {
    path: PathBuf,
    // Never unlink this file: contenders must always lock the same inode.
    _lock: File,
}

impl DataRoot {
    pub(crate) fn open(path: &Path) -> LanpathResult<Self> {
        if path.as_os_str().is_empty() {
            return Err(invalid("the experimental data directory must not be empty"));
        }
        if path.components().any(|part| part == Component::ParentDir) {
            return Err(invalid("the experimental data path must not contain '..'"));
        }
        let absolute = std::path::absolute(path)?;
        // All production/foreign-directory checks precede even mkdir or lock creation.
        check_ancestors(&absolute)?;
        let _ = needs_initialization(&absolute)?;
        fs::create_dir_all(&absolute)?;
        check_ancestors(&absolute)?;
        let path = absolute.canonicalize()?;
        check_ancestors(&path)?;
        let initialize = needs_initialization(&path)?;
        let lock_path = path.join(LOCK_FILE);
        let mut options = file_options();
        options.read(true).write(true);
        if initialize {
            options.create_new(true);
        } else {
            // A missing lock in an owned root is not recreated: an older holder
            // could still own the now-unlinked inode.
            check_regular_file(&lock_path)?;
        }
        let lock = options.open(&lock_path)?;
        check_open_file(&lock, &lock_path)?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(invalid("the experimental data directory is already in use"));
            }
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
        check_ancestors(&path)?;
        check_open_file(&lock, &lock_path)?;
        if initialize {
            // Another writer must not turn an initially empty directory into a
            // foreign directory while we are claiming it.
            for entry in fs::read_dir(&path)? {
                if entry?.file_name() != LOCK_FILE {
                    return Err(invalid(
                        "the experimental data directory changed during initialization",
                    ));
                }
            }
            let owner_path = path.join(OWNER_FILE);
            let mut owner = file_options()
                .write(true)
                .create_new(true)
                .open(&owner_path)?;
            owner.write_all(OWNER_CONTENT)?;
            owner.sync_all()?;
        }
        check_owner(&path)?;
        check_tree(&path)?;
        fs::create_dir_all(path.join("logs"))?;
        fs::create_dir_all(path.join("cache"))?;
        Ok(Self { path, _lock: lock })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for DataRoot {
    fn drop(&mut self) {
        let _ = self._lock.unlock();
    }
}

pub(crate) fn default_data_dir() -> LanpathResult<PathBuf> {
    #[cfg(windows)]
    {
        Ok(environment_directory("LOCALAPPDATA")?.join("LanpathExperimental"))
    }
    #[cfg(not(windows))]
    {
        let base = match std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
            Some(value) => absolute_environment_directory("XDG_DATA_HOME", value)?,
            None => environment_directory("HOME")?.join(".local/share"),
        };
        Ok(base.join("lanpath-experimental"))
    }
}

fn environment_directory(name: &str) -> LanpathResult<PathBuf> {
    let value = std::env::var_os(name).ok_or_else(|| {
        invalid(format!(
            "{name} is required for a persistent experimental data directory"
        ))
    })?;
    absolute_environment_directory(name, value)
}

fn absolute_environment_directory(name: &str, value: std::ffi::OsString) -> LanpathResult<PathBuf> {
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(invalid(format!(
            "{name} must name an absolute persistent directory"
        )));
    }
    Ok(path)
}

fn needs_initialization(root: &Path) -> LanpathResult<bool> {
    let Some(metadata) = metadata_if_present(root)? else {
        return Ok(true);
    };
    if is_link(&metadata) || !metadata.is_dir() {
        return Err(invalid(
            "the experimental data root must be an ordinary directory",
        ));
    }
    if fs::read_dir(root)?.next().transpose()?.is_none() {
        return Ok(true);
    }
    check_owner(root)?;
    Ok(false)
}

fn check_owner(root: &Path) -> LanpathResult<()> {
    let path = root.join(OWNER_FILE);
    let metadata = check_regular_file(&path).map_err(|_| {
        invalid("refusing a nonempty directory without the experimental ownership marker")
    })?;
    if metadata.len() != OWNER_CONTENT.len() as u64 {
        return Err(invalid(
            "the experimental ownership marker has an unknown version or contents",
        ));
    }
    let mut file = file_options().read(true).open(&path)?;
    check_open_file(&file, &path)?;
    let mut content = [0; OWNER_CONTENT.len()];
    file.read_exact(&mut content)?;
    let mut extra = [0];
    if content != OWNER_CONTENT || file.read(&mut extra)? != 0 {
        return Err(invalid(
            "the experimental ownership marker has an unknown version or contents",
        ));
    }
    Ok(())
}

fn check_ancestors(path: &Path) -> LanpathResult<()> {
    // Once the exact experimental marker owns the root, its own client.json is
    // application state rather than evidence that this is a production root.
    // Ancestor client.json files remain forbidden.
    let owned_self = path.is_dir() && check_owner(path).is_ok();
    for (depth, ancestor) in path.ancestors().enumerate() {
        if ancestor.file_name().is_some_and(|name| {
            [".lanpath", ".proxy-panel-client", "com.ybakiame.lanpath"]
                .iter()
                .any(|production| {
                    name.to_str()
                        .is_some_and(|name| name.eq_ignore_ascii_case(production))
                })
        }) {
            return Err(invalid(
                "production Lanpath data roots and their descendants are forbidden",
            ));
        }
        let Some(metadata) = metadata_if_present(ancestor)? else {
            continue;
        };
        if is_link(&metadata) || !metadata.is_dir() {
            return Err(invalid(format!(
                "data path contains a symlink, junction, reparse point or non-directory: {}",
                ancestor.display()
            )));
        }
        // Probe metadata only; never open production configuration or seed data.
        let production_marker = (depth != 0 || !owned_self)
            && (metadata_if_present(&ancestor.join("client.json"))?.is_some()
                || metadata_if_present(&ancestor.join("lanpath.portable"))?.is_some());
        let portable_layout = (metadata_if_present(&ancestor.join("Lanpath.exe"))?.is_some()
            || metadata_if_present(&ancestor.join("lanpath-app.exe"))?.is_some())
            && (metadata_if_present(&ancestor.join("data"))?.is_some()
                || metadata_if_present(&ancestor.join("seed"))?.is_some());
        if production_marker || portable_layout {
            return Err(invalid(format!(
                "refusing a production/Portable Lanpath ancestor: {}",
                ancestor.display()
            )));
        }
    }
    Ok(())
}

fn check_tree(root: &Path) -> LanpathResult<()> {
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if is_link(&metadata) {
                return Err(invalid(format!(
                    "experimental data must not contain symlinks or junctions: {}",
                    path.display()
                )));
            }
            if metadata.is_dir() {
                directories.push(path);
            } else if !metadata.is_file() || has_multiple_links(&path, &metadata)? {
                return Err(invalid(format!(
                    "experimental data must contain only ordinary, unshared files: {}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

fn metadata_if_present(path: &Path) -> LanpathResult<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn check_regular_file(path: &Path) -> LanpathResult<Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    if is_link(&metadata) || !metadata.is_file() || has_multiple_links(path, &metadata)? {
        return Err(invalid(format!(
            "an ordinary, unshared file is required: {}",
            path.display()
        )));
    }
    Ok(metadata)
}

fn check_open_file(file: &File, path: &Path) -> LanpathResult<()> {
    let pathname = check_regular_file(path)?;
    let opened = file.metadata()?;
    #[cfg(windows)]
    let shared = crate::windows_file::link_count(file)? != 1;
    #[cfg(not(windows))]
    let shared = has_multiple_links(path, &opened)?;
    if !opened.is_file() || is_link(&opened) || shared {
        return Err(invalid(
            "the ownership/lock handle is not an ordinary, unshared file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if pathname.dev() != opened.dev() || pathname.ino() != opened.ino() {
            return Err(invalid("the ownership/lock file changed while opening it"));
        }
    }
    #[cfg(not(unix))]
    let _ = pathname;
    Ok(())
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Includes junctions and other reparse points, not just symlinks.
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn has_multiple_links(path: &Path, metadata: &Metadata) -> LanpathResult<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = path;
        Ok(metadata.nlink() != 1)
    }
    #[cfg(windows)]
    {
        let _ = metadata;
        let file = file_options().read(true).open(path)?;
        Ok(crate::windows_file::link_count(&file)? != 1)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (path, metadata);
        Err(invalid("file link counts are unsupported on this platform"))
    }
}

fn file_options() -> OpenOptions {
    #[allow(unused_mut)]
    let mut options = OpenOptions::new();
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Open the reparse point itself, never its destination. Keep read/write
        // sharing for lock contention, but deny deletion while a handle lives.
        options.custom_flags(0x0020_0000).share_mode(0x1 | 0x2);
    }
    options
}

fn invalid(message: impl Into<String>) -> LanpathError {
    LanpathError::Config(message.into())
}

#[cfg(test)]
#[path = "data_root_tests.rs"]
mod tests;
