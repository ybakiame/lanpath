//! 核心页：核心运行（校验并启动/停止，用户态 loopback，不接管系统代理/TUN）
//! 与核心库存/下载/选择（共享 `CoreManagementService::experimental` confined
//! 语义）。运行卡片显示真实 running/busy/结果/停止 kind/错误（状态轮询
//! 能发现核心异常退出，不只记按钮状态）；库存卡片支持 sing-box/Mihomo
//! 类型切换（实际选中独立显示）、稳定/预发布通道、查询远端版本（仅
//! stable/prerelease）、下载指定已查询版本、选择本地真实版本、带确认地
//! 删除非 active 版本。active 为空显示“尚未选择核心”，不把无核心当崩溃。
//!
//! busy 全局单飞（在途操作防重复，完成后恢复）；操作失败保留旧列表并显示
//! 真实错误；下载进度只显示共享真实阶段，不伪造 byte 百分比。操作在
//! `background` 执行，Signal 只在 UI 线程的 `spawn` 任务内回写；离开页面
//! 在途任务随 owner 取消，重进页面是全新状态，不永久 busy。mutation 的
//! 跨页面互斥与运行中拒绝在业务层（同根共享 try_acquire 锁 +
//! `CoreRuntime::is_running_sync` 真实核验），本页 busy 只约束本页按钮。

mod render;
mod runtime;
mod runtime_card;

use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lanpath_client::cores::{CoreChannel, CoreDownloadStage, channel_of_version};
use lanpath_common::{CoreType, ProcessStopKind};
use nsh_core::prelude::*;

use crate::core_inventory::{CoreInventory, LocalCoreRow};
use crate::core_runtime::{CoreRuntime, StartOutcome};

pub(crate) use render::core_page;
use runtime::{RuntimeView, spawn_status_poll};

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
    Started(StartOutcome),
    Stopped(ProcessStopKind),
}

/// 远端通道行（仅 stable/prerelease）。
#[derive(Clone, PartialEq, Eq)]
struct RemoteRow {
    channel: &'static str,
    version: String,
}

/// 实际选中摘要（独立于浏览的库存类型）。
#[derive(Clone, PartialEq, Eq)]
struct ActiveSummary {
    core_type: CoreType,
    version: String,
}

/// 页状态：全部为句柄（Signal/Stored），克隆廉价。
#[derive(Clone)]
struct CorePageState {
    inventory: Option<CoreInventory>,
    runtime: Option<Arc<CoreRuntime>>,
    /// 页面级 owner：在页面构建时捕获。spawn 的工作任务挂在它下面——
    /// 弹窗/行的临时 owner 在按下后被销毁（如确认删除先关弹窗），在其下
    /// spawn 会被立即取消；页面 owner 随离开页面才销毁（在途任务随 owner
    /// 取消，重进页面全新状态，不永久 busy）。
    owner: Owner,
    /// 正在浏览的库存类型（active 类型独立显示，不随浏览切换）。
    browsing: Signal<CoreType>,
    channel: Signal<CoreChannel>,
    active: Signal<Option<ActiveSummary>>,
    local: Signal<Vec<LocalCoreRow>>,
    remote: Signal<Vec<RemoteRow>>,
    remote_checked: Signal<bool>,
    busy: Signal<bool>,
    stage: Signal<Option<String>>,
    error: Signal<Option<String>>,
    notice: Signal<Option<String>>,
    pending_delete: Signal<Option<(String, PathBuf)>>,
    task: Stored<Option<Spawned<()>>>,
    /// 运行卡片真实状态（状态轮询写入，跨页共享）。
    status: Signal<RuntimeView>,
    start_result: Signal<Option<String>>,
    stop_result: Signal<Option<String>>,
    runtime_task: Stored<Option<Spawned<()>>>,
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
        let state = Self {
            inventory: inventory.ok(),
            runtime: runtime.ok(),
            owner: Owner::current(),
            browsing: signal(CoreType::SingBox),
            channel: signal(CoreChannel::Stable),
            active: signal(None),
            local: signal(Vec::new()),
            remote: signal(Vec::new()),
            remote_checked: signal(false),
            busy: signal(false),
            stage: signal(None),
            error: signal(error),
            notice: signal(None),
            pending_delete: signal(None),
            task: stored(None),
            status: signal(RuntimeView::default()),
            start_result: signal(None),
            stop_result: signal(None),
            runtime_task: stored(None),
        };
        // 初始载入：先读实际选中（决定浏览类型与通道默认值），再列本地版本。
        if let Some(inventory) = state.inventory.clone() {
            match inventory.active_core() {
                Ok(Some(core)) => {
                    state.active.set(Some(ActiveSummary {
                        core_type: core.core_type(),
                        version: core.version().to_owned(),
                    }));
                    state.browsing.set(core.core_type());
                    state
                        .channel
                        .set(supported_channel(channel_of_version(core.version())));
                }
                Ok(None) => {}
                Err(error) => state.error.set(Some(format!("读取当前核心失败:{error}"))),
            }
            state.reload_local();
        }
        // 运行状态轮询：随页面 owner 存活；真实状态（含异常退出）跨页共享。
        spawn_status_poll(&state);
        state
    }

    /// busy 全局单飞：在途操作时不启动新工作（调用方不重入）。
    fn begin_work(&self, label: &'static str) -> bool {
        if self.busy.get() || self.inventory.is_none() {
            return false;
        }
        self.busy.set(true);
        self.error.set(None);
        self.notice.set(None);
        self.stage.set(Some(label.to_owned()));
        true
    }

    /// 运行操作（校验并启动/停止）的 busy 单飞：运行时句柄必须可用。
    fn begin_runtime_work(&self, label: &'static str) -> bool {
        if self.busy.get() || self.runtime.is_none() {
            return false;
        }
        self.busy.set(true);
        self.error.set(None);
        self.notice.set(None);
        self.stage.set(Some(label.to_owned()));
        true
    }

    fn reload_active(&self) {
        let Some(inventory) = self.inventory.clone() else {
            return;
        };
        match inventory.active_core() {
            Ok(Some(core)) => self.active.set(Some(ActiveSummary {
                core_type: core.core_type(),
                version: core.version().to_owned(),
            })),
            Ok(None) => self.active.set(None),
            Err(error) => self.error.set(Some(format!("读取当前核心失败:{error}"))),
        }
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

fn core_tag(version: &str) -> String {
    let version = version.trim();
    if version.starts_with("alpha-") {
        version.to_string()
    } else {
        format!("v{}", version.trim_start_matches('v'))
    }
}

fn stage_label(stage: CoreDownloadStage) -> &'static str {
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
/// 复位 busy——离开页面 owner 取消任务，重进页面全新状态，不永久 busy。
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
                                done.reload_active();
                                done.reload_local();
                                done.notice.set(notice);
                            }
                            Some(Ok(WorkOutcome::Started(outcome))) => {
                                done.error.set(None);
                                done.start_result.set(Some(runtime::format_start(&outcome)));
                            }
                            Some(Ok(WorkOutcome::Stopped(kind))) => {
                                done.error.set(None);
                                done.stop_result
                                    .set(Some(runtime::format_stop(kind).to_owned()));
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
