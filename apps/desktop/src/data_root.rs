//! NSH 桌面独立数据目录：所有权标记 + 进程内租约 + 生产/链接路径准入。
//!
//! 从 apps/gpui 的 data_root 迁移并适配为 NSH 桌面身份（`.lanpath-desktop-nsh`），
//! 链接、所有权、租约和路径安全约束与 GPUI 版本一致，不削弱。GPUI 的便携模式
//! 依赖可执行文件目录布局，NSH 桌面不提供便携模式：只有默认目录与显式覆盖
//! （`LANPATH_DESKTOP_DATA_DIR` 环境变量或宿主预置的 NSH storage 键）。

use lanpath_common::{LanpathError, LanpathResult};
use std::fs::{self, File, Metadata, OpenOptions, TryLockError};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

#[path = "data_root_identity.rs"]
mod identity;

/// 显式数据根环境变量（与 apps/gpui 的桌面迁移目标一致）。
pub(crate) const DATA_DIR_ENV: &str = "LANPATH_DESKTOP_DATA_DIR";
/// 宿主/测试可预置的 NSH storage 键，优先级低于环境变量；仅作为启动参数，
/// 不进入持久化业务数据。
pub(crate) const DATA_DIR_STORAGE_KEY: &str = "lanpath_desktop_nsh.data_dir";

const OWNER_FILE: &str = ".lanpath-desktop-nsh";
const OWNER_CONTENT: &[u8] = b"lanpath-desktop-nsh-data-root-v1\n";
const LOCK_FILE: &str = ".lanpath-desktop-nsh.lock";

/// 默认数据根目录名（NSH 桌面独立身份，区别于 Tauri/GPUI/apps/nsh）。
pub(crate) const DATA_DIR_NAME: &str = "lanpath-desktop-nsh";

/// 禁止作为祖先目录的应用数据根：Tauri 生产目录、GPUI 实验目录与 apps/nsh
/// 验证应用目录。匹配不区分大小写；不包含本应用自身的目录名。
const FORBIDDEN_ANCESTORS: &[&str] = &[
    ".lanpath",
    ".proxy-panel-client",
    "com.ybakiame.lanpath",
    "LanpathGPUI",
    "com.ybakiame.lanpath.nsh",
];

pub(crate) struct DataRoot {
    path: PathBuf,
    /// 根身份 UUID：后续切片（激活/导入归属校验）使用；当前仅测试断言。
    #[allow(dead_code)]
    id: uuid::Uuid,
    // 永不删除此文件：争夺者必须锁定同一个 inode。
    _lock: File,
    #[cfg(windows)]
    _directories: crate::windows_file::DirectoryLocks,
}

impl DataRoot {
    pub(crate) fn open(path: &Path) -> LanpathResult<Self> {
        if path.as_os_str().is_empty() {
            return Err(invalid("NSH 桌面数据目录不能为空"));
        }
        if path.components().any(|part| part == Component::ParentDir) {
            return Err(invalid("NSH 桌面数据路径不能包含 '..'"));
        }
        let absolute = std::path::absolute(path)?;
        // 所有生产/外来目录检查先于 mkdir 和锁创建。
        check_ancestors(&absolute)?;
        let _ = needs_initialization(&absolute)?;
        #[cfg(not(windows))]
        fs::create_dir_all(&absolute)?;
        #[cfg(windows)]
        let directories = crate::windows_access::private_root(&absolute)?;
        check_ancestors(&absolute)?;
        let path = absolute.canonicalize()?;
        check_ancestors(&path)?;
        let initialize = needs_initialization(&path)?;
        let lock_path = path.join(LOCK_FILE);
        let mut options = file_options();
        options.read(true).write(true);
        if initialize {
            options.create_new(true);
        } else {
            // 已归属根缺失锁文件时不再重建：旧持有者可能仍持有已 unlink 的 inode。
            check_regular_file(&lock_path)?;
        }
        let lock = options.open(&lock_path)?;
        check_open_file(&lock, &lock_path)?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(invalid("NSH 桌面数据目录正在使用中"));
            }
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
        check_ancestors(&path)?;
        check_open_file(&lock, &lock_path)?;
        if initialize {
            // 写入身份/业务数据前先收紧新建根目录。
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
            }
            // 其他写入者不得把这个空根变成外来目录。
            for entry in fs::read_dir(&path)? {
                if entry?.file_name() != LOCK_FILE {
                    return Err(invalid("NSH 桌面数据目录在初始化期间发生了变化"));
                }
            }
            let owner_path = path.join(OWNER_FILE);
            let mut owner = file_options()
                .write(true)
                .create_new(true)
                .open(&owner_path)?;
            owner.write_all(OWNER_CONTENT)?;
            owner.sync_all()?;
        }
        check_owner(&path)?;
        check_tree(&path)?;
        fs::create_dir_all(path.join("logs"))?;
        fs::create_dir_all(path.join("cache"))?;
        let id = identity::load_or_create(&path)?;
        Ok(Self {
            path,
            id,
            _lock: lock,
            #[cfg(windows)]
            _directories: directories,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    #[cfg(test)]
    pub(crate) fn id(&self) -> uuid::Uuid {
        self.id
    }
}

impl Drop for DataRoot {
    fn drop(&mut self) {
        let _ = self._lock.unlock();
    }
}

pub(crate) fn default_data_dir() -> LanpathResult<PathBuf> {
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

#[derive(Debug, Clone)]
pub(crate) struct DataLocation {
    pub(crate) path: PathBuf,
}

impl DataLocation {
    /// 显式路径非法时是错误，绝不回退默认目录；解析在任何失败后也不重试
    /// 其他目录。
    pub(crate) fn resolve(explicit: Option<&Path>) -> LanpathResult<Self> {
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
            None => default_data_dir()?,
        };
        // 准入仍是 DataRoot::open 的职责；解析永不重试外来或生产目录。
        Ok(Self { path })
    }
}

/// 解析本进程的数据根：环境变量优先，其次宿主预置的 storage 键，最后默认目录。
/// 两者都走同一份 DataRoot::open 准入检查。
pub(crate) fn resolve_data_location() -> LanpathResult<DataLocation> {
    let explicit = std::env::var_os(DATA_DIR_ENV).filter(|value| !value.is_empty());
    let explicit = match explicit {
        Some(value) => Some(PathBuf::from(value)),
        None => nsh_core::storage::Storage::get_item(DATA_DIR_STORAGE_KEY)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from),
    };
    DataLocation::resolve(explicit.as_deref())
}

fn environment_directory(name: &str) -> LanpathResult<PathBuf> {
    let value = std::env::var_os(name)
        .ok_or_else(|| invalid(format!("{name} 是持久化 NSH 桌面数据目录的必需环境变量")))?;
    absolute_environment_directory(name, value)
}

fn absolute_environment_directory(name: &str, value: std::ffi::OsString) -> LanpathResult<PathBuf> {
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(invalid(format!("{name} 必须是绝对路径目录")));
    }
    Ok(path)
}

fn needs_initialization(root: &Path) -> LanpathResult<bool> {
    let Some(metadata) = metadata_if_present(root)? else {
        return Ok(true);
    };
    if is_link(&metadata) || !metadata.is_dir() {
        return Err(invalid("NSH 桌面数据根必须是普通目录"));
    }
    if fs::read_dir(root)?.next().transpose()?.is_none() {
        return Ok(true);
    }
    check_owner(root)?;
    Ok(false)
}

fn check_owner(root: &Path) -> LanpathResult<()> {
    let path = root.join(OWNER_FILE);
    let metadata =
        check_regular_file(&path).map_err(|_| invalid("拒绝没有 NSH 桌面归属标记的非空目录"))?;
    if metadata.len() != OWNER_CONTENT.len() as u64 {
        return Err(invalid("NSH 桌面归属标记的版本或内容未知"));
    }
    let mut file = file_options().read(true).open(&path)?;
    check_open_file(&file, &path)?;
    let mut content = [0; OWNER_CONTENT.len()];
    file.read_exact(&mut content)?;
    let mut extra = [0];
    if content != OWNER_CONTENT || file.read(&mut extra)? != 0 {
        return Err(invalid("NSH 桌面归属标记的版本或内容未知"));
    }
    Ok(())
}

fn check_ancestors(path: &Path) -> LanpathResult<()> {
    // 一旦确切的本应用标记拥有了根目录，其自身的 client.json 就属于应用状态
    // 而非生产根证据；祖先的 client.json 仍然禁止。
    let owned_self = path.is_dir() && check_owner(path).is_ok();
    for (depth, ancestor) in path.ancestors().enumerate() {
        if ancestor.file_name().is_some_and(|name| {
            FORBIDDEN_ANCESTORS.iter().any(|production| {
                name.to_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(production))
            })
        }) {
            return Err(invalid("禁止位于生产或其他 Lanpath 应用数据根之下"));
        }
        let Some(metadata) = metadata_if_present(ancestor)? else {
            continue;
        };
        if is_link(&metadata) || !metadata.is_dir() {
            return Err(invalid(format!(
                "数据路径包含符号链接、junction、reparse point 或非目录: {}",
                ancestor.display()
            )));
        }
        // 只探测元数据；绝不打开生产配置或种子数据。
        let production_marker = (depth != 0 || !owned_self)
            && (metadata_if_present(&ancestor.join("client.json"))?.is_some()
                || metadata_if_present(&ancestor.join("lanpath.portable"))?.is_some());
        let portable_layout = (metadata_if_present(&ancestor.join("Lanpath.exe"))?.is_some()
            || metadata_if_present(&ancestor.join("lanpath-app.exe"))?.is_some())
            && (metadata_if_present(&ancestor.join("data"))?.is_some()
                || metadata_if_present(&ancestor.join("seed"))?.is_some());
        if production_marker || portable_layout {
            return Err(invalid(format!(
                "拒绝生产/便携 Lanpath 祖先目录: {}",
                ancestor.display()
            )));
        }
    }
    Ok(())
}

fn check_tree(root: &Path) -> LanpathResult<()> {
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if is_link(&metadata) {
                return Err(invalid(format!(
                    "NSH 桌面数据不能包含符号链接或 junction: {}",
                    path.display()
                )));
            }
            if metadata.is_dir() {
                directories.push(path);
            } else if !metadata.is_file() || has_multiple_links(&path, &metadata)? {
                return Err(invalid(format!(
                    "NSH 桌面数据只能包含普通、未共享的文件: {}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

fn metadata_if_present(path: &Path) -> LanpathResult<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn check_regular_file(path: &Path) -> LanpathResult<Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    if is_link(&metadata) || !metadata.is_file() || has_multiple_links(path, &metadata)? {
        return Err(invalid(format!(
            "需要普通、未共享的文件: {}",
            path.display()
        )));
    }
    Ok(metadata)
}

fn check_open_file(file: &File, path: &Path) -> LanpathResult<()> {
    let pathname = check_regular_file(path)?;
    let opened = file.metadata()?;
    #[cfg(windows)]
    let shared = crate::windows_file::link_count(file)? != 1;
    #[cfg(not(windows))]
    let shared = has_multiple_links(path, &opened)?;
    if !opened.is_file() || is_link(&opened) || shared {
        return Err(invalid("归属/锁文件句柄不是普通、未共享的文件"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if pathname.dev() != opened.dev() || pathname.ino() != opened.ino() {
            return Err(invalid("归属/锁文件在打开期间被替换"));
        }
    }
    #[cfg(not(unix))]
    let _ = pathname;
    Ok(())
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // 包括 junction 和其他 reparse point，不只是符号链接。
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn has_multiple_links(path: &Path, metadata: &Metadata) -> LanpathResult<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = path;
        Ok(metadata.nlink() != 1)
    }
    #[cfg(windows)]
    {
        let _ = metadata;
        let file = file_options().read(true).open(path)?;
        Ok(crate::windows_file::link_count(&file)? != 1)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (path, metadata);
        Err(invalid("该平台不支持文件链接计数"))
    }
}

fn file_options() -> OpenOptions {
    #[allow(unused_mut)]
    let mut options = OpenOptions::new();
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // 打开 reparse point 本身，绝不打开其目标。保留读/写共享以允许锁争夺，
        // 但句柄存活期间禁止删除。
        options.custom_flags(0x0020_0000).share_mode(0x1 | 0x2);
    }
    options
}

fn invalid(message: impl Into<String>) -> LanpathError {
    LanpathError::Config(message.into())
}

#[cfg(test)]
#[path = "data_root_tests.rs"]
mod tests;
