//! 加密备份与恢复准备业务：共享 `lanpath-client::backup` 落在 NSH 桌面
//! 自己的隔离数据根内（统一 TOML 布局；外观不属于本格式，导出/恢复
//! 固定传 `None`）。只复用共享语义，不复制加密/事务实现：
//!
//! - 导出：共享 `BackupService::export` 真实 age 加密（scrypt 口令），
//!   写出到用户经 NSH 原生保存对话框选定的目标——绝不覆盖已有文件，
//!   失败清理半成品；口令只活在内存，成功/失败都不落 storage/日志。
//! - 导入：共享 `inspect` 解密并校验后返回一次性 `preview_id`（300 秒
//!   过期、二次消费即失效）；改文件/改口令由调用方立即 `discard`。
//! - 恢复准备（`prepare_confirmed_restore`）：先消费一次性预览授权，
//!   再取数据根活动屏障写保护——等在途订阅刷新/配置合成/核心下载/
//!   核心启动全部完成，持写期间新长任务排队；随后停止核心（含系统
//!   代理守护恢复），最后在业务锁内生成加密回滚并落 `.backup-pending`
//!   （与全部写路径互斥，防 TOCTOU）。任一步失败都不遗留 pending，可
//!   重试；成功后一切业务写被 `DataRoot::ensure_writable` 拒绝，直到
//!   重启应用由数据根启动事务原子应用。
//! - 启动事务在 `DataRoot::open` 独占锁内完成（见 data_root），这里只
//!   读结果：`restored()`（含 `.backup-failed` 真实错误）与
//!   `acknowledge()`（用户确认恢复提示后清除标记）。
//!
//! 日志只记文件名、计数、字节与路径（路径限本根），绝不记口令、明文
//! 或订阅正文。

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, Weak};

use lanpath_client::backup::{BackupPreview, MAX_ARCHIVE};
use lanpath_common::{LanpathError, LanpathResult};

use crate::core_runtime::CoreRuntime;
use crate::data_root::{DataRoot, open_root, resolve_data_location};

/// 按数据根共享的备份业务句柄：Weak 缓存，全部句柄释放后自动移除。
static SERVICES: OnceLock<std::sync::Mutex<HashMap<PathBuf, Weak<Backup>>>> = OnceLock::new();

/// 备份业务句柄：持有共享数据根与共享 `BackupService`（进程内预览缓存），
/// 克隆廉价。加密/事务全部由共享层完成，本层只做调度与门控。
pub(crate) struct Backup {
    root: Arc<DataRoot>,
    service: lanpath_client::backup::BackupService,
}

impl Backup {
    /// 解析本进程数据根并打开备份业务（应用启动路径）。
    pub(crate) fn open() -> LanpathResult<Arc<Self>> {
        let location = resolve_data_location()?;
        Self::for_root(&location.path)
    }

    /// 指定数据根打开（测试与显式调用路径，同样走完整准入与启动事务）。
    pub(crate) fn for_root(path: &Path) -> LanpathResult<Arc<Self>> {
        let root = open_root(path)?;
        let key = root.path().to_path_buf();
        let mut services = SERVICES
            .get_or_init(|| std::sync::Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(service) = services.get(&key).and_then(Weak::upgrade) {
            return Ok(service);
        }
        let service = Arc::new(Self {
            service: lanpath_client::backup::BackupService::new(key.clone()),
            root,
        });
        services.insert(key, Arc::downgrade(&service));
        Ok(service)
    }

    /// 导出当前配置的加密备份（真实 age 加密；CPU 密集，调用方须放入
    /// blocking 线程调度）。外观按本页格式固定 `None`（NSH 外观 storage
    /// 不属于备份格式）。口令长度先在这里给出明确错误（共享层同样校验）。
    pub(crate) fn export_encrypted(&self, password: &str) -> LanpathResult<Vec<u8>> {
        if password.chars().count() < 12 {
            return Err(LanpathError::Validation(
                "备份密码至少 12 个字符；建议使用“生成强密码”".into(),
            ));
        }
        let bytes = self.service.export(password, None)?;
        tracing::info!(target: "lanpath_desktop_nsh::backup",
            "导出加密备份完成：{} 字节", bytes.len());
        Ok(bytes)
    }

    /// 把加密字节写到用户选定的目标：`create_new` 绝不覆盖已有文件，
    /// 写出失败清理半成品；源字节只读，口令与明文不经此函数。
    pub(crate) fn write_export(&self, target: &Path, bytes: &[u8]) -> LanpathResult<PathBuf> {
        let target = target.to_path_buf();
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|error| export_error(&target, &error))?;
        let result = out.write_all(bytes).and_then(|()| out.sync_all());
        if let Err(error) = result {
            drop(out);
            let _ = std::fs::remove_file(&target);
            return Err(export_error(&target, &error));
        }
        tracing::info!(target: "lanpath_desktop_nsh::backup",
            "加密备份已写出：{}", target.display());
        Ok(target)
    }

    /// 读取待导入的加密备份：先按共享上限拒绝超限文件，再读入并复核
    /// 实际字节数（稀疏/竞态增长都不放过）。
    pub(crate) fn read_import(&self, source: &Path) -> LanpathResult<Vec<u8>> {
        let file = std::fs::File::open(source).map_err(|error| import_error(source, &error))?;
        let len = file.metadata()?.len();
        if len > MAX_ARCHIVE as u64 {
            return Err(LanpathError::Validation(format!(
                "备份文件超过 {} MiB 上限，请更换文件",
                MAX_ARCHIVE / 1024 / 1024
            )));
        }
        let mut bytes = Vec::new();
        file.take((MAX_ARCHIVE + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_ARCHIVE {
            return Err(LanpathError::Validation(
                "备份文件超过 16 MiB 上限，请更换文件".into(),
            ));
        }
        Ok(bytes)
    }

    /// 解密并校验备份，返回一次性预览（id 绑定、300 秒过期；明文与口令
    /// 只进进程内缓存，绝不持久化）。共享层对一切失败只回脱敏错误。
    pub(crate) fn inspect(
        &self,
        encrypted: &[u8],
        password: String,
    ) -> LanpathResult<BackupPreview> {
        let preview = self.service.inspect(encrypted, password)?;
        tracing::info!(target: "lanpath_desktop_nsh::backup",
            "备份解密预览完成：订阅 {} · 覆写 {} · 文件 {} · {} 字节",
            preview.summary.subscriptions, preview.summary.profiles, preview.summary.files, preview.summary.bytes);
        Ok(preview)
    }

    /// 释放未使用的预览（修改文件/口令、离页、过期调度时调用）。
    pub(crate) fn discard(&self, preview_id: &str) {
        self.service.discard(preview_id);
    }

    /// 消费一次性预览授权（测试直达共享入口；生产路径走
    /// [`Backup::prepare_confirmed_restore`]）。
    #[cfg(test)]
    pub(crate) fn take_preview(
        &self,
        preview_id: &str,
    ) -> LanpathResult<lanpath_client::backup::RestoreInput> {
        self.service.take_preview(preview_id)
    }

    /// 启动后的恢复结果（只读）：`.backup-failed` 存在时返回真实错误
    /// （原配置已保留，需重新导入）；`.backup-applied.json` 存在时返回
    /// 恢复摘要。用户确认提示后由 [`Backup::acknowledge`] 清除。
    pub(crate) fn restored(&self) -> LanpathResult<Option<lanpath_client::backup::BackupSummary>> {
        self.service.restored()
    }

    /// 用户已看到恢复提示：清除 `.backup-applied.json` / `.backup-failed`。
    pub(crate) fn acknowledge(&self) -> LanpathResult<()> {
        self.service.acknowledge()?;
        tracing::info!(target: "lanpath_desktop_nsh::backup", "恢复提示已确认");
        Ok(())
    }

    /// 是否有待恢复的备份（`.backup-pending` 或本进程镜像）。
    pub(crate) fn is_pending(&self) -> bool {
        self.root.is_restore_pending()
    }

    /// 确认后的恢复准备（见模块说明）：消费一次性预览授权 → 活动屏障
    /// 写保护 → 停止核心（含守护恢复）→ 业务锁内生成加密回滚并落
    /// pending。返回回滚文件路径（仅文件名与路径可入日志/上屏）。
    pub(crate) async fn prepare_confirmed_restore(
        &self,
        preview_id: &str,
        runtime: &Arc<CoreRuntime>,
    ) -> LanpathResult<PathBuf> {
        if self.is_pending() {
            return Err(LanpathError::Client(
                "已有待恢复的备份，请重启应用完成恢复".into(),
            ));
        }
        // 先消费一次性授权：任何后续失败都不改变数据，可重新解密重试。
        let input = self.service.take_preview(preview_id)?;
        // 活动屏障写保护：等在途长任务清零；持写期间新长任务排队，拿到
        // 读保护后仍会先过 pending 门，不会闯进恢复窗口。
        let _activity = self.root.activity().write().await;
        if self.root.is_restore_pending() {
            return Err(LanpathError::Client(
                "已有待恢复的备份，请重启应用完成恢复".into(),
            ));
        }
        // 停止核心（先按原值恢复系统代理，再回收核心子进程）；停止失败
        // 即放弃准备，不碰配置。
        runtime
            .stop()
            .await
            .map_err(|error| LanpathError::Core(format!("停止核心失败：{error}；未改动配置")))?;
        // 业务锁内准备：回滚快照捕获与 pending 落盘和全部写路径互斥
        // （写路径在同一锁内过 pending 门），杜绝“检查通过后准备落盘”
        // 的 TOCTOU。共享准备失败不遗留 pending，允许重试。
        let rollback = {
            let _business = self.root.business_guard();
            PathBuf::from(input.prepare(self.root.path(), None)?)
        };
        tracing::info!(target: "lanpath_desktop_nsh::backup",
            "恢复准备完成，重启后生效；回滚备份：{}", rollback.display());
        Ok(rollback)
    }
}

fn export_error(target: &Path, error: &io::Error) -> LanpathError {
    LanpathError::Config(format!("写出备份失败（{}）：{error}", target.display()))
}

fn import_error(source: &Path, error: &io::Error) -> LanpathError {
    LanpathError::Config(format!("读取备份文件失败（{}）：{error}", source.display()))
}

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;
