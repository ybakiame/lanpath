//! Filesystem boundaries shared by inventory installation and experimental selection.

use std::path::{Component, Path};

use lanpath_common::{LanpathError, LanpathResult};

pub(crate) fn invalid(message: impl Into<String>) -> LanpathError {
    LanpathError::Core(message.into())
}

pub(crate) fn normalized_version(value: &str) -> LanpathResult<&str> {
    let value = value.strip_prefix('v').unwrap_or(value);
    let (base, prerelease) = value
        .split_once('-')
        .map_or((value, None), |(a, b)| (a, Some(b)));
    let mut numbers = base.split('.');
    let valid_numbers = (0..3).all(|_| {
        numbers
            .next()
            .is_some_and(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    }) && numbers.next().is_none();
    let valid_prerelease = prerelease.is_none_or(|suffix| {
        !suffix.is_empty()
            && suffix.split('.').all(|part| {
                !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
    });
    if value.len() > 128 || !valid_numbers || !valid_prerelease {
        return Err(invalid(format!("Invalid core version: {value}")));
    }
    Ok(value)
}

pub(crate) fn asset_name(name: &str) -> LanpathResult<()> {
    if name.is_empty()
        || name.len() > 255
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
        || name == "."
        || name == ".."
    {
        return Err(invalid("Invalid release asset name"));
    }
    Ok(())
}

/// Reject lexical traversal and links, including existing ancestors of a new path.
/// This is a same-user filesystem boundary, not a defense against concurrent replacement.
pub(crate) fn unlinked_path(path: &Path) -> LanpathResult<()> {
    let mut current = std::path::PathBuf::new();
    for part in path.components() {
        if matches!(part, Component::ParentDir | Component::CurDir) {
            return Err(invalid("Core path cannot contain traversal components"));
        }
        current.push(part);
        // A Windows drive/UNC prefix is namespace syntax, not a filesystem
        // node. Inspect the root after RootDir is appended, then each child.
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        let metadata = match std::fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() {
            return Err(invalid("Core path cannot contain symbolic links"));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(invalid("Core path cannot contain reparse points"));
            }
        }
        #[cfg(windows)]
        if metadata.is_file() {
            use std::os::windows::io::AsRawHandle as _;
            use windows_sys::Win32::Storage::FileSystem::{
                BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
            };
            let file = std::fs::File::open(&current)?;
            let mut information = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
            // SAFETY: the live owned file handle and correctly sized output
            // storage remain valid for the synchronous Win32 call.
            if unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) }
                == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            // SAFETY: success initialized every field in the structure.
            if unsafe { information.assume_init() }.nNumberOfLinks > 1 {
                return Err(invalid("Core binary cannot be hard-linked"));
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.is_file() && metadata.nlink() > 1 {
                return Err(invalid("Core binary cannot be hard-linked"));
            }
        }
    }
    Ok(())
}

pub(crate) fn executable(path: &Path) -> LanpathResult<()> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(invalid("Core binary is not a regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(invalid("Core binary is not executable"));
        }
    }
    Ok(())
}
