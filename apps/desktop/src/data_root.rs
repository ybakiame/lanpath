//! Lanpath 独立数据目录：所有权标记 + 进程独占锁 + 链接路径安全准入。
//!
//! 程序同级存在且可写的 `data/` 启用便携模式，否则使用平台标准目录（见 `location`）。
//! 准入全部通过并持锁后初始化统一数据布局（`app.toml`、`subscriptions.toml`、
//! `overwrite/sing-box/*.json`、`logs/`、`.cache/`、`runtime/db/`、`runtime/<core>/`）。

use lanpath_common::{LanpathError, LanpathResult};
use std::collections::HashMap;
use std::fs::{self, File, Metadata, OpenOptions, TryLockError};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

#[path = "data_root_identity.rs"]
mod identity;

#[path = "data_location.rs"]
mod location;
pub(crate) use location::{prepare_native_data_dir, resolve_data_location};

const OWNER_FILE: &str = ".lanpath-root";
const OWNER_CONTENT: &[u8] = b"lanpath-data-root-v1\n";
const LOCK_FILE: &str = ".lanpath.lock";

pub(crate) struct DataRoot {
    path: PathBuf,
    /// 根身份 UUID：系统代理守护日志的 owner（跨进程精确身份）。
    id: uuid::Uuid,
    // 永不删除此文件：争夺者必须锁定同一个 inode。
    _lock: File,
    /// 本根全部业务（订阅/覆写模板）清单与配置的串行化锁：同根所有
    /// 服务共用同一把锁，校验+落盘在同一临界区内完成，杜绝“校验无
    /// 引用后另一线程完成绑定”的竞争。锁不跨网络 await（刷新在 await
    /// 前后分别短暂取锁）。
    business: Mutex<()>,
    /// 异步活动屏障：跨 await 的长任务（订阅刷新写回、配置合成、核心
    /// 下载、核心启动）持读保护直到完成；备份恢复准备取写保护并等待
    /// 全部读者清零，持写期间新读者排队。business 锁不跨 await 的约束
    /// 不变——活动屏障只为“准备恢复前让在途任务安静”服务。
    activity: Arc<tokio::sync::RwLock<()>>,
    #[cfg(windows)]
    _directories: crate::windows_file::DirectoryLocks,
}

impl DataRoot {
    pub(crate) fn open(path: &Path) -> LanpathResult<Self> {
        if path.as_os_str().is_empty() {
            return Err(invalid("Lanpath 数据目录不能为空"));
        }
        if path.components().any(|part| part == Component::ParentDir) {
            return Err(invalid("Lanpath 数据路径不能包含 '..'"));
        }
        let absolute = std::path::absolute(path)?;
        // 路径与归属检查先于 mkdir 和锁创建。
        check_ancestors(&absolute)?;
        let pre_initialize = needs_initialization(&absolute)?;
        #[cfg(not(windows))]
        let _ = pre_initialize;
        #[cfg(not(windows))]
        fs::create_dir_all(&absolute)?;
        #[cfg(windows)]
        let directories = crate::windows_access::private_root(&absolute, pre_initialize)?;
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
                return Err(invalid("Lanpath 数据目录正在使用中"));
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
                    return Err(invalid("Lanpath 数据目录在初始化期间发生了变化"));
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
        // 启动恢复事务：数据根独占锁内、任何业务初始化之前，先原子应用
        // 待恢复备份（prepare 阶段落下的 .backup-pending）。共享事务保证
        // 要么全部落地、要么原配置保留；恢复日志含糊（崩溃中断）时真实
        // 失败，绝不带着半恢复状态继续启动。
        lanpath_client::backup::apply_pending_at_startup(&path)?;
        // 归属/树/独占锁全部就绪后，初始化统一 TOML 存储与运行时子目录。
        crate::storage_init::initialize(&path)?;
        fs::create_dir_all(path.join("logs"))?;
        fs::create_dir_all(path.join(".cache"))?;
        fs::create_dir_all(path.join("runtime/db"))?;
        fs::create_dir_all(path.join("runtime/sing-box"))?;
        fs::create_dir_all(path.join("runtime/mihomo"))?;
        let id = identity::load_or_create(&path)?;
        Ok(Self {
            path,
            id,
            _lock: lock,
            business: Mutex::new(()),
            activity: Arc::new(tokio::sync::RwLock::new(())),
            #[cfg(windows)]
            _directories: directories,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// 本根业务的串行化锁（见字段说明）：锁中毒（持锁线程 panic）时
    /// 接管锁，宁可串行继续也不让整个页面失效。
    pub(crate) fn business_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.business
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    /// 异步活动屏障（见字段说明）：跨 await 的长任务持读保护，备份恢复
    /// 准备取写保护并等待在途任务清零。调用方按“activity → mutation →
    /// business”的固定顺序取锁，无反向路径。
    pub(crate) fn activity(&self) -> &Arc<tokio::sync::RwLock<()>> {
        &self.activity
    }

    /// 是否已有待恢复的备份（`.backup-pending` 存在）。文件由共享准备
    /// 事务在业务锁内落盘、由重启后的启动事务清除；检查方在同一把业务
    /// 锁内读取——准备落盘与写路径检查互斥，无 TOCTOU。
    pub(crate) fn is_restore_pending(&self) -> bool {
        self.path.join(".backup-pending").exists()
    }

    /// 写路径的恢复待定门：准备恢复后至重启应用前，订阅/覆写/请求设置/
    /// 核心库存/核心启动等一切业务写拒绝（读不受限），防止变更被重启
    /// 后的恢复覆盖。调用方须已持有本根业务锁——准备路径在同一锁内落
    /// `.backup-pending`，检查与落盘互斥，杜绝 TOCTOU。
    pub(crate) fn ensure_writable(&self) -> LanpathResult<()> {
        if self.is_restore_pending() {
            return Err(LanpathError::Client(
                "已准备恢复备份，重启应用前不可修改；重启完成前如需放弃，请先重启应用".into(),
            ));
        }
        Ok(())
    }

    /// 本根身份 UUID（系统代理守护日志 owner；也用于测试断言）。
    pub(crate) fn id(&self) -> uuid::Uuid {
        self.id
    }
}

impl Drop for DataRoot {
    fn drop(&mut self) {
        let _ = self._lock.unlock();
    }
}

fn needs_initialization(root: &Path) -> LanpathResult<bool> {
    let Some(metadata) = metadata_if_present(root)? else {
        return Ok(true);
    };
    if is_link(&metadata) || !metadata.is_dir() {
        return Err(invalid("Lanpath 数据根必须是普通目录"));
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
        check_regular_file(&path).map_err(|_| invalid("拒绝没有 Lanpath 归属标记的非空目录"))?;
    if metadata.len() != OWNER_CONTENT.len() as u64 {
        return Err(invalid("Lanpath 归属标记的版本或内容未知"));
    }
    let mut file = file_options().read(true).open(&path)?;
    check_open_file(&file, &path)?;
    let mut content = [0; OWNER_CONTENT.len()];
    file.read_exact(&mut content)?;
    let mut extra = [0];
    if content != OWNER_CONTENT || file.read(&mut extra)? != 0 {
        return Err(invalid("Lanpath 归属标记的版本或内容未知"));
    }
    Ok(())
}

fn check_ancestors(path: &Path) -> LanpathResult<()> {
    for ancestor in path.ancestors() {
        let Some(metadata) = metadata_if_present(ancestor)? else {
            continue;
        };
        if is_link(&metadata) || !metadata.is_dir() {
            return Err(invalid(format!(
                "数据路径包含符号链接、junction、reparse point 或非目录: {}",
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
                    "Lanpath 数据不能包含符号链接或 junction: {}",
                    path.display()
                )));
            }
            if metadata.is_dir() {
                directories.push(path);
            } else if !metadata.is_file() || has_multiple_links(&path, &metadata)? {
                return Err(invalid(format!(
                    "Lanpath 数据只能包含普通、未共享的文件: {}",
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

/// 进程内按路径缓存已准入的数据根：根目录的锁租约随进程存活，
/// 同一数据根只准入一次（第二个应用实例会被租约拒绝）。订阅、日志等
/// 业务共用同一条租约，避免同进程重复打开同一根。
static ROOTS: OnceLock<Mutex<HashMap<PathBuf, Arc<DataRoot>>>> = OnceLock::new();

pub(crate) fn open_root(path: &Path) -> LanpathResult<Arc<DataRoot>> {
    let absolute = std::path::absolute(path)?;
    let lookup = absolute.canonicalize().unwrap_or_else(|_| absolute.clone());
    let mut roots = ROOTS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(root) = roots.get(&lookup) {
        return Ok(root.clone());
    }
    let root = Arc::new(DataRoot::open(&absolute)?);
    roots.insert(root.path.clone(), root.clone());
    Ok(root)
}

#[cfg(test)]
#[path = "data_root_tests.rs"]
mod tests;
