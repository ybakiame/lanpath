//! 核心库存与下载/选择业务：共享 `lanpath_client::core_management::
//! CoreManagementService::experimental`（confined 策略）落在 NSH 桌面自己的
//! 隔离数据根内。只管理本根 `cores/<binary>/<version>/` 下的真实核心文件：
//! 列出本地真实版本、查询远端通道（仅 stable/prerelease）、下载指定已查询
//! 版本（共享语义：验证 + 自动选中 + 按通道保留当前与前一版本）、选择本地
//! 版本、删除非 active 版本（共享层保护 active 不可删）。
//!
//! 选择持久化固定写 `app.toml`（统一 TOML 布局，数据根初始化时已建并持久化
//! 新装默认：直连 None、`mitm_enabled=false`）：共享 persist_selection 读-改-
//! 写 `[core]` 段，选择只改 `core_binary`/`core_type` 两个字段，订阅、active
//! 订阅、请求代理等全部保留。
//!
//! 跨页面/跨句柄互斥：UI busy 只管本页按钮；离页后新页面的新句柄 busy 为
//! 假，必须靠业务层按数据根共享的 mutation 锁杜绝并发下载/选择/删除
//! （否则两个下载并发写同一版本目录，或下载与删除/选择交错）。锁为
//! `try_acquire`：任一 mutation 在途时其余 mutation 立即报“另一项核心库存
//! 操作正在进行，请稍后重试”，绝不排队——排队会让用户在不知情时触发
//! 意外操作。锁覆盖整个 mutation（tokio guard 可跨 await）；只读操作
//! （active/list/remote 查询）不加。`DataRoot::business_guard` 仍是短临界区
//! 专用、绝不跨网络 await，取锁顺序恒为 mutation → business。
//!
//! 与核心运行时的互斥（`crate::core_runtime`）：启动/停止持有同一把 mutation
//! 锁（在途下载/选择/删除立即 busy）；`select`/`download`/`delete` 在
//! mutation 锁后真实检查 `CoreRuntime::is_running_sync`（OS 进程身份核验，
//! 不看 UI 状态），核心运行中（含停止进行中 running 未清）立即拒绝修改
//! 核心文件或选择——运行中禁止修改核心文件。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use lanpath_client::core_management::{CoreManagementService, ManagedCore, RemoteChannel};
use lanpath_client::cores::CoreDownloadStage;
use lanpath_common::{CoreType, LanpathError, LanpathResult};

use crate::data_root::{DataRoot, open_root, resolve_data_location};
use crate::request_settings::load_request_config;

/// 注入 release metadata API 的环境变量（测试与显式调用）；未设置时生产仍
/// 走共享库存默认的 GitHub。与数据根覆盖同一优先级规则：环境变量高于
/// storage 键。仅影响核心页的查询/下载目标，不改变其他请求路径。
pub(crate) const API_BASE_ENV: &str = "LANPATH_DESKTOP_CORE_API_BASE";
/// 宿主/测试可预置的 release metadata API storage 键（headless 场景经
/// 内存 storage 预置，每场景独立）。
pub(crate) const API_BASE_STORAGE_KEY: &str = "lanpath_desktop_nsh.core_api_base";
/// mutation 在途时的共享拒绝文案（业务测试与页面断言同一子串）。
pub(crate) const MUTATION_BUSY: &str = "另一项核心库存操作正在进行，请稍后重试";

/// 按数据根共享的 mutation 锁：同根的所有 `CoreInventory` 句柄（不同页面
/// 实例、测试双句柄）竞争同一把锁；根句柄全部释放后 Weak 条目自然失效。
static MUTATION_LOCKS: OnceLock<std::sync::Mutex<HashMap<PathBuf, Weak<tokio::sync::Mutex<()>>>>> =
    OnceLock::new();

/// 同根共享的后台库存活动：页面销毁不会取消 `background` worker，后续
/// 页面和 App 级摘要仍需要看到真实在途状态与完成事实。活动状态只保存
/// 语义类型/进度，不保存翻译后的文案。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InventoryActivityKind {
    Download,
    Select,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InventoryActivity {
    pub(crate) kind: InventoryActivityKind,
    pub(crate) stage: Option<CoreDownloadStage>,
}

#[derive(Default)]
struct SharedActivity {
    current: Mutex<Option<InventoryActivity>>,
    revision: AtomicU64,
}

static ACTIVITY: OnceLock<Mutex<HashMap<PathBuf, Weak<SharedActivity>>>> = OnceLock::new();

fn activity_for(root: &Path) -> Arc<SharedActivity> {
    let mut activities = ACTIVITY
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(activity) = activities.get(root).and_then(Weak::upgrade) {
        return activity;
    }
    let activity = Arc::new(SharedActivity::default());
    activities.insert(root.to_path_buf(), Arc::downgrade(&activity));
    activity
}

/// 活动登记 guard：业务方法返回、失败或取消时都会清除活动并推进版本，
/// 不依赖页面 UI 回调是否还活着。
struct ActivityGuard {
    shared: Arc<SharedActivity>,
}

impl ActivityGuard {
    fn begin(shared: Arc<SharedActivity>, activity: InventoryActivity) -> Self {
        if let Ok(mut current) = shared.current.lock() {
            *current = Some(activity);
        }
        shared.revision.fetch_add(1, Ordering::Release);
        Self { shared }
    }

    fn stage(&self, stage: Option<CoreDownloadStage>) {
        if let Ok(mut current) = self.shared.current.lock()
            && let Some(current) = current.as_mut()
            && current.stage != stage
        {
            current.stage = stage;
            self.shared.revision.fetch_add(1, Ordering::Release);
        }
    }
}

impl Drop for ActivityGuard {
    fn drop(&mut self) {
        if let Ok(mut current) = self.shared.current.lock() {
            *current = None;
        }
        self.shared.revision.fetch_add(1, Ordering::Release);
    }
}

fn mutation_lock_for(root: &Path) -> Arc<tokio::sync::Mutex<()>> {
    let mut locks = MUTATION_LOCKS
        .get_or_init(|| std::sync::Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(lock) = locks.get(root).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    locks.insert(root.to_path_buf(), Arc::downgrade(&lock));
    lock
}

/// 核心库存业务句柄：持有共享数据根、confined 共享服务与同根共享的
/// mutation 锁；克隆廉价。所有清单/配置的同步读写经数据根业务锁，与订阅/
/// 覆写/请求设置串行；网络 await 不持业务锁（共享库存在请求时自行读取已
/// 保存的请求策略快照），mutation 锁除外——它专为跨 await 的 mutation 互斥。
#[derive(Clone)]
pub(crate) struct CoreInventory {
    root: Arc<DataRoot>,
    service: CoreManagementService,
    mutation: Arc<tokio::sync::Mutex<()>>,
    activity: Arc<SharedActivity>,
}

impl CoreInventory {
    /// 解析本进程数据根并打开核心库存业务（应用启动路径）。
    pub(crate) fn open() -> LanpathResult<Self> {
        let location = resolve_data_location()?;
        Self::build(&location.path, configured_api_base())
    }

    /// 指定数据根打开（测试与显式调用路径，同样走完整准入）。
    pub(crate) fn for_root(path: &Path) -> LanpathResult<Self> {
        Self::build(path, None)
    }

    /// 指定数据根与 release metadata API 打开（测试注入本地 fixture）。
    #[cfg(test)]
    pub(crate) fn for_root_with_api_base(
        path: &Path,
        api_base: impl Into<String>,
    ) -> LanpathResult<Self> {
        Self::build(path, Some(api_base.into()))
    }

    fn build(path: &Path, api_base: Option<String>) -> LanpathResult<Self> {
        let root = open_root(path)?;
        let mut service = CoreManagementService::experimental(root.path().to_path_buf());
        if let Some(base) = api_base {
            service = service.with_api_base(base);
        }
        let mutation = mutation_lock_for(root.path());
        let activity = activity_for(root.path());
        Ok(Self {
            root,
            service,
            mutation,
            activity,
        })
    }

    /// 当前同根共享的后台 mutation（不同页面看到同一事实）。
    pub(crate) fn activity(&self) -> Option<InventoryActivity> {
        self.activity
            .current
            .lock()
            .ok()
            .and_then(|activity| activity.clone())
    }

    /// 活动状态版本：无活动时也会在完成时推进，供 App/页面检测需要重读。
    pub(crate) fn activity_revision(&self) -> u64 {
        self.activity.revision.load(Ordering::Acquire)
    }

    fn begin_activity(&self, kind: InventoryActivityKind) -> ActivityGuard {
        ActivityGuard::begin(
            self.activity.clone(),
            InventoryActivity { kind, stage: None },
        )
    }

    /// 取 mutation 锁；在途时立即拒绝（不排队）。调用方持有 guard 直至
    /// mutation 完成（tokio guard 可跨 await）。核心运行时的启动/停止也经
    /// 这一入口与库存互斥。
    pub(crate) fn try_mutation(&self) -> LanpathResult<tokio::sync::MutexGuard<'_, ()>> {
        self.mutation
            .try_lock()
            .map_err(|_| LanpathError::Client(MUTATION_BUSY.to_owned()))
    }

    /// 真实检查核心运行时是否持有运行中的核心（OS 进程身份核验，不看 UI
    /// 状态）；运行中（含停止进行中 running 未清）拒绝修改核心文件或选择。
    fn ensure_not_running(&self) -> LanpathResult<()> {
        if crate::core_runtime::CoreRuntime::cached(self.root.path())
            .is_some_and(|runtime| runtime.is_running_sync())
        {
            return Err(LanpathError::Client(
                "核心运行期间不可修改核心文件或选择，请先停止核心".into(),
            ));
        }
        Ok(())
    }

    /// 实际选中核心，独立于正在浏览的库存类型；尚未选择时 `Ok(None)`，
    /// 不把“无核心”当错误。损坏配置返回真实错误。只读，不加 mutation 锁。
    pub(crate) fn active_core(&self) -> LanpathResult<Option<ManagedCore>> {
        let _guard = self.root.business_guard();
        let config = load_request_config(self.root.path())?;
        if config.core_binary.as_os_str().is_empty() {
            return Ok(None);
        }
        self.service.active_core().map(Some)
    }

    /// 列出指定库存类型的本地真实版本（含 active 标记，active 行带真实
    /// version probe 详情）。目录不存在按空列表，损坏/越界返回真实错误。
    /// 只读，不加 mutation 锁。
    pub(crate) fn list_local(&self, core_type: CoreType) -> LanpathResult<Vec<ManagedCore>> {
        let _guard = self.root.business_guard();
        self.service.for_core(core_type).list_local()
    }

    /// 查询远端各通道最新版本（共享库存实现），只保留 stable/prerelease
    /// 两条通道。共享库每次请求自行读取已保存的 ClientConfig 请求策略
    /// （`request_client_builder` + GitHub 镜像前缀），网络 await 不持业务锁。
    /// 只读，不加 mutation 锁——查询可与在途 mutation 并发。
    pub(crate) async fn remote_channels(
        &self,
        core_type: CoreType,
    ) -> LanpathResult<Vec<RemoteChannel>> {
        let service = self.service.for_core(core_type);
        let remote = service.list_remote_channels().await?;
        Ok(remote
            .into_iter()
            .filter(|channel| matches!(channel.channel(), "stable" | "prerelease"))
            .collect())
    }

    /// 选择本地版本：共享 confined 校验（库存内真实未链接二进制 + version
    /// probe 精确匹配）后只改 `core_binary`/`core_type` 并写入 `app.toml`。
    /// 版本探测在后台线程执行（调用方负责），业务锁不跨 await。与下载/
    /// 删除共享同根 mutation 锁，在途时立即拒绝。
    pub(crate) fn select(&self, core_type: CoreType, path: &Path) -> LanpathResult<()> {
        let _mutation = self.try_mutation()?;
        self.ensure_not_running()?;
        let _guard = self.root.business_guard();
        self.root.ensure_writable()?;
        let _activity = self.begin_activity(InventoryActivityKind::Select);
        tracing::info!(target: "lanpath_desktop_nsh::core_inventory", "选择核心：{core_type}");
        self.service.for_core(core_type).select(path)
    }

    /// 下载指定已查询版本：共享语义——下载、解压、version probe 验证、
    /// 自动选中（失败保留旧选中，选择写入 `app.toml`），并按通道保留当前
    /// 与前一版本（剪枝）。进度回调只回传共享真实阶段，不伪造 byte 进度；
    /// mutation 锁覆盖整个下载（跨网络 await），在途的其它下载/选择/删除
    /// 立即拒绝。不包含 wintun/TUN 资源（confined 策略向共享层传入不取
    /// TUN 附件；TUN 资源将来单独处理）。
    pub(crate) async fn download(
        &self,
        core_type: CoreType,
        version: &str,
        progress: &(dyn Fn(CoreDownloadStage) + Send + Sync),
    ) -> LanpathResult<ManagedCore> {
        // 活动读保护覆盖整个下载（跨网络 await）：备份恢复准备（写保护）
        // 会等下载真正结束，绝不只凭页面 busy；恢复已准备（待重启）时
        // 直接拒绝。
        let _activity = self.root.activity().read().await;
        self.root.ensure_writable()?;
        let _mutation = self.try_mutation()?;
        self.ensure_not_running()?;
        tracing::info!(target: "lanpath_desktop_nsh::core_inventory", "下载核心：{core_type} {version}");
        let service = self.service.for_core(core_type);
        let activity = self.begin_activity(InventoryActivityKind::Download);
        let report = |stage: CoreDownloadStage| {
            activity.stage(Some(stage));
            progress(stage);
        };
        let installed = service.download_with_progress(version, &report).await?;
        tracing::info!(target: "lanpath_desktop_nsh::core_inventory", "下载核心完成：{} {}", installed.core_type(), installed.version());
        Ok(installed)
    }

    /// 删除本地版本目录；共享层拒绝删除 active/越界/链接核心，调用方上屏
    /// 真实错误。与下载/选择共享同根 mutation 锁，在途时立即拒绝。
    pub(crate) fn delete(&self, core_type: CoreType, path: &Path) -> LanpathResult<()> {
        let _mutation = self.try_mutation()?;
        self.ensure_not_running()?;
        let _guard = self.root.business_guard();
        self.root.ensure_writable()?;
        let _activity = self.begin_activity(InventoryActivityKind::Delete);
        tracing::info!(target: "lanpath_desktop_nsh::core_inventory", "删除核心：{core_type}");
        self.service.for_core(core_type).delete(path)
    }
}

/// 解析注入的 release metadata API：环境变量优先，其次宿主预置的
/// storage 键；两者都未设置时 `None`（共享库默认 GitHub）。
fn configured_api_base() -> Option<String> {
    let from_env = std::env::var(API_BASE_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    from_env.or_else(|| {
        nsh_core::storage::Storage::get_item(API_BASE_STORAGE_KEY)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    })
}

/// 行数据转换助手：本地版本行的稳定标识（类型 + 版本目录 + active）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalCoreRow {
    pub(crate) core_type: CoreType,
    pub(crate) version: String,
    pub(crate) path: PathBuf,
    pub(crate) active: bool,
}

impl LocalCoreRow {
    pub(crate) fn from_managed(core: &ManagedCore) -> Self {
        Self {
            core_type: core.core_type(),
            version: core.version().to_owned(),
            path: core.path().to_path_buf(),
            active: core.is_active(),
        }
    }
}

#[cfg(test)]
#[path = "core_inventory_tests.rs"]
mod tests;
