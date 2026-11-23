//! 核心运行时：真实核心二进制的用户态启停 + 经独立守护的 Windows 系统
//! 代理精确接管/恢复（TUN 仍拒绝，不接管 VPN）。
//!
//! `CoreRuntime` 按数据根共享：Weak 缓存 + App 启动时 `stored` 持有——同一
//! 数据根的所有页面/句柄看到同一份真实运行状态；GUI 正常退出时随 `Drop`
//! 先尽力恢复系统代理、再按记录的 OS 进程身份回收本次子进程（另有 spawn
//! 时的 OS 父子绑定兜底），绝不按进程名强杀。
//!
//! 启动固定同一份 `ClientConfig` 快照：拒绝 `tun_enabled`；confined 库存
//! 核心 + `validate_managed_binary`；预检 mixed/API 端口 loopback 可绑定；
//! `compose_with_config(快照)` 一次合成；`ConfigValidator` 校验后才 `start`
//! 并等待真实监听/API 就绪。配置 `system_proxy_enabled=true` 且平台支持时：
//! 核心校验/mixed/API 全部就绪后 arm 同映像独立守护并 acquire
//! `127.0.0.1:mixed_port`；接管失败保留核心与守护证据（不 rollback、不
//! 先清证据）。启动前先恢复本应用 abandoned journal（同一独立守护完成，
//! GUI 死亡不能中断其检查点）。正常停止先守护 restore + 确认退出，再停
//! 核心；通信失败/守护死亡不是恢复证明，只按本应用 owner 精确恢复。
//! 非 Windows 不接管：已保存偏好不产生运行失败（如实记录日志）。
//!
//! spawn 后任何失败（start 失败、就绪超时、pid 身份读取失败）都走同一
//! `rollback` 显式 `stop_process` 回收（manager drop 在 Unix 只 detach，
//! 不得提前返回留半截清理）；回滚失败同时报告原错误与回收错误并保留可
//! 重试状态（再次启动/停止会先重试回收）。
//!
//! 与库存的互斥：启动/停止持有 `CoreInventory` 同根 mutation 锁（在途下载/
//! 选择/删除立即 busy）；运行中（含停止进行中 running 未清）库存 mutation 由
//! `is_running_sync` 真实核验拒绝。取锁顺序恒为 activity（启动持读，备份恢复
//! 准备持写）→ mutation → lifecycle → proxy_guardian → business/active
//! （短临界区，business 只在 arm 守护前的读取阶段持有），无反向路径；
//! 守护 RPC 绝不持 business 锁，不会死锁。
//!
//! 日志只记操作、端口、版本与阶段，不记配置正文、节点/代理凭据或完整 URL。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use lanpath_client::core_management::CoreManagementService;
use lanpath_client::port_guard;
use lanpath_common::{LanpathError, LanpathResult, ProcessIdentity};

use crate::config_composer::ConfigComposer;
use crate::config_validator::{ConfigValidator, ValidatorBindings};
use crate::core_inventory::CoreInventory;
use crate::data_root::{DataRoot, open_root, resolve_data_location};
use crate::request_settings::load_request_config;
use crate::system_proxy::{ArmedGuardian, GuardianSpawner};

mod lifecycle;
mod manager;
mod proxy;
mod status;
mod telemetry;
mod types;

use manager::{ActiveManager, parse_preview, read_pid_identity};
use types::ActiveInfo;
pub(crate) use types::{
    CoreExit, CoreOperation, CoreStatus, RuntimeStage, SessionSummary, StartOutcome,
};

/// 新数据根默认 mixed 端口。
pub(crate) const DEFAULT_MIXED_PORT: u16 = 17899;
/// 新数据根默认 Clash API 端口。
pub(crate) const DEFAULT_CLASH_API_PORT: u16 = 19090;

/// 启动/停止在途标记（`status` 在 lifecycle 锁被占时据此如实上报，不伪造）。
const PHASE_IDLE: u8 = 0;
const PHASE_STARTING: u8 = 1;
const PHASE_STOPPING: u8 = 2;

/// 等待 mixed 端口真实监听的上限（核心冷启动余量充足）。
const LISTENER_TIMEOUT: Duration = Duration::from_secs(15);
/// 等待 Clash API 就绪的上限。
const CLASH_API_TIMEOUT: Duration = Duration::from_secs(15);

/// 按数据根共享的运行时缓存：根句柄全部释放后 Weak 条目自然失效。
static RUNTIMES: OnceLock<Mutex<HashMap<PathBuf, Weak<CoreRuntime>>>> = OnceLock::new();

fn runtime_cache() -> &'static Mutex<HashMap<PathBuf, Weak<CoreRuntime>>> {
    RUNTIMES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 核心运行时句柄：按根共享，克隆廉价。
pub(crate) struct CoreRuntime {
    root: Arc<DataRoot>,
    inventory: CoreInventory,
    composer: ConfigComposer,
    /// 持有中的核心进程管理器（运行中，或失败回滚后待重试回收）。
    lifecycle: tokio::sync::Mutex<Option<ActiveManager>>,
    phase: AtomicU8,
    stage: Mutex<Option<RuntimeStage>>,
    active: Mutex<Option<ActiveInfo>>,
    unexpected_exit: Mutex<Option<CoreExit>>,
    listener_timeout: Duration,
    api_timeout: Duration,
    /// 系统代理守护日志 owner：本数据根 UUID（跨进程精确身份）。
    proxy_owner: String,
    /// 守护装配与只读查询（Windows 真实守护；测试注入 fixture；非 Windows
    /// 显式不支持）。Mutex 只保护 Arc 交换，任何守护 RPC 都不持此锁。
    proxy_spawner: Arc<Mutex<Arc<dyn GuardianSpawner>>>,
    /// 已装配守护（接管失败也保留，直到停止/显式恢复确认）。
    proxy_guardian: Mutex<Option<Box<dyn ArmedGuardian>>>,
    /// 一次性显式接管授权（仅进程内，绝不持久化）：仅用户在确认框确认
    /// 后置位；启动 acquire 时读取并消费（成功或失败都消费），普通
    /// Acquire 仍 RequireDirect，消费后仅当次 acquire 用 ExplicitTakeover。
    proxy_takeover_consent: AtomicBool,
    /// 连接实时追踪句柄（核心运行且启用 Clash API 时拉起，停止/退出时回收）。
    tracker: tokio::sync::Mutex<Option<lanpath_client::ConnectionTrackerHandle>>,
    /// 本地 SQLite 流量统计库（惰性打开 `<data_root>/runtime/db/stats.db`，不纳入备份）。
    stats: tokio::sync::OnceCell<Arc<lanpath_client::StatsStore>>,
    /// 测试注入的校验超时（`None` 用校验器默认 20 秒）。
    #[cfg(test)]
    check_timeout: Option<Duration>,
}

impl CoreRuntime {
    /// 解析本进程数据根并打开运行时（应用启动路径）。
    pub(crate) fn open() -> LanpathResult<Arc<Self>> {
        let location = resolve_data_location()?;
        Self::for_root(&location.path)
    }

    /// 指定数据根打开运行时（测试与显式调用路径，同样走完整准入）。
    pub(crate) fn for_root(path: &Path) -> LanpathResult<Arc<Self>> {
        let root = open_root(path)?;
        let key = root.path().to_path_buf();
        let mut runtimes = runtime_cache()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(runtime) = runtimes.get(&key).and_then(Weak::upgrade) {
            return Ok(runtime);
        }
        let runtime = Arc::new(Self::build(root)?);
        runtimes.insert(key, Arc::downgrade(&runtime));
        Ok(runtime)
    }

    /// 测试注入更短的就绪/校验超时（失败回滚用例不真的等 15/20 秒）。
    #[cfg(test)]
    pub(crate) fn for_root_with_timeouts(
        path: &Path,
        listener: Duration,
        api: Duration,
        check: Duration,
    ) -> LanpathResult<Arc<Self>> {
        let root = open_root(path)?;
        let key = root.path().to_path_buf();
        let mut runtimes = runtime_cache()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(runtime) = runtimes.get(&key).and_then(Weak::upgrade) {
            return Ok(runtime);
        }
        let mut runtime = Self::build(root)?;
        runtime.listener_timeout = listener;
        runtime.api_timeout = api;
        runtime.check_timeout = Some(check);
        let runtime = Arc::new(runtime);
        runtimes.insert(key, Arc::downgrade(&runtime));
        Ok(runtime)
    }

    /// 只查找已缓存的运行时（库存 guard 用；不存在即本进程无运行时，未运行）。
    pub(crate) fn cached(root: &Path) -> Option<Arc<Self>> {
        runtime_cache()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(root)
            .and_then(Weak::upgrade)
    }

    fn build(root: Arc<DataRoot>) -> LanpathResult<Self> {
        let inventory = CoreInventory::for_root(root.path())?;
        let composer = ConfigComposer::for_root(root.path())?;
        let proxy_owner = root.id().to_string();
        let proxy_spawner = crate::system_proxy::default_spawner(&proxy_owner)?;
        Ok(Self {
            root,
            inventory,
            composer,
            lifecycle: tokio::sync::Mutex::new(None),
            phase: AtomicU8::new(PHASE_IDLE),
            stage: Mutex::new(None),
            active: Mutex::new(None),
            unexpected_exit: Mutex::new(None),
            listener_timeout: LISTENER_TIMEOUT,
            api_timeout: CLASH_API_TIMEOUT,
            proxy_owner,
            proxy_spawner,
            proxy_guardian: Mutex::new(None),
            proxy_takeover_consent: AtomicBool::new(false),
            tracker: tokio::sync::Mutex::new(None),
            stats: tokio::sync::OnceCell::new(),
            #[cfg(test)]
            check_timeout: None,
        })
    }

    fn set_stage(&self, stage: RuntimeStage) {
        if let Ok(mut slot) = self.stage.lock() {
            *slot = Some(stage);
        }
    }

    fn clear_stage(&self) {
        if let Ok(mut slot) = self.stage.lock() {
            *slot = None;
        }
    }

    /// 库存 guard 真实检查：本运行时持有的核心进程是否仍在（OS 身份核验）。
    pub(crate) fn is_running_sync(&self) -> bool {
        let identity = self
            .active
            .lock()
            .ok()
            .and_then(|active| active.as_ref().map(|info| info.identity.clone()));
        let Some(identity) = identity else {
            return false;
        };
        ProcessIdentity::capture(identity.pid).is_ok_and(|actual| actual == identity)
    }

    /// 启动：校验并启动用户态核心（二进制校验 + 真实进程 + 真实监听）。
    /// 实现整体在独立任务中跑到完成：UI 离页取消只是看不到结果，不会留下
    /// 半启动子进程或未完成的守护接管。
    pub(crate) async fn start(self: &Arc<Self>) -> LanpathResult<StartOutcome> {
        tracing::info!(target: "lanpath::core_runtime", "开始核心启动任务");
        let this = self.clone();
        tokio::spawn(async move { this.start_impl().await })
            .await
            .map_err(|error| LanpathError::Core(format!("启动任务异常中断:{error}")))?
    }

    async fn start_impl(self: &Arc<Self>) -> LanpathResult<StartOutcome> {
        // 活动读保护覆盖整个启动：备份恢复准备（写保护）会等启动真正结束，
        // 绝不只凭页面 busy；恢复已准备（待重启）时直接拒绝。内部配置合成
        // 走无保护的 inner 变体（读保护已由本路径持有）。
        let _activity = self.root.activity().read().await;
        self.root.ensure_writable()?;
        // 与库存 mutation 互斥：启动全程持有，在途下载/选择/删除立即 busy。
        let _mutation = self.inventory.try_mutation()?;
        let mut slot = self.lifecycle.lock().await;
        if let Some(manager) = slot.as_ref()
            && manager.is_running().await
        {
            return Err(LanpathError::Client("核心已运行；请先停止后再启动".into()));
        }
        // 上次失败回滚保留的待回收管理器：启动前先重试回收（可重试语义）。
        // stop_locked 会先恢复系统代理（守护 shutdown + owner 精确恢复）。
        if slot.is_some() {
            self.stop_locked(&mut slot).await.map_err(|error| {
                LanpathError::Core(format!("上次启动残留回收失败：{error}；请先停止重试"))
            })?;
        }
        // 待回收清理后处理 abandoned journal：同一独立守护完成恢复，
        // 其仍持有本实例 owner 时拒绝启动（需要先停止/显式恢复）。
        self.recover_proxy_before_start()?;
        self.phase.store(PHASE_STARTING, Ordering::Release);
        let result = self.start_locked(&mut slot).await;
        self.phase.store(PHASE_IDLE, Ordering::Release);
        self.clear_stage();
        if result.is_ok()
            && let Ok(mut exit) = self.unexpected_exit.lock()
        {
            *exit = None;
        }
        result
    }

    async fn start_locked(
        &self,
        slot: &mut tokio::sync::MutexGuard<'_, Option<ActiveManager>>,
    ) -> LanpathResult<StartOutcome> {
        self.set_stage(RuntimeStage::ReadSettings);
        let config = {
            let _guard = self.root.business_guard();
            load_request_config(self.root.path())?
        };
        if config.tun_enabled {
            return Err(LanpathError::Client(
                "本切片未授权 TUN；设置已保留，请关闭 TUN 后启动（不接管 TUN/VPN）".into(),
            ));
        }
        self.set_stage(RuntimeStage::ValidateBinary);
        let core = self.inventory.active_core()?.ok_or_else(|| {
            LanpathError::Client("尚未选择核心；请先在核心页下载并选择 sing-box 或 Mihomo".into())
        })?;
        let (core_type, binary, version) = (
            core.core_type(),
            core.path().to_path_buf(),
            core.version().to_owned(),
        );
        CoreManagementService::validate_managed_binary(self.root.path(), core_type, &binary)?;

        self.set_stage(RuntimeStage::PreflightPorts);
        let mut ports = vec![(config.mixed_port, "Mixed 代理")];
        if config.clash_api_enabled {
            ports.push((config.clash_api_port, "Clash API"));
        }
        port_guard::ensure_ports_available(&ports)?;

        self.set_stage(RuntimeStage::ComposeConfig);
        let preview = self.composer.compose_with_config(&config).await?;
        let value = parse_preview(&preview)?;

        self.set_stage(RuntimeStage::ValidateConfig);
        let bindings = ValidatorBindings {
            mixed_port: config.mixed_port,
            clash_api_enabled: config.clash_api_enabled,
            clash_api_port: config.clash_api_port,
            clash_api_secret: config.clash_api_secret.clone(),
        };
        let validator = ConfigValidator::new(self.root.path(), &binary, core_type, bindings);
        #[cfg(test)]
        let validator = match self.check_timeout {
            Some(timeout) => validator.with_check_timeout(timeout),
            None => validator,
        };
        validator
            .validate_value(&value)
            .await
            .map_err(|error| LanpathError::Core(format!("核心配置校验失败（未启动）：{error}")))?;

        self.set_stage(RuntimeStage::StartProcess);
        let manager = ActiveManager::new(core_type, &binary, self.root.path())?;
        // spawn 后任何失败都走同一回滚，不得提前返回留半截清理。
        if let Err(error) = manager.start(&value).await {
            return Err(self.rollback(slot, manager, error).await);
        }

        self.set_stage(RuntimeStage::WaitReady);
        if let Err(error) = self.wait_ready(&manager, &config).await {
            return Err(self.rollback(slot, manager, error).await);
        }

        let identity = match read_pid_identity(self.root.path(), core_type) {
            Ok(identity) => identity,
            Err(error) => return Err(self.rollback(slot, manager, error).await),
        };
        let clash_api_port = config.clash_api_enabled.then_some(config.clash_api_port);
        if let Ok(mut active) = self.active.lock() {
            *active = Some(ActiveInfo {
                identity,
                core_type,
                version: version.clone(),
                mixed_port: config.mixed_port,
                clash_api_port,
                started: Instant::now(),
                config: config.clone(),
                missing_rule_sets: preview.missing_tags().to_vec(),
            });
        }
        **slot = Some(manager);
        self.start_tracker(&config).await;
        // 系统代理接管：核心校验/mixed/API 全部就绪后 arm 独立守护并
        // acquire 127.0.0.1:mixed_port。接管失败保留核心与守护证据——
        // 绝不能先 rollback 清掉恢复资源；用户经“停止/显式恢复”完成恢复。
        if let Err(error) = self.acquire_proxy(config.system_proxy_enabled, config.mixed_port) {
            tracing::error!(target: "lanpath::core_runtime",
                "系统代理接管失败；核心与恢复资源保留：{error}");
            return Err(error);
        }
        tracing::info!(target: "lanpath::core_runtime",
            "核心已启动：{core_type} {version} mixed {} api {:?}（已完成二进制校验）",
            config.mixed_port, clash_api_port);
        Ok(StartOutcome {
            core_type,
            version,
            mixed_port: config.mixed_port,
            clash_api_port,
            node_count: preview.node_count(),
            warnings: preview.warnings().to_vec(),
            missing_tags: preview.missing_tags().to_vec(),
        })
    }
}

#[cfg(test)]
#[path = "core_runtime_tests.rs"]
mod tests;
