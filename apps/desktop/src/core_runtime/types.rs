//! 核心运行时的状态类型：运行中会话身份/摘要、状态快照、启动回执。

use std::time::Instant;

use lanpath_common::{CoreType, ProcessIdentity};

/// 运行中会话的真实身份信息（active 短临界区读取，绝不跨 await）。
pub(super) struct ActiveInfo {
    pub(super) identity: ProcessIdentity,
    pub(super) core_type: CoreType,
    pub(super) version: String,
    pub(super) mixed_port: u16,
    pub(super) clash_api_port: Option<u16>,
    pub(super) started: Instant,
    pub(super) config: lanpath_client::ClientConfig,
    pub(super) missing_rule_sets: Vec<String>,
}

/// 运行中会话摘要（UI 展示）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionSummary {
    pub(crate) core_type: CoreType,
    pub(crate) version: String,
    pub(crate) mixed_port: u16,
    pub(crate) clash_api_port: Option<u16>,
    pub(crate) uptime_secs: u64,
}

/// 启动/停止在途操作（UI 文案在展示层按语言取值，业务层只给语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoreOperation {
    Starting,
    Stopping,
}

/// 运行时真实阶段（业务层只给语义，语言切换后由展示层重新翻译）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeStage {
    ReadSettings,
    ValidateBinary,
    PreflightPorts,
    ComposeConfig,
    ValidateConfig,
    StartProcess,
    WaitReady,
    RestoreProxyAndStop,
}

/// 核心异常退出事实：退出码是否可知不在这里推断，只保留可展示的证据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CoreExit {
    /// 核心进程已退出，且没有可用的核心输出尾行。
    Exited,
    /// 核心进程异常退出，附带真实核心输出/回收错误。
    Failed(String),
}

/// 运行时真实状态快照（每次 `status` 都真实 poll，不记按钮状态）。
#[derive(Debug, Clone)]
pub(crate) struct CoreStatus {
    /// 启动/停止在途（lifecycle 锁被占时的如实阶段）。
    pub(crate) operation: Option<CoreOperation>,
    pub(crate) stage: Option<RuntimeStage>,
    pub(crate) running: bool,
    pub(crate) session: Option<SessionSummary>,
    /// 核心异常退出事实（下次成功启动或手动停止时清除）。
    pub(crate) unexpected_exit: Option<CoreExit>,
    /// 系统代理守护是否受支持（Windows 真实守护；测试 fixture 可声明）。
    pub(crate) proxy_supported: bool,
    /// 系统代理 ownership 探测（守护真实状态；不支持为 `None`）。
    /// `Err` 如实上报查询失败（界面显示“未知错误”），不伪造成未接管。
    pub(crate) proxy_ownership:
        Option<Result<lanpath_client::sysproxy::SystemProxyOwnership, String>>,
}

/// 启动成功回执（真实校验 + 真实进程 + 真实监听后的摘要）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartOutcome {
    pub(crate) core_type: CoreType,
    pub(crate) version: String,
    pub(crate) mixed_port: u16,
    pub(crate) clash_api_port: Option<u16>,
    pub(crate) node_count: usize,
    pub(crate) warnings: Vec<String>,
    pub(crate) missing_tags: Vec<String>,
}
