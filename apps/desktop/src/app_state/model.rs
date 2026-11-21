//! App 级共享状态模型：运行快照、命令状态与摘要错误。
//!
//! 业务层只保存语义值和原始错误；所有用户文案在展示层按当前语言生成，
//! 因此语言切换不会把旧翻译留在长任务、结果或错误里。

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use lanpath_client::SubFormat;
use lanpath_common::{CoreType, ProcessStopKind};
use nsh_core::prelude::*;
use uuid::Uuid;

use crate::core_inventory::CoreInventory;
use crate::core_runtime::{
    CoreExit, CoreOperation, CoreRuntime, RuntimeStage, SessionSummary, StartOutcome,
};

/// 运行状态视图模型（由 `CoreStatus` 转换；App 级共享的真实状态）。
#[derive(Clone, Default, PartialEq)]
pub(crate) struct RuntimeView {
    pub(crate) operation: Option<CoreOperation>,
    pub(crate) stage: Option<RuntimeStage>,
    pub(crate) running: bool,
    pub(crate) session: Option<SessionSummary>,
    pub(crate) exit_error: Option<CoreExit>,
    /// 系统代理守护支持性（Windows 真实守护；非 Windows 如实为 false）。
    pub(crate) proxy_supported: bool,
    /// 系统代理 ownership 探测（守护真实状态；不支持为 `None`）。
    pub(crate) proxy_ownership:
        Option<Result<lanpath_client::sysproxy::SystemProxyOwnership, String>>,
}

impl RuntimeView {
    pub(crate) fn from_status(status: &crate::core_runtime::CoreStatus) -> Self {
        Self {
            operation: status.operation,
            stage: status.stage,
            running: status.running,
            session: status.session.clone(),
            exit_error: status.unexpected_exit.clone(),
            proxy_supported: status.proxy_supported,
            proxy_ownership: status.proxy_ownership.clone(),
        }
    }
}

/// 一次共享命令的类型；阶段文案由展示层按语言翻译。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandKind {
    Start,
    Stop,
    Restart,
    RestoreProxy,
}

/// 命令成功后的语义结果。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CommandResult {
    Started(StartOutcome),
    Stopped(ProcessStopKind),
    ProxyRestored,
}

/// 命令失败的结构化原因；原始业务错误保持原样，前缀由展示层翻译。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommandError {
    RuntimeUnavailable,
    StartFailed(String),
    StopFailed(String),
    RestartStartFailed(String),
    RestoreProxyFailed(String),
    ProxyUnsupported,
    TaskInterrupted,
}

/// 启动/停止/恢复命令状态：App 级单飞，结果与错误只存语义。
#[derive(Clone, Default, PartialEq)]
pub(crate) struct CommandView {
    pub(crate) busy: bool,
    pub(crate) kind: Option<CommandKind>,
    pub(crate) result: Option<CommandResult>,
    pub(crate) error: Option<CommandError>,
}

/// 当前生效订阅摘要（概览与全局状态栏共享；来自共享订阅业务层）。
#[derive(Clone, PartialEq)]
pub(crate) struct SubscriptionSummary {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) node_count: u64,
    pub(crate) format: Option<SubFormat>,
}

/// 实际选中的核心（独立于浏览类型；来自共享核心库存）。
#[derive(Clone, PartialEq)]
pub(crate) struct CoreSelection {
    pub(crate) core_type: CoreType,
    pub(crate) version: String,
}

/// 单秒流量速率采样点（字节/秒）。
#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) struct TrafficPoint {
    pub(crate) up_bps: f64,
    pub(crate) down_bps: f64,
}

/// App 级共享流量与统计快照（供概览图表与连接页复用，切页不丢曲线）。
#[derive(Clone, PartialEq)]
pub(crate) struct TrafficSnapshot {
    pub(crate) up_bps: u64,
    pub(crate) down_bps: u64,
    pub(crate) active_count: usize,
    pub(crate) active_upload_total: u64,
    pub(crate) active_download_total: u64,
    pub(crate) history: Vec<TrafficPoint>,
    pub(crate) today: lanpath_client::TodaySummary,
    pub(crate) recent_days: Vec<lanpath_client::DailyTotalRow>,
}

/// 实时速率图默认保留的采样点数（约 30 秒滚动窗口）。
pub(crate) const TRAFFIC_HISTORY_LEN: usize = 30;

impl Default for TrafficSnapshot {
    fn default() -> Self {
        Self {
            up_bps: 0,
            down_bps: 0,
            active_count: 0,
            active_upload_total: 0,
            active_download_total: 0,
            history: vec![TrafficPoint::default(); TRAFFIC_HISTORY_LEN],
            today: lanpath_client::TodaySummary::default(),
            recent_days: Vec::new(),
        }
    }
}

/// App 级共享状态（`global` 单例，每个引擎一份）。
#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) runtime: Option<Arc<CoreRuntime>>,
    pub(crate) inventory: Option<CoreInventory>,
    pub(crate) subscriptions: Option<Arc<crate::subscriptions::Subscriptions>>,
    pub(crate) status: Signal<RuntimeView>,
    pub(crate) command: Signal<CommandView>,
    /// 最后成功读取的生效订阅；读取失败时保留，不覆盖为“未设置”。
    pub(crate) active_subscription: Signal<Option<SubscriptionSummary>>,
    pub(crate) active_core: Signal<Option<CoreSelection>>,
    pub(crate) subscription_error: Signal<Option<String>>,
    pub(crate) core_error: Signal<Option<String>>,
    /// 已保存的系统代理偏好（最近一次成功读取/保存的值）。
    pub(crate) proxy_preference: Signal<Option<bool>>,
    /// 实时速率、滚动历史曲线与本地 SQLite 统计汇总。
    pub(crate) traffic: Signal<TrafficSnapshot>,
    pub(crate) owner: Option<Owner>,
    /// 轮询/命令任务的存活句柄（替换即释放旧任务）。
    pub(crate) tasks: Rc<RefCell<Vec<Spawned<()>>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            runtime: None,
            inventory: None,
            subscriptions: None,
            status: signal(RuntimeView::default()),
            command: signal(CommandView::default()),
            active_subscription: signal(None),
            active_core: signal(None),
            subscription_error: signal(None),
            core_error: signal(None),
            proxy_preference: signal(None),
            traffic: signal(TrafficSnapshot::default()),
            owner: None,
            tasks: Rc::new(RefCell::new(Vec::new())),
        }
    }
}
