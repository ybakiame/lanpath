use super::{files, invalid};
use lanpath_common::LanpathResult;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum DataMode {
    Standard,
    Portable,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ObjectIdentity {
    device: u64,
    file: u64,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    root: ObjectIdentity,
    marker: Option<ObjectIdentity>,
    data: Option<ObjectIdentity>,
}

pub(super) struct Root {
    pub(super) path: PathBuf,
    pub(super) mode: DataMode,
    pub(super) identity: Identity,
    _root: File,
    _portable: Option<(File, File)>,
}

fn directory(path: &Path) -> LanpathResult<File> {
    files::check_path(path)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Retain directory identity: no rename/delete while selected admission is held.
        options.share_mode(3).custom_flags(0x0220_0000);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_dir() {
        return Err(invalid("payload root/data must be directories"));
    }
    Ok(file)
}

fn identity(file: &File) -> LanpathResult<ObjectIdentity> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
        // SAFETY: the retained handle is valid and the output has the documented layout.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: successful API call initialized the structure.
        let info = unsafe { info.assume_init() };
        Ok(ObjectIdentity {
            device: u64::from(info.dwVolumeSerialNumber),
            file: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        })
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = file.metadata()?;
        Ok(ObjectIdentity {
            device: meta.dev(),
            file: meta.ino(),
        })
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = file;
        Err(invalid("native filesystem identity unavailable"))
    }
}

pub(super) fn admit(path: &Path) -> LanpathResult<Root> {
    // Updating never creates a payload root or initializes either data location.
    let root = directory(path)?;
    let path = path.canonicalize()?;
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        if !matches!(path.components().next(), Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
        {
            return Err(invalid(
                "payload effects require a local Windows drive root; UNC/WSL-hosted filesystems cannot prove native resource ownership",
            ));
        }
    }
    super::refuse_legacy_state(&path)?;
    let marker_path = path.join(crate::product::PORTABLE_MARKER);
    files::check_path(&marker_path)?;
    let (mode, marker, data, portable_handles) = match files::options(false).open(&marker_path) {
        Ok(marker) => {
            let metadata = marker.metadata()?;
            if !metadata.is_file() || metadata.len() != 0 {
                return Err(invalid(
                    "Portable update requires its existing empty marker",
                ));
            }
            let data = directory(&path.join("data"))?;
            (
                DataMode::Portable,
                Some(identity(&marker)?),
                Some(identity(&data)?),
                Some((marker, data)),
            )
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            (DataMode::Standard, None, None, None)
        }
        Err(error) => return Err(error.into()),
    };
    let identity = Identity {
        root: identity(&root)?,
        marker,
        data,
    };
    Ok(Root {
        path,
        mode,
        identity,
        _root: root,
        _portable: portable_handles,
    })
}

pub(super) fn validate(root: &Root) -> LanpathResult<()> {
    let current = directory(&root.path)?;
    if identity(&current)? != root.identity.root {
        return Err(invalid(
            "payload root identity changed; transaction preserved",
        ));
    }
    super::refuse_legacy_state(&root.path)?;
    let marker_path = root.path.join(crate::product::PORTABLE_MARKER);
    files::check_path(&marker_path)?;
    match files::options(false).open(&marker_path) {
        Ok(marker) => {
            let metadata = marker.metadata()?;
            if root.mode != DataMode::Portable
                || !metadata.is_file()
                || metadata.len() != 0
                || Some(identity(&marker)?) != root.identity.marker
            {
                return Err(invalid(
                    "payload data mode or marker identity changed; transaction preserved",
                ));
            }
            let data = directory(&root.path.join("data"))?;
            if Some(identity(&data)?) != root.identity.data {
                return Err(invalid(
                    "Portable data identity changed; transaction preserved",
                ));
            }
        }
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound && root.mode == DataMode::Standard => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
