//! 核心运行时：真实核心二进制的用户态启停（本切片不接管系统代理/TUN）。
//!
//! `CoreRuntime` 按数据根共享：Weak 缓存 + App 启动时 `stored` 持有——同一
//! 数据根的所有页面/句柄看到同一份真实运行状态；GUI 正常退出时随 `Drop`
//! 按记录的 OS 进程身份回收本次子进程（另有 spawn 时的 OS 父子绑定兜底），
//! 绝不按进程名强杀。
//!
//! 启动固定同一份 `ClientConfig` 快照：拒绝 `tun_enabled`/`system_proxy_enabled`
//! （本切片无授权）；confined 库存核心 + `validate_managed_binary`；预检
//! mixed/API 端口 loopback 可绑定；`compose_with_config(快照)` 一次合成；
//! `ConfigValidator` 校验后才 `start` 并等待真实监听/API 就绪。spawn 后
//! 任何失败（start 失败、就绪超时、pid 身份读取失败）都走同一 `rollback`
//! 显式 `stop_process` 回收（manager drop 在 Unix 只 detach，不得提前返回
//! 留半截清理）；回滚失败同时报告原错误与回收错误并保留可重试状态（再次
//! 启动/停止会先重试回收）。
//!
//! 与库存的互斥：启动/停止持有 `CoreInventory` 同根 mutation 锁（在途下载/
//! 选择/删除立即 busy）；运行中（含停止进行中 running 未清）库存 mutation 由
//! `is_running_sync` 真实核验拒绝。取锁顺序恒为 mutation → lifecycle →
//! business/active（短临界区），无反向路径，不会死锁。
//!
//! 日志只记操作、端口、版本与阶段，不记配置正文、节点/代理凭据或完整 URL。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use lanpath_client::core_management::CoreManagementService;
use lanpath_client::{ClientConfig, port_guard};
use lanpath_common::{LanpathError, LanpathResult, ProcessIdentity, ProcessStopKind};

use crate::config_composer::ConfigComposer;
use crate::config_validator::{ConfigValidator, ValidatorBindings};
use crate::core_inventory::CoreInventory;
use crate::data_root::{DataRoot, open_root, resolve_data_location};
use crate::request_settings::load_request_config;

mod manager;
mod types;

use manager::{ActiveManager, parse_preview, read_pid_identity, wait_listener};
use types::ActiveInfo;
pub(crate) use types::{CoreStatus, SessionSummary, StartOutcome};

/// 新数据根默认 mixed 端口（GPUI 实验值；避免与生产 17890 冲突）。
pub(crate) const DEFAULT_MIXED_PORT: u16 = 17899;
/// 新数据根默认 Clash API 端口（GPUI 实验值；避免与生产 9090 冲突）。
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
    stage: Mutex<Option<String>>,
    active: Mutex<Option<ActiveInfo>>,
    unexpected_exit: Mutex<Option<String>>,
    listener_timeout: Duration,
    api_timeout: Duration,
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
            #[cfg(test)]
            check_timeout: None,
        })
    }

    fn set_stage(&self, stage: &'static str) {
        if let Ok(mut slot) = self.stage.lock() {
            *slot = Some(stage.to_owned());
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
    pub(crate) async fn start(self: &Arc<Self>) -> LanpathResult<StartOutcome> {
        tracing::info!(target: "lanpath_desktop_nsh::core_runtime", "开始核心启动任务");
        let this = self.clone();
        tokio::spawn(async move { this.start_impl().await })
            .await
            .map_err(|error| LanpathError::Core(format!("启动任务异常中断:{error}")))?
    }

    async fn start_impl(self: &Arc<Self>) -> LanpathResult<StartOutcome> {
        // 与库存 mutation 互斥：启动全程持有，在途下载/选择/删除立即 busy。
        let _mutation = self.inventory.try_mutation()?;
        let mut slot = self.lifecycle.lock().await;
        if let Some(manager) = slot.as_ref()
            && manager.is_running().await
        {
            return Err(LanpathError::Client("核心已运行；请先停止后再启动".into()));
        }
        // 上次失败回滚保留的待回收管理器：启动前先重试回收（可重试语义）。
        if slot.is_some() {
            self.stop_locked(&mut slot).await.map_err(|error| {
                LanpathError::Core(format!("上次启动残留回收失败：{error}；请先停止重试"))
            })?;
        }
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
        self.set_stage("读取已保存设置");
        let config = {
            let _guard = self.root.business_guard();
            load_request_config(self.root.path())?
        };
        if config.tun_enabled {
            return Err(LanpathError::Client(
                "本切片未授权 TUN；设置已保留，请关闭 TUN 后启动（不接管系统代理/TUN）".into(),
            ));
        }
        if config.system_proxy_enabled {
            return Err(LanpathError::Client(
                "本切片未授权系统代理；设置已保留，请关闭系统代理后启动".into(),
            ));
        }
        self.set_stage("校验核心二进制");
        let core = self.inventory.active_core()?.ok_or_else(|| {
            LanpathError::Client("尚未选择核心；请先在核心页下载并选择 sing-box 或 Mihomo".into())
        })?;
        let (core_type, binary, version) = (
            core.core_type(),
            core.path().to_path_buf(),
            core.version().to_owned(),
        );
        CoreManagementService::validate_managed_binary(self.root.path(), core_type, &binary)?;

        self.set_stage("预检监听端口");
        let mut ports = vec![(config.mixed_port, "Mixed 代理")];
        if config.clash_api_enabled {
            ports.push((config.clash_api_port, "Clash API"));
        }
        port_guard::ensure_ports_available(&ports)?;

        self.set_stage("合成配置");
        let preview = self.composer.compose_with_config(&config).await?;
        let value = parse_preview(&preview)?;

        self.set_stage("校验配置（真实核心二进制）");
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

        self.set_stage("启动核心进程");
        let manager = ActiveManager::new(core_type, &binary, self.root.path())?;
        // spawn 后任何失败都走同一回滚，不得提前返回留半截清理。
        if let Err(error) = manager.start(&value).await {
            return Err(self.rollback(slot, manager, error).await);
        }

        self.set_stage("等待核心就绪");
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
            });
        }
        **slot = Some(manager);
        tracing::info!(target: "lanpath_desktop_nsh::core_runtime",
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

    /// 等待 mixed 端口真实监听；Clash API 启用时等待其就绪。失败由调用方回滚。
    async fn wait_ready(
        &self,
        manager: &ActiveManager,
        config: &ClientConfig,
    ) -> LanpathResult<()> {
        wait_listener(manager, config.mixed_port, self.listener_timeout).await?;
        if config.clash_api_enabled {
            lanpath_client::core_config::wait_clash_api_ready(
                config.clash_api_port,
                &config.clash_api_secret,
                self.api_timeout,
            )
            .await?;
        }
        Ok(())
    }

    /// 启动失败回滚：回收已 spawn 的子进程。回收失败时同时报告原错误与回收
    /// 错误，manager 保留在运行时里（再次启动/停止先重试回收）。
    async fn rollback(
        &self,
        slot: &mut tokio::sync::MutexGuard<'_, Option<ActiveManager>>,
        manager: ActiveManager,
        original: LanpathError,
    ) -> LanpathError {
        match manager.stop_process().await {
            Ok(_) => original,
            Err(recovery) => {
                tracing::error!(target: "lanpath_desktop_nsh::core_runtime",
                    "启动失败且回收未完成：{original}；回收错误：{recovery}");
                **slot = Some(manager);
                LanpathError::Core(format!(
                    "{original}；启动回收未完成：{recovery}（可再次停止或启动重试回收）"
                ))
            }
        }
    }

    /// 停止：回收核心子进程并清理本次会话状态；同 `start` 在独立任务中
    /// 跑到完成（UI 离页不会让停止半途而废）。
    pub(crate) async fn stop(self: &Arc<Self>) -> LanpathResult<ProcessStopKind> {
        let this = self.clone();
        tokio::spawn(async move { this.stop_impl().await })
            .await
            .map_err(|error| LanpathError::Core(format!("停止任务异常中断:{error}")))?
    }

    async fn stop_impl(self: &Arc<Self>) -> LanpathResult<ProcessStopKind> {
        let _mutation = self.inventory.try_mutation()?;
        let mut slot = self.lifecycle.lock().await;
        self.phase.store(PHASE_STOPPING, Ordering::Release);
        self.set_stage("停止核心进程");
        let result = self.stop_locked(&mut slot).await;
        self.phase.store(PHASE_IDLE, Ordering::Release);
        self.clear_stage();
        if let Ok(kind) = result {
            tracing::info!(target: "lanpath_desktop_nsh::core_runtime", "核心已停止：{kind:?}");
        }
        result
    }

    async fn stop_locked(
        &self,
        slot: &mut tokio::sync::MutexGuard<'_, Option<ActiveManager>>,
    ) -> LanpathResult<ProcessStopKind> {
        let kind = match slot.as_ref() {
            Some(manager) => manager.stop_process().await?,
            None => ProcessStopKind::AlreadyStopped,
        };
        // 清理失败（Err 提前返回）时 manager 保留：running 未清，库存仍拒绝，
        // 且可再次停止重试。
        **slot = None;
        if let Ok(mut active) = self.active.lock() {
            *active = None;
        }
        if let Ok(mut exit) = self.unexpected_exit.lock() {
            *exit = None;
        }
        Ok(kind)
    }

    /// 真实状态 poll：每次调用都核验核心进程活性；异常退出在此被发现并
    /// 记录（不依赖按钮状态）。
    pub(crate) async fn status(&self) -> CoreStatus {
        let operation = match self.phase.load(Ordering::Acquire) {
            PHASE_STARTING => Some("正在启动"),
            PHASE_STOPPING => Some("正在停止"),
            _ => None,
        };
        let stage = self.stage.lock().ok().and_then(|slot| slot.clone());
        let Ok(mut slot) = self.lifecycle.try_lock() else {
            let session = self.session_snapshot();
            return CoreStatus {
                operation,
                stage,
                running: session.is_some(),
                session,
                unexpected_exit: self.exit_snapshot(),
            };
        };
        if let Some(manager) = slot.as_ref()
            && !manager.is_running().await
        {
            let detail = manager.last_error().await;
            let message = if detail.is_empty() {
                "核心进程已退出".to_owned()
            } else {
                format!("核心进程异常退出：{detail}")
            };
            if let Ok(mut exit) = self.unexpected_exit.lock() {
                *exit = Some(message);
            }
            *slot = None;
            if let Ok(mut active) = self.active.lock() {
                *active = None;
            }
        }
        let session = self.session_snapshot();
        CoreStatus {
            operation,
            stage,
            running: session.is_some(),
            session,
            unexpected_exit: self.exit_snapshot(),
        }
    }

    fn session_snapshot(&self) -> Option<SessionSummary> {
        self.active.lock().ok().and_then(|active| {
            active.as_ref().map(|info| SessionSummary {
                core_type: info.core_type,
                version: info.version.clone(),
                mixed_port: info.mixed_port,
                clash_api_port: info.clash_api_port,
                uptime_secs: info.started.elapsed().as_secs(),
            })
        })
    }

    fn exit_snapshot(&self) -> Option<String> {
        self.unexpected_exit
            .lock()
            .ok()
            .and_then(|exit| exit.clone())
    }
}

impl Drop for CoreRuntime {
    fn drop(&mut self) {
        // GUI 正常退出：按记录的 OS 进程身份回收本次子进程（spawn 时的
        // 父子绑定是 OS 级兜底；这里做确定性的显式回收，绝不按进程名强杀）。
        let identity = self
            .active
            .get_mut()
            .map(|active| active.take())
            .ok()
            .flatten()
            .map(|info| info.identity);
        if let Some(identity) = identity {
            match identity.terminate_verified() {
                Ok(kind) => {
                    tracing::info!(target: "lanpath_desktop_nsh::core_runtime", "核心随运行时释放回收：{kind:?}")
                }
                Err(error) => {
                    tracing::warn!(target: "lanpath_desktop_nsh::core_runtime",
                        "核心回收未完成（依赖 OS 父子绑定兜底）：{error}")
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "core_runtime_tests.rs"]
mod tests;
