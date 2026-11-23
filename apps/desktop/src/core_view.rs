//! 核心页：核心运行（校验并启动/停止，用户态 loopback，不接管 TUN/VPN）
//! 与核心库存/下载/选择（共享 `CoreManagementService::experimental` confined
//! 语义），以及“系统代理”分区（经独立守护的 Windows 精确接管/恢复；
//! 非 Windows 如实显示不支持）。运行状态与启动/停止命令不复用页面私有
//! Signal——与概览/全局状态栏共用 App 级 `app_state`（同一 CoreRuntime
//! 状态源、同一命令单飞、任务跨页面存活）；本页 busy 只约束库存操作
//! （下载/选择/删除），运行卡片只读共享状态、把交互转发给 `app_state`。
//! 系统代理分区显示守护真实 ownership（未接管/本应用持有/需要恢复/未知
//! 错误），开关只保存偏好、运行中切换走安全启停（共享命令），提供仅本
//! 应用 owner 的显式恢复。
//!
//! 库存 busy 全局单飞（在途操作防重复，完成后恢复）；操作失败保留旧列表
//! 并显示真实错误；下载进度只显示共享真实阶段，不伪造 byte 百分比。库存
//! 操作在 `background` 执行，Signal 只在 UI 线程的 `spawn` 任务内回写；
//! 页面销毁只取消 UI 观察，业务 worker 仍可能完成，因此真正的在途状态和
//! 完成版本由 `CoreInventory` 按数据根共享：新页面恢复真实 busy，App 级
//! 摘要在版本变化后重读，不依赖旧页面回调，也不会永久 busy。
//! mutation 的跨页面互斥与运行中拒绝在业务层（同根共享 try_acquire 锁 +
//! `CoreRuntime::is_running_sync` 真实核验），本页 busy 只约束本页按钮。

mod activity;
mod proxy_card;
mod render;
mod runtime;
mod runtime_card;

use activity::{activity_stage, read_activity_initial};

use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lanpath_client::cores::{CoreChannel, CoreDownloadStage, channel_of_version};
use lanpath_common::CoreType;
use nsh_core::prelude::*;

use crate::app_state;
use crate::core_inventory::{CoreInventory, LocalCoreRow};
use crate::core_runtime::CoreRuntime;

pub(crate) use crate::app_state::core_tag;
pub(crate) use render::core_page;

const TYPES: [(CoreType, &str); 2] = [
    (CoreType::SingBox, "sing-box"),
    (CoreType::Mihomo, "Mihomo"),
];
const CHANNELS: [(CoreChannel, &str); 2] = [
    (CoreChannel::Stable, "稳定版"),
    (CoreChannel::Prerelease, "预发布版"),
];
/// 阶段轮询间隔：共享层真实阶段回调写进槽位，UI 任务按此节奏回写 Signal
///（Signal 只属于 owner 线程，worker 线程绝不直接写）。
const STAGE_POLL: Duration = Duration::from_millis(120);

/// 一次后台工作的共享槽位：worker（background 线程）写入，UI 任务轮询读出。
#[derive(Default)]
struct WorkSlot {
    stage: String,
    finished: bool,
    outcome: Option<Result<WorkOutcome, String>>,
}

enum WorkOutcome {
    Remote(Vec<RemoteRow>),
    Changed,
}

/// 远端通道行（仅 stable/prerelease）。
#[derive(Clone, PartialEq, Eq)]
struct RemoteRow {
    channel: &'static str,
    version: String,
}

/// 页状态：全部为句柄（Signal/Stored），克隆廉价。运行状态与命令结果
/// 不复用页面私有 Signal——与概览/状态栏共用 App 级 `app_state` 的同一
/// `CoreRuntime` 状态源（见 `crate::app_state`）；本页 busy 只约束库存
/// 操作（下载/选择/删除），运行命令的 busy 由共享命令状态单飞。
#[derive(Clone)]
struct CorePageState {
    inventory: Option<CoreInventory>,
    runtime: Option<Arc<CoreRuntime>>,
    /// 页面级 owner：在页面构建时捕获。spawn 的 UI 轮询任务挂在它下面——
    /// 弹窗/行的临时 owner 在按下后被销毁（如确认删除先关弹窗），在其下
    /// spawn 会被立即取消；离开页面销毁本页 UI 观察。业务 worker 仍由共享
    /// 库存活动登记，完成后新页面/App 摘要通过版本变化重读。
    owner: Owner,
    /// 正在浏览的库存类型（active 类型独立显示，不随浏览切换）。
    browsing: Signal<CoreType>,
    channel: Signal<CoreChannel>,
    local: Signal<Vec<LocalCoreRow>>,
    remote: Signal<Vec<RemoteRow>>,
    remote_checked: Signal<bool>,
    busy: Signal<bool>,
    stage: Signal<Option<String>>,
    error: Signal<Option<String>>,
    pending_delete: Signal<Option<(String, PathBuf)>>,
    task: Stored<Option<Spawned<()>>>,
    /// 页面初始化时的活动版本基线。必须与初始 activity 在同一构造流程中
    /// 按“先 revision、后 activity”读取；任何落在两者之间的 worker 完成都会
    /// 推进 revision，monitor 首 tick 可见，不会永久 busy。
    shared_activity_revision: u64,
    /// 已保存的系统代理偏好（页面打开时读一次；切换后立即回写）。
    proxy_enabled: Signal<bool>,
    /// 启用系统代理前的一次性确认框（说明仅本次接管/精确归还/授权不保存）。
    proxy_confirm: Signal<bool>,
}

impl CorePageState {
    fn new() -> Self {
        let inventory = CoreInventory::open();
        let runtime = CoreRuntime::open();
        let mut error = None;
        if let Err(failure) = &inventory {
            error = Some(format!("打开数据根失败:{failure}"));
        }
        if let Err(failure) = &runtime {
            error = Some(format!("打开核心运行时失败:{failure}"));
        }
        // 系统代理偏好：页面打开时读已保存值（运行时未打开时默认关闭）。
        let state_proxy_enabled = runtime
            .as_ref()
            .ok()
            .and_then(|runtime| runtime.proxy_preference().ok())
            .unwrap_or(false);
        // 同根后台下载可能来自已销毁的旧页面：先记录 revision 基线，再读取
        // activity。若 worker 在两者之间完成，基线仍是完成前版本，monitor
        // 下一 tick 能看到变化并清除初始 busy；反过来读会漏掉该窗口。
        let (initial_activity_revision, initial_activity) = read_activity_initial(
            || {
                inventory
                    .as_ref()
                    .ok()
                    .map(CoreInventory::activity_revision)
                    .unwrap_or(0)
            },
            || {
                inventory
                    .as_ref()
                    .ok()
                    .and_then(|inventory| inventory.activity())
            },
        );
        let state = Self {
            inventory: inventory.ok(),
            runtime: runtime.ok(),
            owner: Owner::current(),
            browsing: signal(CoreType::SingBox),
            channel: signal(CoreChannel::Stable),
            local: signal(Vec::new()),
            remote: signal(Vec::new()),
            remote_checked: signal(false),
            busy: signal(initial_activity.is_some()),
            stage: signal(initial_activity.as_ref().map(activity_stage)),
            error: signal(error),
            pending_delete: signal(None),
            task: stored(None),
            shared_activity_revision: initial_activity_revision,
            proxy_enabled: signal(state_proxy_enabled),
            proxy_confirm: signal(false),
        };
        // 初始载入：先读实际选中（决定浏览类型与通道默认值），再列本地版本。
        // 实际选中与全局状态栏/概览共用 App 级信号；读取失败保留最后有效
        // 摘要并记录错误，不把损坏配置当成“尚未选择”。
        if state.inventory.is_some() {
            app_state::refresh_active_core(&app_state::current());
            if let Some(selection) = app_state::current().active_core.get() {
                state.browsing.set(selection.core_type);
                state
                    .channel
                    .set(supported_channel(channel_of_version(&selection.version)));
            }
        }
        state.reload_local();
        state
    }

    /// busy 全局单飞：在途库存操作时不启动新工作（调用方不重入）。
    fn begin_work(&self, label: &'static str) -> bool {
        if self.busy.get() || self.inventory.is_none() {
            return false;
        }
        self.busy.set(true);
        self.error.set(None);
        self.stage.set(Some(label.to_owned()));
        true
    }

    /// 刷新本地列表；失败保留旧列表并显示真实错误。
    fn reload_local(&self) {
        let Some(inventory) = self.inventory.clone() else {
            return;
        };
        match inventory.list_local(self.browsing.get()) {
            Ok(list) => self
                .local
                .set(list.iter().map(LocalCoreRow::from_managed).collect()),
            Err(error) => self
                .error
                .set(Some(format!("刷新本地列表失败:{error}；保留上次列表"))),
        }
    }

    fn switch_type(&self, kind: CoreType) {
        if self.busy.get() || self.browsing.get() == kind {
            return;
        }
        self.browsing.set(kind);
        self.remote.set(Vec::new());
        self.remote_checked.set(false);
        self.error.set(None);
        self.reload_local();
    }

    /// 当前通道的远端行（下载只针对已查询到的版本）。
    fn current_remote(&self) -> Option<RemoteRow> {
        let channel = self.channel.get().as_str();
        self.remote
            .get()
            .into_iter()
            .find(|row| row.channel == channel)
    }

    /// 本地是否已安装指定版本（下载按钮的“重新校验并选中”语义）。
    fn is_installed(&self, version: &str) -> bool {
        self.local.get().iter().any(|row| row.version == version)
    }
}

/// beta 通道并入预发布展示（与共享通道模型一致：本页只分稳定/预发布）。
fn supported_channel(channel: CoreChannel) -> CoreChannel {
    match channel {
        CoreChannel::Stable => CoreChannel::Stable,
        CoreChannel::Beta | CoreChannel::Prerelease => CoreChannel::Prerelease,
    }
}

pub(super) fn stage_label(stage: CoreDownloadStage) -> &'static str {
    match stage {
        CoreDownloadStage::ResolvingRelease => "正在查询指定版本的发布资产",
        CoreDownloadStage::Downloading => "正在下载核心归档",
        CoreDownloadStage::Extracting => "正在解压核心二进制",
        CoreDownloadStage::Verifying => "正在探测并校验核心版本",
        CoreDownloadStage::Selecting => "正在保存选中核心",
    }
}

// ---------- 后台操作 ----------

fn spawn_check_remote(state: CorePageState) {
    if !state.begin_work("正在查询远端版本…") {
        return;
    }
    let Some(inventory) = state.inventory.clone() else {
        return;
    };
    let kind = state.browsing.get();
    spawn_work(state, "正在查询远端版本…", None, move |_slot| async move {
        inventory
            .remote_channels(kind)
            .await
            .map(|channels| {
                WorkOutcome::Remote(
                    channels
                        .into_iter()
                        .map(|channel| RemoteRow {
                            channel: match channel.channel() {
                                "stable" => "stable",
                                _ => "prerelease",
                            },
                            version: channel.version().to_owned(),
                        })
                        .collect(),
                )
            })
            .map_err(|error| format!("检查更新失败：{error}；保留上次版本信息"))
    });
}

fn spawn_download(state: CorePageState, version: String) {
    // 只下载已查询到的版本（伪造/手填版本不进入下载路径）。
    if !state
        .current_remote()
        .is_some_and(|remote| remote.version == version)
    {
        return;
    }
    if !state.begin_work("正在下载并校验核心…") {
        return;
    }
    let Some(inventory) = state.inventory.clone() else {
        return;
    };
    let kind = state.browsing.get();
    let notice = format!("已下载并选中 {}", core_tag(&version));
    spawn_work(
        state,
        "正在下载并校验核心…",
        Some(notice),
        move |slot| async move {
            inventory
                .download(kind, &version, &move |stage| {
                    if let Ok(mut guard) = slot.lock() {
                        guard.stage = format!("{}…", stage_label(stage));
                    }
                })
                .await
                .map(|_| WorkOutcome::Changed)
                .map_err(|error| format!("下载核心失败:{error}"))
        },
    );
}

fn spawn_select(state: CorePageState, kind: CoreType, version: String, path: PathBuf) {
    if !state.begin_work("正在探测并保存选中核心…") {
        return;
    }
    let Some(inventory) = state.inventory.clone() else {
        return;
    };
    let notice = format!("已选中 {}", core_tag(&version));
    spawn_work(
        state,
        "正在探测并保存选中核心…",
        Some(notice),
        move |_slot| async move {
            inventory
                .select(kind, &path)
                .map(|_| WorkOutcome::Changed)
                .map_err(|error| format!("选择核心失败:{error}"))
        },
    );
}

fn spawn_delete(state: CorePageState, version: String, path: PathBuf) {
    if !state.begin_work("正在删除核心版本目录…") {
        return;
    }
    let Some(inventory) = state.inventory.clone() else {
        return;
    };
    let kind = state.browsing.get();
    let notice = format!("已删除 {}", core_tag(&version));
    spawn_work(
        state,
        "正在删除核心版本目录…",
        Some(notice),
        move |_slot| async move {
            inventory
                .delete(kind, &path)
                .map(|_| WorkOutcome::Changed)
                .map_err(|error| format!("删除核心失败:{error}"))
        },
    );
}

/// 统一后台工作模型：busy 已由调用方置位；worker 在 tokio 后台执行并把结果
/// 写入共享槽位，UI 任务轮询槽位、只在 owner 线程回写 Signal；完成/中断都
/// 复位 busy。页面 owner 只拥有 UI 观察，不拥有已跨 await 的业务 mutation；
/// 新页面通过库存共享活动恢复真实 busy。
/// spawn 挂在页面级 owner 下（见 [`CorePageState::owner`]）：弹窗/行级临时
/// owner 销毁不会取消在途工作。mutation 互斥在业务层（同根共享锁），此处
/// 的 work 返回 Err 时按普通失败上屏（如“另一项核心库存操作正在进行”）。
fn spawn_work<F>(
    state: CorePageState,
    label: &'static str,
    notice: Option<String>,
    work: impl FnOnce(Arc<Mutex<WorkSlot>>) -> F,
) where
    F: Future<Output = Result<WorkOutcome, String>> + Send + 'static,
{
    let shared = Arc::new(Mutex::new(WorkSlot::default()));
    let future = work(shared.clone());
    let done = state.clone();
    let polling = state.clone();
    state.owner.run(move || {
        state.task.with(|task_slot| {
            task_slot.replace(spawn(async move {
                // worker 在 spawn 任务内启动：background 需要 tokio 上下文
                //（engine 的事件回调线程没有 runtime）。
                let worker_shared = shared.clone();
                let _worker = background(async move {
                    let outcome = future.await;
                    if let Ok(mut guard) = worker_shared.lock() {
                        guard.outcome = Some(outcome);
                        guard.finished = true;
                    }
                });
                polling.stage.set(Some(label.to_owned()));
                loop {
                    let (stage, finished, outcome) = match shared.lock() {
                        Ok(mut guard) => {
                            (guard.stage.clone(), guard.finished, guard.outcome.take())
                        }
                        Err(_) => (String::new(), true, Some(Err("任务状态锁中毒".to_owned()))),
                    };
                    if !stage.is_empty() {
                        polling.stage.set(Some(stage));
                    }
                    if finished {
                        done.busy.set(false);
                        done.stage.set(None);
                        match outcome {
                            Some(Ok(WorkOutcome::Remote(rows))) => {
                                done.error.set(None);
                                done.remote.set(rows);
                                done.remote_checked.set(true);
                            }
                            Some(Ok(WorkOutcome::Changed)) => {
                                done.error.set(None);
                                app_state::refresh_active_core(&app_state::current());
                                done.reload_local();
                                if let Some(msg) = notice {
                                    crate::ui::feedback::show_toast(msg);
                                }
                            }
                            Some(Err(message)) => {
                                // 失败保留旧列表与旧选中，只显示真实错误。
                                done.error.set(Some(message));
                            }
                            None => done.error.set(Some("任务未完成".to_owned())),
                        }
                        break;
                    }
                    // 真实时间轮询（engine 的 timer() 走引擎时钟，headless
                    // 手动时钟下不推进；tokio 计时到点经引擎 waker 唤醒任务）。
                    tokio::time::sleep(STAGE_POLL).await;
                }
            }));
        });
    });
}
