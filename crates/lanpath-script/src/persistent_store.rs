use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use lanpath_common::{LanpathError, LanpathResult};
use serde::{Deserialize, Serialize};

use crate::host::PersistentStore;

pub(crate) const SHARED_SCOPE: &str = "shared";
pub(crate) const DEFAULT_KEY: &str = "__default__";
const STATE_FILE: &str = "store-v1.json";
const LOCK_FILE: &str = "store.lock";
const VERSION: u32 = 1;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Default, Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    scopes: BTreeMap<String, BTreeMap<String, String>>,
    migrated_legacy: BTreeSet<String>,
}

#[derive(Deserialize)]
struct LegacyScopeFile {
    scope: String,
    entries: BTreeMap<String, String>,
}

/// A live view of one data root, not a cached snapshot. Every operation holds an
/// OS file lock across read/migrate/modify/atomic replace, including other processes.
/// Legacy files remain untouched for recovery; conflicting values abort migration.
/// All writers to this directory must use this protocol (legacy binaries must stop
/// before migration, since they do not participate in the lock).
pub struct FilePersistentStore {
    dir: PathBuf,
}

impl FilePersistentStore {
    /// Opening is lazy so callers receive real I/O/migration errors from operations.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn transaction<T>(
        &self,
        write: bool,
        operation: impl FnOnce(&mut StoreFile) -> LanpathResult<T>,
    ) -> LanpathResult<T> {
        std::fs::create_dir_all(&self.dir).map_err(|e| io_error("create directory", e))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.dir.join(LOCK_FILE))
            .map_err(|e| io_error("open lock", e))?;
        fs2::FileExt::lock_exclusive(&lock).map_err(|e| io_error("lock", e))?;
        // Dropping this handle releases the OS lock even on early error returns.
        let mut state = self.load()?;
        let migrated = self.migrate(&mut state)?;
        let result = operation(&mut state)?;
        if write || migrated {
            self.persist(&state)?;
        }
        drop(lock);
        Ok(result)
    }

    fn load(&self) -> LanpathResult<StoreFile> {
        let bytes = match std::fs::read(self.dir.join(STATE_FILE)) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(StoreFile {
                    version: VERSION,
                    ..StoreFile::default()
                });
            }
            Err(e) => return Err(io_error("read state", e)),
        };
        let state: StoreFile = serde_json::from_slice(&bytes)
            .map_err(|e| store_error(format!("invalid versioned state: {e}")))?;
        if state.version != VERSION {
            return Err(store_error(format!(
                "unsupported store version {}",
                state.version
            )));
        }
        Ok(state)
    }

    fn migrate(&self, state: &mut StoreFile) -> LanpathResult<bool> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(&self.dir).map_err(|e| io_error("list legacy files", e))? {
            let entry = entry.map_err(|e| io_error("read legacy entry", e))?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json")
                && entry.file_name() != STATE_FILE
            {
                files.push(path);
            }
        }
        files.sort();
        let mut migrated = false;
        for path in files {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| store_error("legacy filename is not UTF-8"))?;
            if state.migrated_legacy.contains(name) {
                continue;
            }
            if !std::fs::symlink_metadata(&path)
                .map_err(|e| io_error("inspect legacy file", e))?
                .file_type()
                .is_file()
            {
                return Err(store_error(format!(
                    "legacy entry {name} is not a regular file"
                )));
            }
            let bytes = std::fs::read(&path).map_err(|e| io_error("read legacy file", e))?;
            let legacy: LegacyScopeFile = serde_json::from_slice(&bytes)
                .map_err(|e| store_error(format!("invalid legacy file {name}: {e}")))?;
            for (key, value) in legacy.entries {
                let scope = if let Some(identity) = legacy.scope.strip_prefix("prefs:") {
                    if key == DEFAULT_KEY {
                        format!("script:{identity}")
                    } else {
                        SHARED_SCOPE.to_owned()
                    }
                } else if legacy.scope.starts_with("pstore:") {
                    // The old Surge adapter required explicit keys, including __default__.
                    SHARED_SCOPE.to_owned()
                } else {
                    legacy.scope.clone()
                };
                let entries = state.scopes.entry(scope).or_default();
                if let Some(existing) = entries.get(&key) {
                    if existing != &value {
                        return Err(store_error(format!(
                            "legacy migration conflict in {name} for key {key:?}; original files preserved; resolve conflicting legacy entries before retrying"
                        )));
                    }
                } else {
                    entries.insert(key, value);
                }
            }
            state.migrated_legacy.insert(name.to_owned());
            migrated = true;
        }
        Ok(migrated)
    }

    fn persist(&self, state: &StoreFile) -> LanpathResult<()> {
        let bytes =
            serde_json::to_vec(state).map_err(|e| store_error(format!("serialize state: {e}")))?;
        let (path, mut file) = self.create_temp()?;
        let result = (|| {
            file.write_all(&bytes)
                .map_err(|e| io_error("write state", e))?;
            file.sync_all().map_err(|e| io_error("sync state", e))?;
            drop(file);
            // std::fs::rename replaces an existing file on Windows too (using
            // MoveFileExW/FileRenameInfoEx), not only on Unix. Keep the temporary
            // file in this directory, and never remove the old state first:
            // replacement failures must leave the last committed state intact.
            // https://doc.rust-lang.org/std/fs/fn.rename.html
            std::fs::rename(&path, self.dir.join(STATE_FILE))
                .map_err(|e| io_error("replace state", e))?;
            #[cfg(unix)]
            File::open(&self.dir)
                .and_then(|dir| dir.sync_all())
                .map_err(|e| {
                    io_error(
                        "sync directory after replacement (state may already be committed)",
                        e,
                    )
                })?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(path);
        }
        result
    }

    fn create_temp(&self) -> LanpathResult<(PathBuf, File)> {
        loop {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = self
                .dir
                .join(format!("store-{}-{sequence}.tmp", std::process::id()));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(file) => return Ok((path, file)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(io_error("create temporary state", e)),
            }
        }
    }
}

impl PersistentStore for FilePersistentStore {
    fn read(&self, scope: &str, key: &str) -> LanpathResult<Option<String>> {
        self.transaction(false, |state| {
            Ok(state
                .scopes
                .get(scope)
                .and_then(|entries| entries.get(key))
                .cloned())
        })
    }

    fn write(&self, scope: &str, key: &str, value: &str) -> LanpathResult<()> {
        self.transaction(true, |state| {
            state
                .scopes
                .entry(scope.to_owned())
                .or_default()
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        })
    }

    fn erase(&self, scope: &str, key: &str) -> LanpathResult<bool> {
        self.transaction(true, |state| {
            Ok(state
                .scopes
                .get_mut(scope)
                .and_then(|entries| entries.remove(key))
                .is_some())
        })
    }

    fn clear(&self, scope: &str) -> LanpathResult<()> {
        self.transaction(true, |state| {
            state.scopes.remove(scope);
            Ok(())
        })
    }

    fn clear_all(&self) -> LanpathResult<()> {
        self.transaction(true, |state| {
            // Keep migration receipts: the retained recovery files must not
            // resurrect values after an explicit reset.
            state.scopes.clear();
            Ok(())
        })
    }
}

fn store_error(message: impl Into<String>) -> LanpathError {
    LanpathError::Internal(format!("persistent store: {}", message.into()))
}

fn io_error(operation: &str, error: std::io::Error) -> LanpathError {
    store_error(format!("{operation}: {error}"))
}

#[cfg(test)]
#[path = "persistent_store_tests.rs"]
mod tests;
