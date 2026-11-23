//! 启动时选择数据目录；业务配置、界面偏好和缓存使用同一隔离根。
//!
//! 优先级：`LANPATH_DATA_DIR` 显式覆盖，其次为程序同级存在且可写的 `data/`
//! （便携模式），否则回退到 `etcetera` 平台原生标准数据目录（Windows
//! `%APPDATA%\Lanpath`、macOS `~/Library/Application Support/Lanpath`、Linux
//! `$XDG_DATA_HOME/lanpath`）。
use super::{invalid, is_link, metadata_if_present, open_root};
use etcetera::base_strategy::{BaseStrategy, choose_native_strategy};
use lanpath_common::LanpathResult;
use std::{
    fs::{self, OpenOptions},
    io::ErrorKind,
    path::{Component, Path, PathBuf},
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) const PORTABLE_DIR_NAME: &str = "data";
const DATA_DIR_ENV: &str = "LANPATH_DATA_DIR";
const DATA_DIR_STORAGE_KEY: &str = "lanpath.data_dir";
#[cfg(any(target_os = "windows", target_os = "macos"))]
const APP_DIR_NAME: &str = "Lanpath";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const APP_DIR_NAME: &str = "lanpath";
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
                        "显式 Lanpath 数据路径必须非空且不含 '..'（LANPATH_DATA_DIR）",
                    ));
                }
                std::path::absolute(path)?
            }
            None => {
                let executable = std::path::absolute(executable)?;
                let directory = executable
                    .parent()
                    .ok_or_else(|| invalid("无法确定程序所在目录"))?;
                if let Some(portable) = portable_data_dir(directory)? {
                    portable
                } else {
                    default_data_dir()?
                }
            }
        };
        Ok(Self { path })
    }
}

/// 当程序同级存在普通的 `data/` 目录且当前进程具备写入权限时，启用便携模式；
/// 不存在、不是普通目录或不可写时回退到平台标准数据目录。
fn portable_data_dir(directory: &Path) -> LanpathResult<Option<PathBuf>> {
    let candidate = directory.join(PORTABLE_DIR_NAME);
    let Some(metadata) = metadata_if_present(&candidate)? else {
        return Ok(None);
    };
    if is_link(&metadata) || !metadata.is_dir() {
        return Ok(None);
    }
    if !is_writable_directory(&candidate) {
        return Ok(None);
    }
    Ok(Some(candidate))
}

fn is_writable_directory(directory: &Path) -> bool {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let probe = directory.join(format!(
        ".lanpath-write-probe-{}-{nonce}",
        std::process::id()
    ));
    match OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(file) => {
            drop(file);
            let _ = fs::remove_file(&probe);
            true
        }
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem
            ) =>
        {
            false
        }
        Err(_) => false,
    }
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
    let strategy = choose_native_strategy()
        .map_err(|error| invalid(format!("无法定位平台标准数据目录: {error}")))?;
    let dir = strategy.data_dir().join(APP_DIR_NAME);
    if !dir.is_absolute() {
        return Err(invalid("平台标准数据目录必须是绝对路径"));
    }
    Ok(dir)
}

#[cfg(test)]
#[path = "data_location_tests.rs"]
mod tests;
