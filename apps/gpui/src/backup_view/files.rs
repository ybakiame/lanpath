use std::{
    fs::{self, Metadata, OpenOptions},
    io::{Read as _, Write as _},
    path::{Component, Path, PathBuf},
};

use lanpath_client::backup::MAX_ARCHIVE;
use lanpath_common::{LanpathError, LanpathResult};

fn invalid(message: &str) -> LanpathError {
    LanpathError::Config(message.into())
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

/// Match the existing resource confinement policy: no symlinks, junctions or
/// reparse points in any path component. Concurrent hostile parent replacement
/// is outside that policy, as it is for the existing configuration stores.
fn selected_path(path: &Path) -> LanpathResult<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(invalid("请选择完整的本地备份文件路径"));
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid("备份文件名必须是有效文本"))?;
    if !name.to_ascii_lowercase().ends_with(".zip.age")
        || name
            .chars()
            .any(|character| character.is_control() || "<>:\"|?*".contains(character))
    {
        return Err(invalid("请选择以 .zip.age 结尾的备份文件"));
    }
    let parent = path.parent().ok_or_else(|| invalid("备份目录不存在"))?;
    for ancestor in parent.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        if is_link(&metadata) || !metadata.is_dir() {
            return Err(invalid("备份路径不能包含链接、联接点或非目录父路径"));
        }
    }
    Ok(fs::canonicalize(parent)?.join(name))
}

fn ordinary(metadata: &Metadata) -> LanpathResult<()> {
    if is_link(metadata) || !metadata.is_file() {
        return Err(invalid("备份必须是普通文件，不能是目录、链接或设备"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if metadata.nlink() != 1 {
            return Err(invalid("不支持硬链接备份文件"));
        }
    }
    if metadata.len() > MAX_ARCHIVE as u64 {
        return Err(invalid("加密备份超过 16 MiB 限制"));
    }
    Ok(())
}

pub(super) fn read_encrypted(path: &Path) -> LanpathResult<Vec<u8>> {
    let path = selected_path(path)?;
    let pathname = fs::symlink_metadata(&path)?;
    ordinary(&pathname)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options.custom_flags(0x0020_0000).share_mode(0x1 | 0x2);
    }
    let file = options.open(&path)?;
    let opened = file.metadata()?;
    ordinary(&opened)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if pathname.dev() != opened.dev() || pathname.ino() != opened.ino() {
            return Err(invalid("备份文件在打开时发生变化，请重新选择"));
        }
    }
    #[cfg(windows)]
    if crate::windows_file::link_count(&file)? != 1 {
        return Err(invalid("不支持硬链接备份文件"));
    }
    #[cfg(not(unix))]
    let _ = pathname;
    let mut encrypted = Vec::with_capacity(opened.len() as usize);
    file.take((MAX_ARCHIVE + 1) as u64)
        .read_to_end(&mut encrypted)?;
    if encrypted.len() > MAX_ARCHIVE {
        return Err(invalid("加密备份超过 16 MiB 限制"));
    }
    Ok(encrypted)
}

pub(super) fn export_destination(path: &Path, root: &Path) -> LanpathResult<PathBuf> {
    let path = selected_path(path)?;
    let root = fs::canonicalize(root)?;
    if path.starts_with(&root) {
        return Err(invalid(
            "不能导出到实验应用数据目录或其子目录；请选择独立备份目录",
        ));
    }
    match fs::symlink_metadata(&path) {
        Ok(_) => Err(invalid("目标文件已存在；为避免覆盖，请选择新的文件名")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn save_encrypted(path: &Path, root: &Path, encrypted: &[u8]) -> LanpathResult<()> {
    let path = export_destination(path, root)?;
    if encrypted.len() > MAX_ARCHIVE {
        return Err(invalid("加密备份超过 16 MiB 限制"));
    }
    let parent = path.parent().ok_or_else(|| invalid("备份目录不存在"))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(encrypted)?;
    file.as_file().sync_all()?;
    // No plaintext ever reaches a file. Atomic no-clobber also protects an
    // existing destination created after the native save picker returned.
    file.persist_noclobber(&path).map_err(|error| {
        if error.error.kind() == std::io::ErrorKind::AlreadyExists {
            invalid("目标文件已存在；为避免覆盖，请选择新的文件名")
        } else {
            error.error.into()
        }
    })?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
