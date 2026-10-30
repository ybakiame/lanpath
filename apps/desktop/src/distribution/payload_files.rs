use super::{invalid, signature};
use lanpath_common::LanpathResult;
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub(super) type Files = BTreeMap<String, File>;

pub(super) fn paths() -> impl Iterator<Item = &'static str> {
    crate::product::TRUSTED_BUNDLE_FILES
        .iter()
        .copied()
        .chain(["bundle.json", "bundle.json.sig"])
}

// Only inspect the selected paths and their ancestors; unrelated user files are
// neither enumerated nor interpreted as application state.
pub(super) fn check_path(path: &Path) -> LanpathResult<()> {
    for ancestor in path.ancestors() {
        let meta = match std::fs::symlink_metadata(ancestor) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        #[cfg(windows)]
        let linked = {
            use std::os::windows::fs::MetadataExt;
            meta.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let linked = meta.file_type().is_symlink();
        if linked || (ancestor != path && !meta.is_dir()) || (!meta.is_dir() && !meta.is_file()) {
            return Err(invalid(
                "payload path contains a link, reparse point or special file",
            ));
        }
    }
    Ok(())
}

pub(super) fn options(target: bool) -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(target);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Sources cannot be changed/deleted. Destination handles deny read and
        // write admission (including image loading), but permit our atomic rename.
        // Opening a mapped executable for write fails before any payload mutation.
        options
            .share_mode(if target { 0x4 } else { 0x1 })
            .custom_flags(0x0020_0000);
    }
    options
}

pub(super) fn open_files(root: &Path, target: bool) -> LanpathResult<Files> {
    let mut files = Files::new();
    for name in paths() {
        let path = root.join(name);
        check_path(&path)?;
        match options(target).open(&path) {
            Ok(file) if file.metadata()?.is_file() => {
                files.insert(name.into(), file);
            }
            Ok(_) => return Err(invalid("payload contains a nonregular file")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(invalid(format!(
                    "cannot admit selected payload {}; close this application and retry: {error}",
                    path.display()
                )));
            }
        }
    }
    Ok(files)
}

pub(super) fn stamp(file: &File) -> LanpathResult<(u64, String)> {
    let mut input = file;
    input.seek(SeekFrom::Start(0))?;
    signature::digest_reader(&mut input)
}

pub(super) fn bytes(file: &File, limit: u64) -> LanpathResult<Vec<u8>> {
    if file.metadata()?.len() > limit {
        return Err(invalid("payload metadata exceeds limit"));
    }
    let mut input = file;
    input.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    input.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid("payload metadata exceeds limit"));
    }
    Ok(bytes)
}

pub(super) fn snapshot(files: &Files, backup: &Path) -> LanpathResult<()> {
    for (name, input) in files {
        let path = backup.join(name);
        check_path(&path)?;
        std::fs::create_dir_all(
            path.parent()
                .ok_or_else(|| invalid("payload has no parent"))?,
        )?;
        let mut output = options(true).create_new(true).open(&path)?;
        let mut input = input;
        input.seek(SeekFrom::Start(0))?;
        std::io::copy(&mut input, &mut output)?;
        output.sync_all()?;
    }
    sync_directory(backup)
}

pub(super) fn replace(root: &Path, name: &str, input: &File) -> LanpathResult<File> {
    let path = root.join(name);
    check_path(&path)?;
    let parent = path
        .parent()
        .ok_or_else(|| invalid("payload has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    let (file, temporary_path) = temporary.into_parts();
    drop(file);
    let mut output = options(true).open(&temporary_path)?;
    let mut input = input;
    input.seek(SeekFrom::Start(0))?;
    std::io::copy(&mut input, &mut output)?;
    output.flush()?;
    output.sync_all()?;
    std::fs::rename(&temporary_path, &path)?;
    sync_directory(parent)?;
    // Retain the replacement handle as well as the displaced one: no new
    // instance can observe a partly replaced bundle between individual renames.
    Ok(output)
}

pub(super) fn sync_directory(path: &Path) -> LanpathResult<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub(super) fn clean_backup(backup: &Path) -> LanpathResult<()> {
    let mut directories = std::collections::BTreeSet::<PathBuf>::new();
    for name in paths() {
        let path = backup.join(name);
        check_path(&path)?;
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let mut parent = path.parent();
        while let Some(path) = parent {
            if path == backup {
                break;
            }
            directories.insert(path.to_path_buf());
            parent = path.parent();
        }
    }
    for directory in directories.into_iter().rev().chain([backup.to_path_buf()]) {
        match std::fs::remove_dir(directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
