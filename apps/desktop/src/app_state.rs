//! App 级共享运行摘要与命令状态：概览、核心页与
//! 全局状态栏消费同一份 `CoreRuntime` 真实状态与同一命令状态，不由各页
//! 独立轮询。状态轮询与命令任务挂在 App owner 下：切页不取消、不重 poll，
//! 不产生互相矛盾的状态，也不会因离页永久 busy。
//!
//! 业务状态只保存语义值和原始错误；展示文案集中在 `display`，由绑定按当前
//! 语言重新生成。摘要读取失败保留最后成功值并单独暴露错误，不把坏数据或
//! 瞬时失败伪装成“未设置”。

mod display;
mod model;

pub(crate) use display::{
    command_error_line, command_pending_line, core_error_line, core_selection_line, core_tag,
    exit_error_line, proxy_ownership_line, proxy_preference_line, proxy_recovery_visible,
    runtime_status_line, subscription_error_line, subscription_summary_line,
    summary_error_short_line,
};
pub(crate) use model::*;

use std::time::Duration;

use lanpath_common::{LanpathError, LanpathResult};
use nsh_core::prelude::*;

use crate::core_inventory::CoreInventory;
use crate::core_runtime::CoreRuntime;
use crate::subscriptions::Subscriptions;

/// 状态轮询间隔：运行中/异常退出都由这里的真实 `status()` poll 发现，
/// 页面不只记按钮状态。App 级单实例，切页不重建。
const STATUS_POLL: Duration = Duration::from_millis(500);

/// App 级共享状态单例。
pub(crate) fn current() -> AppState {
    global::<AppState>().get()
}

/// App 启动安装：打开共享运行时与库存、读取摘要、拉起唯一的状态轮询。
/// 必须在 App 组件体内调用（捕获 App owner，任务随应用存活）。
pub(crate) fn install(startup_ready: bool) {
    let mut state = current();
    state.owner = Some(Owner::current());
    if startup_ready {
        match CoreRuntime::open() {
            Ok(runtime) => state.runtime = Some(runtime),
            Err(error) => {
                state
                    .command
                    .update(|command| command.error = Some(CommandError::RuntimeUnavailable));
                tracing::error!(target: "lanpath::app_state",
                    "打开核心运行时失败：{error}");
            }
        }
        match CoreInventory::open() {
            Ok(inventory) => state.inventory = Some(inventory),
            Err(error) => {
                state.core_error.set(Some(error.to_string()));
                tracing::error!(target: "lanpath::app_state",
                    "打开核心库存失败：{error}");
            }
        }
    }
    match Subscriptions::open() {
        Ok(subscriptions) => state.subscriptions = Some(subscriptions),
        Err(error) => {
            state.subscription_error.set(Some(error.to_string()));
            tracing::error!(target: "lanpath::app_state",
                "打开订阅业务失败：{error}");
        }
    }
    refresh_active_core(&state);
    refresh_active_subscription(&state);
    refresh_proxy_preference(&state);
    spawn_status_poll(&state);
    global::<AppState>().set(state);
}

/// 从共享订阅业务层重读当前生效订阅摘要。
///
/// 读取失败时保留最后有效摘要，只更新独立错误信号；不将 `Err` 折叠成
/// “没有订阅”，也不会覆盖概览/状态栏已经展示的有效值。
pub(crate) fn refresh_active_subscription(state: &AppState) {
    let result = load_active_subscription(state);
    match result {
        Ok(summary) => {
            state.active_subscription.set(summary);
            state.subscription_error.set(None);
        }
        Err(error) => state.subscription_error.set(Some(error.to_string())),
    }
}

fn load_active_subscription(state: &AppState) -> LanpathResult<Option<SubscriptionSummary>> {
    let service = state
        .subscriptions
        .clone()
        .map(Ok)
        .unwrap_or_else(Subscriptions::open)?;
    let Some(active_id) = service.active_id()? else {
        return Ok(None);
    };
    let list = service.load()?;
    let subscription = list
        .into_iter()
        .find(|subscription| subscription.id == active_id)
        .ok_or_else(|| {
            LanpathError::Client("当前生效订阅在本地清单中不存在；请重新选择或检查清单".into())
        })?;
    Ok(Some(SubscriptionSummary {
        id: subscription.id,
        name: subscription.name,
        enabled: subscription.enabled,
        node_count: subscription.node_count,
        format: subscription.format,
    }))
}

/// 从共享核心库存重读实际选中核心；失败保留最后有效摘要并暴露错误。
pub(crate) fn refresh_active_core(state: &AppState) {
    let result = match state.inventory.as_ref() {
        Some(inventory) => inventory.active_core().map(|core| {
            core.map(|core| CoreSelection {
                core_type: core.core_type(),
                version: core.version().to_owned(),
            })
        }),
        None => Err(LanpathError::Client("核心库存不可用".into())),
    };
    match result {
        Ok(selection) => {
            state.active_core.set(selection);
            state.core_error.set(None);
        }
        Err(error) => state.core_error.set(Some(error.to_string())),
    }
}

/// 当前系统代理偏好是独立状态轴；读取失败保留最后有效值。
pub(crate) fn refresh_proxy_preference(state: &AppState) {
    let Some(runtime) = state.runtime.as_ref() else {
        state.proxy_preference.set(None);
        return;
    };
    if let Ok(preference) = runtime.proxy_preference() {
        state.proxy_preference.set(Some(preference));
    }
}

/// 主动刷新一次本地 SQLite 流量统计（清空记录等操作后即时反馈到概览与连接页）。
pub(crate) fn refresh_traffic_stats(state: &AppState) {
    let (Some(runtime), Some(owner)) = (state.runtime.clone(), state.owner) else {
        return;
    };
    let traffic = state.traffic;
    let tasks = state.tasks.clone();
    owner.run(move || {
        let handle = spawn(async move {
            if let Ok(store) = runtime.stats_store().await {
                let today_key = lanpath_client::stats::today();
                let today = store.today_summary(&today_key).await.unwrap_or_default();
                let recent_days = store.recent_daily_totals(7).await.unwrap_or_default();
                traffic.update(|snap| {
                    snap.today = today;
                    snap.recent_days = recent_days;
                });
            }
        });
        tasks.borrow_mut().push(handle);
    });
}

fn spawn_status_poll(state: &AppState) {
    let (Some(runtime), Some(owner)) = (state.runtime.clone(), state.owner) else {
        return;
    };
    let status = state.status;
    let traffic = state.traffic;
    let tasks = state.tasks.clone();
    let poll_state = state.clone();
    let inventory = state.inventory.clone();
    let subscriptions = state.subscriptions.clone();
    let mut core_revision = inventory
        .as_ref()
        .map(CoreInventory::activity_revision)
        .unwrap_or(0);
    let mut subscription_revision = subscriptions
        .as_ref()
        .map(|service| service.refresh_activity_revision())
        .unwrap_or(0);
    owner.run(move || {
        let handle = spawn(async move {
            let mut tick: u64 = 0;
            let mut prev_running = false;
            let mut prev_by_id: std::collections::HashMap<String, (u64, u64)> =
                std::collections::HashMap::new();
            loop {
                let live_status = RuntimeView::from_status(&runtime.status().await);
                let is_running = live_status.running;
                status.set(live_status);

                // 每 2 拍（约 1 秒）采样一次实时连接速率与滚动曲线。
                if tick.is_multiple_of(2) {
                    if is_running {
                        if let Some(active) = runtime.active_connections().await {
                            let mut up_delta = 0u64;
                            let mut down_delta = 0u64;
                            let mut next_map =
                                std::collections::HashMap::with_capacity(active.connections.len());
                            for conn in &active.connections {
                                if let Some((prev_up, prev_down)) = prev_by_id.get(&conn.id) {
                                    up_delta = up_delta
                                        .saturating_add(conn.upload.saturating_sub(*prev_up));
                                    down_delta = down_delta
                                        .saturating_add(conn.download.saturating_sub(*prev_down));
                                }
                                next_map.insert(conn.id.clone(), (conn.upload, conn.download));
                            }
                            prev_by_id = next_map;
                            let active_count = active.connections.len();
                            let up_total = active.upload_total;
                            let down_total = active.download_total;
                            traffic.update(|snap| {
                                snap.up_bps = up_delta;
                                snap.down_bps = down_delta;
                                snap.active_count = active_count;
                                snap.active_upload_total = up_total;
                                snap.active_download_total = down_total;
                                if snap.history.len() >= TRAFFIC_HISTORY_LEN {
                                    snap.history.remove(0);
                                }
                                snap.history.push(TrafficPoint {
                                    up_bps: up_delta as f64,
                                    down_bps: down_delta as f64,
                                });
                            });
                        }
                    } else {
                        prev_by_id.clear();
                        traffic.update(|snap| {
                            snap.up_bps = 0;
                            snap.down_bps = 0;
                            snap.active_count = 0;
                            snap.active_upload_total = 0;
                            snap.active_download_total = 0;
                            if snap.history.len() >= TRAFFIC_HISTORY_LEN {
                                snap.history.remove(0);
                            }
                            snap.history.push(TrafficPoint::default());
                        });
                    }
                }

                // 首拍、运行态切换或每 10 拍（约 5 秒）刷新 SQLite 统计汇总与 7 日数据。
                if (tick == 0 || is_running != prev_running || tick.is_multiple_of(10))
                    && let Ok(store) = runtime.stats_store().await
                {
                    let today_key = lanpath_client::stats::today();
                    let today = store.today_summary(&today_key).await.unwrap_or_default();
                    let recent_days = store.recent_daily_totals(7).await.unwrap_or_default();
                    traffic.update(|snap| {
                        snap.today = today;
                        snap.recent_days = recent_days;
                    });
                }
                prev_running = is_running;
                tick = tick.wrapping_add(1);

                // 业务层在离页 worker 真正完成时推进活动版本；只有变化才
                // 重读磁盘摘要，不用频繁轮询掩盖生命周期问题。
                if let Some(inventory) = inventory.as_ref() {
                    let revision = inventory.activity_revision();
                    if revision != core_revision {
                        core_revision = revision;
                        refresh_active_core(&poll_state);
                    }
                }
                if let Some(service) = subscriptions.as_ref() {
                    let revision = service.refresh_activity_revision();
                    if revision != subscription_revision {
                        subscription_revision = revision;
                        refresh_active_subscription(&poll_state);
                    }
                }
                tokio::time::sleep(STATUS_POLL).await;
            }
        });
        tasks.borrow_mut().push(handle);
    });
}

/// 共享命令统一入口：App 级单飞，任务挂在 App owner 下；完成后刷新摘要。
fn run_command(
    state: &AppState,
    kind: CommandKind,
    work: impl std::future::Future<Output = Result<CommandResult, CommandError>> + Send + 'static,
) -> bool {
    let (Some(owner), Some(_)) = (state.owner, state.runtime.clone()) else {
        state.command.update(|command| {
            command.error = Some(CommandError::RuntimeUnavailable);
            command.busy = false;
        });
        return false;
    };
    // 在事件处理器的同步路径先预约 busy，再异步启动。若只在新任务的
    // 首帧里置 busy，调用方的“检查 busy→提交命令”之间可被同一帧的
    // 第二次操作重入，导致两个 worker 并发并互相覆盖结果。
    if state.command.get().busy {
        return false;
    }
    state.command.update(|command| {
        command.busy = true;
        command.kind = Some(kind);
        command.result = None;
        command.error = None;
    });
    let command = state.command;
    let tasks = state.tasks.clone();
    let done = state.clone();
    owner.run(move || {
        let refresh = done.clone();
        let handle = spawn(async move {
            // worker 在 spawn 任务内启动：background 需要 tokio 上下文。
            let outcome = background(work)
                .await
                .unwrap_or(Err(CommandError::TaskInterrupted));
            command.update(|command| {
                command.busy = false;
                command.kind = None;
                match outcome {
                    Ok(result) => {
                        let toast_text = display::command_result_line(&Some(result.clone()));
                        if !toast_text.is_empty() {
                            crate::ui::feedback::show_toast(toast_text);
                        }
                        command.result = Some(result);
                    }
                    Err(error) => command.error = Some(error),
                }
            });
            // 命令可能刷新订阅缓存、选择核心或改变代理偏好；完成后统一重读。
            refresh_active_subscription(&refresh);
            refresh_active_core(&refresh);
            refresh_proxy_preference(&refresh);
        });
        tasks.borrow_mut().push(handle);
    });
    true
}

/// “校验并启动”：busy 单飞 + 未在运行/在途才可发起。
pub(crate) fn spawn_start(state: &AppState) {
    let status = state.status.get();
    if status.running || status.operation.is_some() || state.command.get().busy {
        return;
    }
    let Some(runtime) = state.runtime.clone() else {
        state.command.update(|command| {
            command.error = Some(CommandError::RuntimeUnavailable);
        });
        return;
    };
    let _ = run_command(state, CommandKind::Start, async move {
        runtime
            .start()
            .await
            .map(CommandResult::Started)
            .map_err(|error| CommandError::StartFailed(error.to_string()))
    });
}

/// “停止”：仅在运行中（且无在途操作）可发起。
pub(crate) fn spawn_stop(state: &AppState) {
    let status = state.status.get();
    if !status.running || status.operation.is_some() || state.command.get().busy {
        return;
    }
    let Some(runtime) = state.runtime.clone() else {
        state.command.update(|command| {
            command.error = Some(CommandError::RuntimeUnavailable);
        });
        return;
    };
    let _ = run_command(state, CommandKind::Stop, async move {
        runtime
            .stop()
            .await
            .map(CommandResult::Stopped)
            .map_err(|error| CommandError::StopFailed(error.to_string()))
    });
}

/// 运行中切换系统代理偏好的安全启停周期；调用前必须已保存偏好并授予授权。
/// 返回 false 表示命令未启动，调用方必须回滚本次偏好/授权修改。
pub(crate) fn spawn_restart(state: &AppState) -> bool {
    let status = state.status.get();
    if state.command.get().busy || status.operation.is_some() {
        return false;
    }
    let Some(runtime) = state.runtime.clone() else {
        state.command.update(|command| {
            command.error = Some(CommandError::RuntimeUnavailable);
        });
        return false;
    };
    run_command(state, CommandKind::Restart, async move {
        runtime
            .stop()
            .await
            .map_err(|error| CommandError::StopFailed(error.to_string()))?;
        runtime
            .start()
            .await
            .map(CommandResult::Started)
            .map_err(|error| CommandError::RestartStartFailed(error.to_string()))
    })
}

/// 显式恢复系统代理：只按本应用 owner 的守护记录恢复，未运行时也允许。
pub(crate) fn spawn_restore_proxy(state: &AppState) -> bool {
    let status = state.status.get();
    if state.command.get().busy || status.operation.is_some() {
        return false;
    }
    if !status.proxy_supported {
        state.command.update(|command| {
            command.error = Some(CommandError::ProxyUnsupported);
        });
        return false;
    }
    let Some(runtime) = state.runtime.clone() else {
        state.command.update(|command| {
            command.error = Some(CommandError::RuntimeUnavailable);
        });
        return false;
    };
    run_command(state, CommandKind::RestoreProxy, async move {
        runtime
            .restore_system_proxy()
            .await
            .map(|()| CommandResult::ProxyRestored)
            .map_err(|error| CommandError::RestoreProxyFailed(error.to_string()))
    })
}
