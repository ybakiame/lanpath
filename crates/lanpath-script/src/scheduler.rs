//! cron 脚本调度器：管理按 cron 表达式定时执行的脚本任务（QX/Surge/Loon 的
//! task/cron 签到类脚本），支持注册/移除、手动触发、到期批量执行与后台循环。

use std::str::FromStr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::host::ScriptHost;
use crate::types::{
    ScriptDialect, ScriptExecutionMetadata, ScriptKind, ScriptLimits, ScriptOutput,
};
use crate::worker::ScriptWorker;
use lanpath_common::{LanpathError, LanpathResult};

/// 一个待注册的定时脚本任务。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskScript {
    pub name: String,
    pub cron_expr: String,
    pub source: String,
    pub dialect: ScriptDialect,
    pub enabled: bool,
    #[serde(default)]
    pub argument: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Stable imported source identity; an empty legacy identity falls back to `name`.
    #[serde(default)]
    pub source_id: String,
    #[serde(default)]
    pub module_id: Option<String>,
}

impl TaskScript {
    /// Opaque registration identity, independent of arguments, schedule and runtime records.
    /// Structured encoding keeps module/source/declaration boundaries unambiguous.
    pub fn id(&self) -> String {
        let module = self.module_id.as_deref().unwrap_or_default();
        format!(
            "task:{}:{}:{}:{}:{}:{}:{}",
            u8::from(self.module_id.is_some()),
            module.len(),
            module,
            self.source_id.len(),
            self.source_id,
            self.name.len(),
            self.name,
        )
    }
}

/// 任务运行时视图：任务信息 + 下次执行/上次执行/上次错误。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskScriptView {
    pub id: String,
    pub name: String,
    pub cron_expr: String,
    pub dialect: ScriptDialect,
    pub enabled: bool,
    /// 从当前时刻起的下一次执行时间。
    pub next_run: Option<DateTime<Utc>>,
    /// 上次执行时间（手动触发/到期执行均记录）。
    pub last_run: Option<DateTime<Utc>>,
    /// 上次执行的错误信息（成功为 None）。
    pub last_error: Option<String>,
}

/// 内部登记的任务：`TaskScript` + 解析后的 `Schedule` + 运行记录。
struct ScheduledTask {
    id: String,
    task: Arc<TaskScript>,
    schedule: Schedule,
    last_run: Option<DateTime<Utc>>,
    last_error: Option<String>,
}

/// cron 脚本调度器。
///
/// 任务注册时解析 cron 表达式并校验；到期判定基于“`(last_run, now]` 区间内是否有
/// 匹配时刻”，无状态可注入 `now` 便于测试。单任务执行统一走 [`ScriptWorker`]
/// （`kind = Cron`），失败仅记录在结果/`last_error`，不影响其他任务。
pub struct ScriptScheduler {
    worker: ScriptWorker,
    tasks: Mutex<Vec<ScheduledTask>>,
    shutdown: watch::Sender<bool>,
}

impl ScriptScheduler {
    pub fn new(host: Arc<ScriptHost>, limits: ScriptLimits) -> Self {
        let (shutdown, _rx) = watch::channel(false);
        let worker = ScriptWorker::new(host, limits);
        Self {
            worker,
            tasks: Mutex::new(Vec::new()),
            shutdown,
        }
    }

    /// 注册任务：校验并解析 cron 表达式；相同模块、来源和声明的重复任务报错。
    pub fn add_task(&mut self, task: TaskScript) -> LanpathResult<()> {
        let normalized;
        let cron_expr = if task.cron_expr.split_whitespace().count() == 5 {
            normalized = format!("0 {}", task.cron_expr);
            normalized.as_str()
        } else {
            task.cron_expr.as_str()
        };
        let schedule = Schedule::from_str(cron_expr).map_err(|e| {
            LanpathError::Validation(format!("invalid cron expression '{}': {e}", task.cron_expr))
        })?;
        let id = task.id();
        let mut tasks = self.lock_tasks();
        if tasks.iter().any(|t| t.id == id) {
            return Err(LanpathError::Validation(format!(
                "task already registered: {}",
                task.name
            )));
        }
        tasks.push(ScheduledTask {
            id,
            task: Arc::new(task),
            schedule,
            last_run: None,
            last_error: None,
        });
        Ok(())
    }

    /// 移除任务；返回是否存在并被移除。
    pub fn remove_task(&mut self, id: &str) -> bool {
        let mut tasks = self.lock_tasks();
        let before = tasks.len();
        tasks.retain(|t| t.id != id);
        tasks.len() != before
    }

    /// 列出全部任务（含下次执行/上次执行/上次错误）。
    pub fn list_tasks(&self) -> Vec<TaskScriptView> {
        let tasks = self.lock_tasks();
        let now = Utc::now();
        tasks
            .iter()
            .map(|t| TaskScriptView {
                id: t.id.clone(),
                name: t.task.name.clone(),
                cron_expr: t.task.cron_expr.clone(),
                dialect: t.task.dialect,
                enabled: t.task.enabled,
                next_run: t.schedule.after(&now).next(),
                last_run: t.last_run,
                last_error: t.last_error.clone(),
            })
            .collect()
    }

    /// 手动触发指定任务（对应 QX 点击执行 / Surge 手动运行）。
    ///
    /// 不校验 `enabled`：手动执行与定时开关相互独立。
    pub async fn run_now(&self, id: &str) -> LanpathResult<ScriptOutput> {
        let task = {
            let tasks = self.lock_tasks();
            let task = tasks
                .iter()
                .find(|t| t.id == id)
                .ok_or_else(|| LanpathError::NotFound(format!("task not found: {id}")))?;
            Arc::clone(&task.task)
        };
        let result = self.run_script_once(&task).await;
        self.record_run(id, &result, Utc::now()).await;
        result
    }

    /// 执行所有到期任务（`now` 可注入以便测试）。
    ///
    /// 返回 `(任务 ID, 执行结果)`；单任务失败不中断其他任务（异常隔离）。
    /// 从未执行的任务只要存在任一匹配时刻早于/等于 `now` 即视为到期（首次注册后补跑）；
    /// 已执行过的任务仅在 `last_run` 之后存在新匹配时刻时触发，避免轮询重复执行。
    pub async fn run_due(&self, now: DateTime<Utc>) -> Vec<(String, LanpathResult<ScriptOutput>)> {
        let due: Vec<(String, Arc<TaskScript>)> = {
            let tasks = self.lock_tasks();
            let mut due = Vec::new();
            for task in tasks.iter().filter(|t| t.task.enabled) {
                if Self::is_due(&task.schedule, now, task.last_run) {
                    due.push((task.id.clone(), Arc::clone(&task.task)));
                }
            }
            due
        };
        let mut results = Vec::with_capacity(due.len());
        for (id, task) in due {
            let result = self.run_script_once(&task).await;
            self.record_run(&id, &result, now).await;
            results.push((id, result));
        }
        results
    }

    /// 启动后台循环（公开 API：1s 轮询 tick）。返回可 join 的 handle。
    ///
    /// 循环以 tick 为上限轮询：计算最近一次未来触发时刻并等待（不超过 tick），
    /// 到期后执行全部到期任务；收到 `stop()` 信号后退出。
    pub async fn start(self: Arc<Self>) -> LanpathResult<JoinHandle<()>> {
        self.start_with_interval(Duration::from_secs(1)).await
    }

    /// 内部启动：以指定轮询间隔驱动后台循环（测试可注入更短间隔）。
    ///
    /// 脚本执行由 [`ScriptWorker`] 在专有线程驱动（`Send` future），后台循环
    /// 直接在调用方运行时 `tokio::spawn`，不再需要 `spawn_blocking` +
    /// 独立 `current_thread` runtime 的绕行。
    pub(crate) async fn start_with_interval(
        self: Arc<Self>,
        tick: Duration,
    ) -> LanpathResult<JoinHandle<()>> {
        let mut shutdown_rx = self.shutdown.subscribe();
        let scheduler = Arc::clone(&self);
        let handle = tokio::spawn(async move {
            loop {
                // stop 已触发（含 start 之前 stop）：立即退出。
                if *shutdown_rx.borrow() {
                    break;
                }
                let sleep_for = {
                    let tasks = scheduler.lock_tasks();
                    let now = Utc::now();
                    let soonest = tasks
                        .iter()
                        .filter(|t| t.task.enabled)
                        .filter_map(|t| t.schedule.after(&now).next())
                        .min();
                    match soonest {
                        Some(next) => {
                            let delta = (next - now).to_std().unwrap_or(Duration::ZERO);
                            delta.min(tick).max(Duration::from_millis(1))
                        }
                        None => tick,
                    }
                };
                tokio::select! {
                    _ = shutdown_rx.changed() => (),
                    _ = tokio::time::sleep(sleep_for) => {
                        let now = Utc::now();
                        for (id, result) in scheduler.run_due(now).await {
                            if let Err(e) = result {
                                tracing::warn!(task_id = %id, error = %e, "scheduled task failed");
                            }
                        }
                    }
                }
            }
        });
        Ok(handle)
    }

    /// 发送 shutdown 信号，后台循环收到后退出。
    pub async fn stop(&self) {
        let _ = self.shutdown.send(true);
        tokio::task::yield_now().await;
    }

    /// 单任务执行：经 [`ScriptWorker`] 串行执行（`kind = Cron`，超时/异常由引擎隔离）。
    async fn run_script_once(&self, task: &TaskScript) -> LanpathResult<ScriptOutput> {
        let metadata = ScriptExecutionMetadata {
            name: task.name.clone(),
            source_id: if task.source_id.is_empty() {
                task.name.clone()
            } else {
                task.source_id.clone()
            },
            module_id: task.module_id.clone(),
            binary_body_mode: false,
        };
        self.worker
            .run_script_with_timeout(
                &task.source,
                ScriptKind::Cron,
                None,
                task.argument.as_deref(),
                task.dialect,
                metadata,
                task.timeout_ms,
            )
            .await
    }

    /// 记录执行结果（last_run / last_error）。
    async fn record_run(&self, id: &str, result: &LanpathResult<ScriptOutput>, at: DateTime<Utc>) {
        let mut tasks = self.lock_tasks();
        if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
            task.last_run = Some(at);
            match result {
                Ok(_) => task.last_error = None,
                Err(e) => task.last_error = Some(e.to_string()),
            }
        }
    }

    /// 到期判定（无副作用）：
    /// - 已执行过：`last_run` 之后存在不晚于 `now` 的匹配时刻才到期（避免重复触发）；
    /// - 从未执行：`now` 本身命中或存在早于 `now` 的匹配时刻即到期（首次补跑）。
    fn is_due(schedule: &Schedule, now: DateTime<Utc>, last_run: Option<DateTime<Utc>>) -> bool {
        match last_run {
            Some(last) => schedule.after(&last).next().is_some_and(|next| next <= now),
            None => schedule.includes(now) || schedule.after(&now).next_back().is_some(),
        }
    }

    /// 加锁获取任务表（吞掉 poison，避免一处 panic 拖垮调度器）。
    fn lock_tasks(&self) -> MutexGuard<'_, Vec<ScheduledTask>> {
        self.tasks.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
#[path = "scheduler_tests.rs"]
mod tests;
