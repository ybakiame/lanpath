//! 启动时选择数据目录；原生业务、NSH 偏好和缓存使用同一隔离根。
use super::{check_regular_file, invalid, metadata_if_present, open_root};
use lanpath_common::LanpathResult;
use std::{
    path::{Component, Path, PathBuf},
    sync::OnceLock,
};

pub(super) const PORTABLE_MARKER: &str = "lanpath.portable";
const DATA_DIR_ENV: &str = "LANPATH_DESKTOP_DATA_DIR";
const DATA_DIR_STORAGE_KEY: &str = "lanpath_desktop_nsh.data_dir";
const DATA_DIR_NAME: &str = "lanpath-desktop-nsh";
static NATIVE_ROOT: OnceLock<PathBuf> = OnceLock::new();

#[derive(Debug, Clone)]
pub(crate) struct DataLocation {
    pub(crate) path: PathBuf,
}

impl DataLocation {
    pub(super) fn resolve(executable: &Path, explicit: Option<&Path>) -> LanpathResult<Self> {
        let path = match explicit {
            Some(path) => {
                if path.as_os_str().is_empty()
                    || path.components().any(|part| part == Component::ParentDir)
                {
                    return Err(invalid(
                        "显式 NSH 桌面数据路径必须非空且不含 '..'（LANPATH_DESKTOP_DATA_DIR）",
                    ));
                }
                std::path::absolute(path)?
            }
            None => {
                let executable = std::path::absolute(executable)?;
                let directory = executable
                    .parent()
                    .ok_or_else(|| invalid("无法确定程序所在目录"))?;
                if portable_marker(directory)? {
                    directory.join("data")
                } else {
                    default_data_dir()?
                }
            }
        };
        Ok(Self { path })
    }
}

fn portable_marker(directory: &Path) -> LanpathResult<bool> {
    let marker = directory.join(PORTABLE_MARKER);
    if metadata_if_present(&marker)?.is_none() {
        return Ok(false);
    }
    if check_regular_file(&marker)?.len() != 0 {
        return Err(invalid("lanpath.portable 必须是空的普通文件"));
    }
    Ok(true)
}

/// 只为当前进程所在目录的标记放行，外来便携目录仍属于受保护祖先。
pub(super) fn is_own_portable_directory(directory: &Path) -> LanpathResult<bool> {
    let executable = std::env::current_exe()?;
    let Some(parent) = executable.parent() else {
        return Ok(false);
    };
    if parent.canonicalize()? != directory.canonicalize()? {
        return Ok(false);
    }
    portable_marker(directory)
}

/// 原生启动在 NSH storage 初始化前准入；本次进程固定该根，不再读取旧 storage 路径。
pub(crate) fn prepare_native_data_dir() -> LanpathResult<PathBuf> {
    if let Some(path) = NATIVE_ROOT.get() {
        return Ok(path.clone());
    }
    let explicit = std::env::var_os(DATA_DIR_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let location = DataLocation::resolve(&std::env::current_exe()?, explicit.as_deref())?;
    let root = open_root(&location.path)?;
    let path = root.path().to_path_buf();
    NATIVE_ROOT
        .set(path.clone())
        .map_err(|_| invalid("原生数据目录已初始化"))?;
    Ok(path)
}

/// Headless/嵌入场景保留 storage 键注入；原生场景总是使用启动快照。
pub(crate) fn resolve_data_location() -> LanpathResult<DataLocation> {
    if let Some(path) = NATIVE_ROOT.get() {
        return Ok(DataLocation { path: path.clone() });
    }
    let explicit = std::env::var_os(DATA_DIR_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            nsh_core::storage::Storage::get_item(DATA_DIR_STORAGE_KEY)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        });
    DataLocation::resolve(&std::env::current_exe()?, explicit.as_deref())
}

pub(super) fn default_data_dir() -> LanpathResult<PathBuf> {
    #[cfg(windows)]
    {
        Ok(environment_directory("LOCALAPPDATA")?.join(DATA_DIR_NAME))
    }
    #[cfg(not(windows))]
    {
        let base = match std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
            Some(value) => absolute_environment_directory("XDG_DATA_HOME", value)?,
            None => environment_directory("HOME")?.join(".local/share"),
        };
        Ok(base.join(DATA_DIR_NAME))
    }
}

fn environment_directory(name: &str) -> LanpathResult<PathBuf> {
    let value = std::env::var_os(name)
        .ok_or_else(|| invalid(format!("{name} 是持久化 NSH 桌面数据目录的必需环境变量")))?;
    absolute_environment_directory(name, value)
}

pub(super) fn absolute_environment_directory(
    name: &str,
    value: std::ffi::OsString,
) -> LanpathResult<PathBuf> {
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(invalid(format!("{name} 必须是绝对路径目录")));
    }
    Ok(path)
}

#[cfg(test)]
#[path = "data_location_tests.rs"]
mod tests;
