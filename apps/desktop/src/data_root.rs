//! Independent GPUI data directory, guarded by a lifetime OS file lock.

use lanpath_common::{LanpathError, LanpathResult};
use std::fs::{self, File, Metadata, OpenOptions, TryLockError};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

#[path = "data_root_identity.rs"]
mod identity;

const OWNER_FILE: &str = ".lanpath-gpui";
const OWNER_CONTENT: &[u8] = b"lanpath-gpui-data-root-v1\n";
const LOCK_FILE: &str = ".lanpath-gpui.lock";

pub(crate) struct DataRoot {
    path: PathBuf,
    id: uuid::Uuid,
    // Never unlink this file: contenders must always lock the same inode.
    _lock: File,
    #[cfg(windows)]
    _directories: crate::windows_file::DirectoryLocks,
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
        #[cfg(not(windows))]
        fs::create_dir_all(&absolute)?;
        #[cfg(windows)]
        let directories = crate::windows_access::private_root(&absolute)?;
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
            // Restrict the newly claimed root before writing identity/secrets.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
            }
            // Another writer must not turn this empty root into a foreign one.
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
        let id = identity::load_or_create(&path)?;
        Ok(Self {
            path,
            id,
            _lock: lock,
            #[cfg(windows)]
            _directories: directories,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn id(&self) -> uuid::Uuid {
        self.id
    }

    pub(crate) fn is_in_use(path: &Path) -> LanpathResult<bool> {
        let path = validate_owned_root(path)?;
        let lock_path = path.join(LOCK_FILE);
        let file = file_options().read(true).write(true).open(&lock_path)?;
        check_open_file(&file, &lock_path)?;
        match file.try_lock() {
            Ok(()) => {
                file.unlock()?;
                Ok(false)
            }
            Err(TryLockError::WouldBlock) => Ok(true),
            Err(TryLockError::Error(error)) => Err(error.into()),
        }
    }

    pub(crate) fn lock_activation(&self) -> LanpathResult<File> {
        validate_owned_root(self.path())?;
        let path = self.path.join(".gpui-activation.lock");
        let file = file_options()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)?;
        check_open_file(&file, &path)?;
        match file.try_lock() {
            Ok(()) => Ok(file),
            Err(TryLockError::WouldBlock) => Err(invalid(
                "an activation server is already bound to this root",
            )),
            Err(TryLockError::Error(error)) => Err(error.into()),
        }
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
        Ok(environment_directory("LOCALAPPDATA")?.join(crate::product::DATA_DIR_NAME))
    }
    #[cfg(not(windows))]
    {
        let base = match std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
            Some(value) => absolute_environment_directory("XDG_DATA_HOME", value)?,
            None => environment_directory("HOME")?.join(".local/share"),
        };
        Ok(base.join(crate::product::DATA_DIR_NAME))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DataMode {
    Standard,
    Portable,
}

#[derive(Debug, Clone)]
pub(crate) struct DataLocation {
    pub mode: DataMode,
    pub path: PathBuf,
}

impl DataLocation {
    /// An invalid portable marker is an error, never a standard-directory fallback.
    pub(crate) fn resolve(executable: &Path, explicit: Option<&Path>) -> LanpathResult<Self> {
        let executable = std::path::absolute(executable)?;
        let directory = executable
            .parent()
            .ok_or_else(|| invalid("executable has no parent"))?;
        check_ancestors_for_links(directory)?;
        let marker = directory.join(crate::product::PORTABLE_MARKER);
        let mode = if metadata_if_present(&marker)?.is_some() {
            if check_regular_file(&marker)?.len() != 0 {
                return Err(invalid(
                    "the GPUI Portable marker must be an empty ordinary file",
                ));
            }
            DataMode::Portable
        } else {
            DataMode::Standard
        };
        let path = match explicit {
            Some(path) => {
                if path.as_os_str().is_empty()
                    || path.components().any(|part| part == Component::ParentDir)
                {
                    return Err(invalid(
                        "explicit GPUI data path must be nonempty and contain no '..'",
                    ));
                }
                std::path::absolute(path)?
            }
            None if mode == DataMode::Portable => directory.join("data"),
            None => default_data_dir()?,
        };
        // Admission remains DataRoot::open's responsibility; resolution never
        // retries a legacy or production directory after any failure.
        Ok(Self { mode, path })
    }
}

pub(crate) fn validate_owned_root(path: &Path) -> LanpathResult<PathBuf> {
    check_ancestors(path)?;
    check_owner(path)?;
    check_tree(path)?;
    Ok(path.canonicalize()?)
}

pub(crate) fn validate_tree(path: &Path) -> LanpathResult<()> {
    check_ancestors_for_links(path)?;
    check_tree(path)
}

fn check_ancestors_for_links(path: &Path) -> LanpathResult<()> {
    for ancestor in path.ancestors() {
        if let Some(metadata) = metadata_if_present(ancestor)?
            && (is_link(&metadata) || !metadata.is_dir())
        {
            return Err(invalid("source path contains a link or non-directory"));
        }
    }
    Ok(())
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
