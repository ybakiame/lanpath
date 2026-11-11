use std::fs::File;
use std::os::windows::io::AsRawHandle;
use std::{
    collections::BTreeMap,
    fs::{Metadata, OpenOptions},
    io,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
};
use windows_sys::Win32::{
    Storage::FileSystem::GetVolumeInformationByHandleW,
    System::SystemServices::FILE_PERSISTENT_ACLS,
};

pub(crate) fn link_count(file: &File) -> std::io::Result<u32> {
    // BY_HANDLE_FILE_INFORMATION consists exclusively of DWORDs; each FILETIME
    // is two DWORDs. Keep its exact documented C layout.
    #[repr(C)]
    struct Information {
        attributes: u32,
        creation: [u32; 2],
        access: [u32; 2],
        write: [u32; 2],
        volume: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(
            file: *mut std::ffi::c_void,
            information: *mut Information,
        ) -> i32;
    }
    let mut information = std::mem::MaybeUninit::<Information>::uninit();
    // SAFETY: file owns a live handle, and the output is correctly sized,
    // aligned, writable storage for the Win32 structure.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: a successful call initialized every structure field.
    Ok(unsafe { information.assume_init() }.links)
}

/// Keep every directory component non-renamable, not just the final file.
#[derive(Default)]
pub(crate) struct DirectoryLocks {
    handles: BTreeMap<PathBuf, File>,
}
impl DirectoryLocks {
    pub(crate) fn acquire(path: &Path) -> io::Result<Self> {
        let mut locks = Self::default();
        locks.extend(path)?;
        Ok(locks)
    }
    pub(crate) fn supports_private_acls(&self, path: &Path) -> io::Result<bool> {
        let file = self
            .handles
            .get(path)
            .ok_or_else(|| io::Error::other("volume capability query requires a held directory"))?;
        let mut flags = 0;
        if unsafe {
            GetVolumeInformationByHandleW(
                file.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut flags,
                std::ptr::null_mut(),
                0,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(flags & FILE_PERSISTENT_ACLS != 0)
    }
    pub(crate) fn extend(&mut self, path: &Path) -> io::Result<()> {
        if path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "directory leases reject parent components",
            ));
        }
        let absolute = std::path::absolute(path)?;
        let mut parts = absolute.components();
        let prefix = parts
            .next()
            .ok_or_else(|| io::Error::other("directory lease requires an absolute Windows path"))?;
        if !matches!(prefix, Component::Prefix(_)) || parts.next() != Some(Component::RootDir) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "directory lease requires a rooted Windows namespace",
            ));
        }
        let mut current = PathBuf::from(prefix.as_os_str());
        current.push(Component::RootDir.as_os_str());
        self.hold(&current)?;
        for part in parts {
            if !matches!(part, Component::Normal(_)) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "directory lease rejects non-normal components",
                ));
            }
            current.push(part.as_os_str());
            self.hold(&current)?;
        }
        Ok(())
    }
    fn hold(&mut self, path: &Path) -> io::Result<()> {
        if self.handles.contains_key(path) {
            return Ok(());
        }
        let file = OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        let metadata: Metadata = file.metadata()?;
        if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "directory lease encountered a non-directory or reparse point",
            ));
        }
        self.handles.insert(path.to_path_buf(), file);
        Ok(())
    }
}
