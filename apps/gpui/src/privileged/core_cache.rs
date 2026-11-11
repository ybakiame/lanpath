use super::{protocol::failure, security};
use lanpath_common::LanpathResult;
use std::{
    fs::{File, OpenOptions},
    io,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
use windows_sys::Win32::{Foundation::ERROR_FILE_NOT_FOUND, Storage::FileSystem::*};

const MAX_CACHE: u64 = 512 * 1024 * 1024;
fn location(machine: &Path, owner_sid: &str, root_id: uuid::Uuid) -> LanpathResult<PathBuf> {
    // Both components come from the authenticated OS token/persistent root identity,
    // never a caller-provided elevated filesystem path.
    if root_id.is_nil()
        || !owner_sid.starts_with("S-1-")
        || !owner_sid
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'S' | b'-'))
    {
        return Err(failure("invalid managed cache owner identity"));
    }
    let mut path = machine.to_path_buf();
    for component in ["owners", owner_sid, &root_id.to_string(), "cache"] {
        path.push(component);
        security::ensure_private_directory(&path)?;
    }
    Ok(path.join("cache.db"))
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn copy(source: &Path, target: &Path) -> LanpathResult<()> {
    let mut input = match OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(source)
    {
        Ok(file) => file,
        Err(error) if error.raw_os_error() == Some(ERROR_FILE_NOT_FOUND as i32) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    validate(&input)?;
    let expected = input.metadata()?.len();
    match std::fs::symlink_metadata(target) {
        Ok(metadata)
            if !metadata.is_file()
                || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 =>
        {
            return Err(failure("managed cache target is not an ordinary file"));
        }
        Ok(_) => {
            validate(&File::open(target)?)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let parent = target
        .parent()
        .ok_or_else(|| failure("managed cache has no parent"))?;
    let path = parent.join(format!("cache-{}.tmp", uuid::Uuid::new_v4()));
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .open(&path)?;
    let temporary = Temporary(path);
    if io::copy(&mut input, &mut output)? != expected {
        return Err(failure(
            "managed cache changed during its stopped-core snapshot",
        ));
    }
    output.sync_all()?;
    drop(output);
    if unsafe {
        MoveFileExW(
            security::wide(&temporary.0).as_ptr(),
            security::wide(target).as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}
fn validate(file: &File) -> LanpathResult<()> {
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || metadata.len() > MAX_CACHE
        || crate::windows_file::link_count(file)? != 1
    {
        return Err(failure(
            "managed cache must be a bounded, unshared ordinary file",
        ));
    }
    Ok(())
}
pub(super) fn load(
    machine: &Path,
    owner_sid: &str,
    root_id: uuid::Uuid,
    session: &Path,
) -> LanpathResult<()> {
    copy(
        &location(machine, owner_sid, root_id)?,
        &session.join("cache/cache.db"),
    )
}
/// Called only after the original core has exited. Session-local writes stay inside
/// the existing full configuration policy; only this fixed cache file is committed.
pub(super) fn save(
    machine: &Path,
    owner_sid: &str,
    root_id: uuid::Uuid,
    session: &Path,
) -> LanpathResult<()> {
    copy(
        &session.join("cache/cache.db"),
        &location(machine, owner_sid, root_id)?,
    )
}
