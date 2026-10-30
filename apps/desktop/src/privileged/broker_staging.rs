use super::{
    file_operation,
    protocol::failure,
    security,
    staging::{self, TrustedBundle},
    staging_lock::StagingLock,
};
use lanpath_common::LanpathResult;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
};

const PREFIX: &str = "LanpathGPUI-Helper-";
fn cache_name(image: &str, sid: &str) -> String {
    format!(
        "{PREFIX}{}-{:x}",
        image.to_ascii_lowercase(),
        Sha256::digest(sid.as_bytes())
    )
}
fn inventory() -> BTreeSet<PathBuf> {
    crate::product::TRUSTED_BUNDLE_FILES
        .iter()
        .map(PathBuf::from)
        .chain([
            PathBuf::from("bundle.json"),
            PathBuf::from("bundle.json.sig"),
        ])
        .collect()
}
fn incoming(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".lanpath-staging");
    PathBuf::from(name)
}
fn directory(
    path: &Path,
    policy: &security::CodeDirectoryPolicy,
    operation: &file_operation::Operation,
) -> LanpathResult<()> {
    if !path.try_exists()? {
        operation.batch("create-directory.PerformOperations", |operation| {
            operation.queue_create_directory(path)
        })?;
    }
    policy.validate(path)
}

/// Never elevate a user-writable application. Copy only fixed leaf paths whose
/// source files and ancestors remain locked; never enumerate a mutable source tree.
pub(super) fn ensure(trusted: &TrustedBundle, sid: &str) -> LanpathResult<TrustedBundle> {
    file_operation::session(|operation| ensure_in_session(trusted, sid, operation))
}

fn ensure_in_session(
    trusted: &TrustedBundle,
    sid: &str,
    operation: &file_operation::Operation,
) -> LanpathResult<TrustedBundle> {
    let programs = security::code_location()?;
    let name = cache_name(&trusted.image_sha256, sid);
    let _serialization = StagingLock::acquire(&name, sid)?;
    let destination = programs.join(&name);
    let policy = security::CodeDirectoryPolicy::new()?;
    let mut directories = crate::windows_file::DirectoryLocks::acquire(&programs)?;
    policy.validate(&programs)?;
    let (missing, partial) = if destination.try_exists()? {
        directories.extend(&destination)?;
        inspect(&destination, &policy, true)?
    } else {
        (inventory(), Vec::new())
    };
    if !missing.is_empty() || !partial.is_empty() {
        for file in inventory().difference(&missing) {
            trusted.verify_file(file, &destination.join(file))?;
        }
        directory(&destination, &policy, operation)?;
        directories.extend(&destination)?;
        // Only reserved, ordinary, single-link incoming files may be discarded.
        // Changed final payloads and unknown entries were rejected above.
        if !partial.is_empty() {
            operation.batch("discard-incoming.PerformOperations", |operation| {
                operation.queue_delete_files(partial.iter().map(|path| destination.join(path)))
            })?;
        }
        let mut parents = BTreeSet::new();
        for file in &missing {
            for parent in file
                .ancestors()
                .skip(1)
                .filter(|path| !path.as_os_str().is_empty())
            {
                parents.insert(parent.to_path_buf());
            }
        }
        for parent in parents {
            let parent = destination.join(parent);
            directory(&parent, &policy, operation)?;
            directories.extend(&parent)?;
        }
        if !missing.is_empty() {
            operation.batch("copy-files.PerformOperations", |operation| {
                for file in &missing {
                    operation.queue_copy_file(
                        &trusted.root.join(file),
                        &destination.join(incoming(file)),
                    )?;
                }
                Ok(())
            })?;
            // Verify every frozen incoming payload before queuing any promotion.
            for file in &missing {
                let temporary = destination.join(incoming(file));
                policy.validate_file(&temporary)?;
                trusted.verify_file(file, &temporary)?;
            }
            operation.batch("promote-files.PerformOperations", |operation| {
                for file in &missing {
                    operation.queue_rename_file(
                        &destination.join(incoming(file)),
                        &destination.join(file),
                    )?;
                }
                Ok(())
            })?;
        }
    }
    validate_location(&destination, &trusted.image_sha256, sid)?;
    staging::trusted_bundle_at(&destination.join(crate::product::BINARY_NAME))
}

pub(super) fn validate_location(root: &Path, image: &str, sid: &str) -> LanpathResult<()> {
    let programs = security::code_location()?;
    if root != programs.join(cache_name(image, sid)) {
        return Err(failure(
            "helper must execute from its image/user-bound Program Files code cache",
        ));
    }
    let _directories = crate::windows_file::DirectoryLocks::acquire(root)?;
    let policy = security::CodeDirectoryPolicy::new()?;
    policy.validate(&programs)?;
    let (missing, _) = inspect(root, &policy, false)?;
    if !missing.is_empty() {
        return Err(failure("protected helper cache is incomplete"));
    }
    Ok(())
}

fn inspect(
    root: &Path,
    policy: &security::CodeDirectoryPolicy,
    allow_partial: bool,
) -> LanpathResult<(BTreeSet<PathBuf>, Vec<PathBuf>)> {
    let mut expected = inventory();
    let incoming: BTreeSet<_> = expected.iter().map(|path| incoming(path)).collect();
    let mut parents = BTreeSet::new();
    for file in &expected {
        for directory in file
            .ancestors()
            .skip(1)
            .filter(|path| !path.as_os_str().is_empty())
        {
            parents.insert(directory.to_path_buf());
        }
    }
    fn walk(
        root: &Path,
        relative: &Path,
        policy: &security::CodeDirectoryPolicy,
        expected: &mut BTreeSet<PathBuf>,
        parents: &BTreeSet<PathBuf>,
        incoming: &BTreeSet<PathBuf>,
        partial: &mut Vec<PathBuf>,
    ) -> LanpathResult<()> {
        let directory = root.join(relative);
        policy.validate(&directory)?;
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let path = relative.join(entry.file_name());
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_attributes()
                & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                != 0
            {
                return Err(failure("protected helper cache contains a reparse entry"));
            }
            if metadata.is_dir() && parents.contains(&path) {
                walk(root, &path, policy, expected, parents, incoming, partial)?;
            } else if metadata.is_file() {
                policy.validate_file(&entry.path())?;
                staging::validate_file_shape(&entry.path())?;
                if expected.remove(&path) {
                    continue;
                }
                if incoming.contains(&path) {
                    partial.push(path);
                } else {
                    return Err(failure(
                        "protected helper cache contains an unshipped or duplicate entry",
                    ));
                }
            } else {
                return Err(failure(
                    "protected helper cache contains an unshipped directory or non-file",
                ));
            }
        }
        Ok(())
    }
    let mut partial = Vec::new();
    walk(
        root,
        Path::new(""),
        policy,
        &mut expected,
        &parents,
        &incoming,
        &mut partial,
    )?;
    if !allow_partial && !partial.is_empty() {
        return Err(failure(
            "protected helper cache has unfinished incoming payloads",
        ));
    }
    Ok((expected, partial))
}
