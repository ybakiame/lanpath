//! Client runtime orchestration.
//!
//! [`ClientState`] orchestrates the overall client lifecycle:
//!
//! **Start**: Pull subscription → (optional) start MITM (get listen address) → compose core config
//! (inject MITM routing rules and return inbound) → start core → enable system proxy (pointing to core
//! mixed main entry). Roll back completed steps on any failure.
//!
//! **Stop**: Shut down in reverse order of startup (best-effort, single-item failure does not affect the rest).

use std::path::Path;
use std::sync::Arc;

use lanpath_common::{LanpathError, LanpathResult};
#[cfg(feature = "mitm")]
use lanpath_mitm::{MemoryRecorder, RunningProxy};
use lanpath_script::{Notifier, ScriptScheduler};
use tokio::task::JoinHandle;

use crate::config::ClientConfig;
use crate::core_config;
use crate::privilege::{TunAuthStatus, tun_auth_status};
use crate::profile;
use crate::remote::TracingNotifier;
use crate::runner::CoreRunner;
use crate::subscription;
use crate::sysproxy::{PlatformSystemProxy, SystemProxy};

mod clash_setup;
mod compat;
mod connection_tracker;
#[cfg(feature = "mitm")]
mod mitm;
mod scheduler;
mod services;
mod start;
#[cfg(all(test, unix, feature = "mitm"))]
mod tests;
mod types;

pub use types::ClientStatus;

/// Scheduled task scheduler running handle.
struct SchedulerHandle {
    scheduler: Arc<ScriptScheduler>,
    handle: JoinHandle<()>,
}

/// Connection tracker running handle.
struct ConnectionTrackerHandle {
    tracker: crate::connections::ConnectionTrackerHandle,
}

/// Client runtime orchestrator.
pub struct ClientState {
    /// Client configuration.
    pub config: ClientConfig,
    core: Option<CoreRunner>,
    #[cfg(feature = "mitm")]
    mitm: Option<RunningProxy>,
    sysproxy: Arc<dyn SystemProxy>,
    /// Script notifier (`$notify` / `$notification`); default [`TracingNotifier`] (logs only).
    notifier: Arc<dyn Notifier>,
    /// Packet capture recorder (in-memory ring buffer, capacity 2048; injected when MITM proxy starts).
    #[cfg(feature = "mitm")]
    recorder: Arc<MemoryRecorder>,
    /// Remote subscription task script scheduler (starts after MITM ready, stops on stop).
    scheduler: Option<SchedulerHandle>,
    /// Background connection tracker (started after core startup when Clash API is enabled, stopped on stop).
    connection_tracker: Option<ConnectionTrackerHandle>,
    /// 流量统计存储（SQLite；由壳层注入，未注入时 tracker 仅维护内存环形缓冲）。
    stats_store: Option<Arc<crate::stats::StatsStore>>,
    /// TUN privilege detection function (default [`tun_auth_status`]; tests can inject overrides to bypass real privilege checks).
    ///
    /// Only read on desktop builds (TUN pre-start privilege check gated by `#[cfg(not(target_os = "android"))]`
    ///; Android authorization goes through VpnService system dialog), field is still kept on Android builds
    /// (for desktop test compilation and unified construction path) and allows dead_code.
    #[cfg_attr(target_os = "android", allow(dead_code))]
    tun_auth_check: Arc<dyn Fn(&Path) -> TunAuthStatus + Send + Sync>,
    /// 端口占用探测函数（ADR-0012 D4；默认 [`crate::port_guard::ensure_ports_available`]）。
    ///
    /// 测试可注入覆盖：集成测试的 mock Clash API 服务器本身就占用
    /// clash_api_port（模拟核心的 API 端），需绕过真实探测。
    port_probe: crate::port_guard::PortProbe,
    /// Number of rules in the composed config (written after successful start, cleared on stop; returned by status()).
    rule_count: u64,
    /// 本次启动未能本地化的内置 CN 规则集 tag（降级启动；空 = 完整分流）。后台重试补齐
    /// 后由命令层重载核心恢复完整配置（见 `ruleset_manager` 模块文档）。
    missing_rule_sets: Vec<String>,
}

impl ClientState {
    /// Create state machine using platform system proxy implementation.
    pub fn new(config: ClientConfig) -> Self {
        Self::with_dependencies(
            config,
            Arc::new(PlatformSystemProxy::default()),
            Arc::new(TracingNotifier::new()),
        )
    }

    /// Create state machine with custom system proxy implementation (for test injection).
    pub fn with_system_proxy(config: ClientConfig, sysproxy: Arc<dyn SystemProxy>) -> Self {
        Self::with_dependencies(config, sysproxy, Arc::new(TracingNotifier::new()))
    }

    /// Create state machine with custom notifier (for OS desktop notification integration).
    pub fn with_notifier(config: ClientConfig, notifier: Arc<dyn Notifier>) -> Self {
        Self::with_dependencies(config, Arc::new(PlatformSystemProxy::default()), notifier)
    }

    /// Combined dependency constructor for the state machine.
    fn with_dependencies(
        config: ClientConfig,
        sysproxy: Arc<dyn SystemProxy>,
        notifier: Arc<dyn Notifier>,
    ) -> Self {
        Self {
            config,
            core: None,
            #[cfg(feature = "mitm")]
            mitm: None,
            sysproxy,
            notifier,
            #[cfg(feature = "mitm")]
            recorder: Arc::new(MemoryRecorder::new(2048)),
            scheduler: None,
            connection_tracker: None,
            stats_store: None,
            tun_auth_check: Arc::new(tun_auth_status),
            port_probe: Arc::new(crate::port_guard::ensure_ports_available),
            rule_count: 0,
            missing_rule_sets: Vec::new(),
        }
    }

    /// Packet capture recorder (in-memory ring buffer, capacity 2048).
    #[cfg(feature = "mitm")]
    pub fn recorder(&self) -> Arc<MemoryRecorder> {
        Arc::clone(&self.recorder)
    }

    /// 注入流量统计存储（壳层在 `start` 前调用；未注入时连接统计仅保留内存态）。
    pub fn set_stats_store(&mut self, store: Arc<crate::stats::StatsStore>) {
        self.stats_store = Some(store);
    }

    /// 注入端口占用探测函数（测试用；生产默认 [`crate::port_guard::ensure_ports_available`]）。
    pub fn set_port_probe(
        &mut self,
        probe: impl Fn(&[(u16, &str)]) -> LanpathResult<()> + Send + Sync + 'static,
    ) {
        self.port_probe = Arc::new(probe);
    }

    /// Scheduled task scheduler (remote subscription task scripts); `None` when not started or no tasks.
    /// After phase ③ decoupling: no longer depends on MITM, reads task scripts from remote cache independently.
    pub fn scheduler(&self) -> Option<&ScriptScheduler> {
        self.scheduler.as_ref().map(|h| h.scheduler.as_ref())
    }

    /// `Arc` handle of the scheduled task scheduler (for external driving in a separate thread; `None` when not ready).
    pub fn scheduler_handle(&self) -> Option<Arc<ScriptScheduler>> {
        self.scheduler.as_ref().map(|h| Arc::clone(&h.scheduler))
    }

    /// Start: subscription selection (active_subscription_id) → override parsing (subscription-linked template) → MITM →
    /// compose config (MITM chain) → core → system proxy, rollback on failure.
    ///
    /// Runtime model (pure association): the subscription selected on the home page (`active_subscription_id`) is the only one effective,
    /// subscription `enabled` only means "can be selected on home page"; the override used at runtime = the override template associated with the currently effective subscription
    /// (no association = no override). Any exception in subscription selection or template association returns a clear error without starting.
    /// When no subscription is selected, start fails with a clear error.
    ///
    /// Startup order: first pull subscription and generate base config through Profile layer (subscription only takes nodes → local template →
    /// remote YAML → local YAML → remote JS → local JS overlay override, remote URL fetch failure falls back to cache, no cache skips with warning),
    /// when MITM is enabled, start MITM (get listen address), inject [`MitmChain`] (routing rules + return inbound) during config composition,
    /// then start core, finally enable system proxy (always pointing to core mixed main entry, MITM hangs after core and is distributed by core routing rules).
    /// Profile build failure occurs before any component starts, no rollback needed.
    pub async fn start(&mut self) -> LanpathResult<()> {
        self.start_with_progress(&|_| {}).await
    }

    /// 停止：按启动逆序逐项关闭（best-effort）。
    pub async fn stop(&mut self) {
        let _ = self.sysproxy.disable().await;
        self.stop_mitm().await;
        self.stop_scheduler().await;
        self.stop_connection_tracker().await;
        self.stop_core().await;
        // 未运行时规则条数清零。
        self.rule_count = 0;
        self.missing_rule_sets = Vec::new();
    }

    /// 当前运行状态。
    pub async fn status(&self) -> ClientStatus {
        let core_running = match &self.core {
            Some(c) => c.is_running().await,
            None => false,
        };
        // 核心运行中且 Clash 面板 API 开启时才暴露其地址。
        let clash_api_url = if core_running && self.config.clash_api_enabled {
            Some(format!("http://127.0.0.1:{}", self.config.clash_api_port))
        } else {
            None
        };
        #[cfg(feature = "mitm")]
        let mitm_addr = self.mitm.as_ref().map(|m| m.addr);
        #[cfg(not(feature = "mitm"))]
        let mitm_addr = None;
        ClientStatus {
            core_running,
            mitm_addr,
            system_proxy: self.sysproxy.is_enabled().await,
            rule_mode: self.config.normalized_rule_mode().to_string(),
            rule_count: self.rule_count,
            clash_api_url,
            missing_rule_sets: self.missing_rule_sets.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Non-MITM fallback (scripts-only / mobile shape)
// ---------------------------------------------------------------------------

/// MITM 相关方法在 `mitm` feature 关闭（mobile 形态）时的空实现：不启动 MITM，
/// 链路始终为 `None`，回滚仅关停调度器（与 mitm 实现的停止序列保持一致）。
#[cfg(not(feature = "mitm"))]
impl ClientState {
    pub(crate) async fn start_mitm_chain(
        &mut self,
    ) -> LanpathResult<Option<core_config::MitmChain>> {
        Ok(None)
    }

    pub(crate) async fn rollback_mitm_started(&mut self) {
        self.stop_scheduler().await;
    }

    pub(crate) async fn stop_mitm(&mut self) {}
}
