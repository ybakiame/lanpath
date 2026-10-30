use std::sync::Arc;
use std::task::Poll;
use std::time::{Duration, Instant};

use rquickjs::{AsyncContext, AsyncRuntime};
use tokio::sync::oneshot;

use crate::api;
use crate::engine::ScriptEngine;
use crate::host::ScriptHost;
use crate::types::{
    ScriptDialect, ScriptExecutionMetadata, ScriptKind, ScriptLimits, ScriptOutput,
};
use lanpath_common::{LanpathError, LanpathResult};

/// QuickJS（rquickjs）脚本引擎实现。
pub struct QuickJsEngine {
    host: Arc<ScriptHost>,
    dialect: ScriptDialect,
    limits: ScriptLimits,
    pub(crate) metadata: ScriptExecutionMetadata,
    pub(crate) cancellation: Arc<crate::cancellation::Cancellation>,
}

impl QuickJsEngine {
    /// 构造签名约定（与未来 `engine-jsc` 后端保持一致的形状）。
    pub fn new(
        host: Arc<ScriptHost>,
        dialect: ScriptDialect,
        limits: ScriptLimits,
        script_name: String,
    ) -> LanpathResult<Self> {
        Self::with_metadata(
            host,
            dialect,
            limits,
            ScriptExecutionMetadata {
                name: script_name.clone(),
                source_id: script_name,
                ..Default::default()
            },
        )
    }

    pub(crate) fn with_metadata(
        host: Arc<ScriptHost>,
        dialect: ScriptDialect,
        limits: ScriptLimits,
        metadata: ScriptExecutionMetadata,
    ) -> LanpathResult<Self> {
        Ok(Self {
            host,
            dialect,
            limits,
            metadata,
            cancellation: Arc::new(crate::cancellation::Cancellation::default()),
        })
    }
}

impl ScriptEngine for QuickJsEngine {
    #[allow(clippy::let_and_return)]
    async fn run_script(
        &mut self,
        source: &str,
        kind: ScriptKind,
        arg: Option<serde_json::Value>,
        argument: Option<&str>,
    ) -> LanpathResult<ScriptOutput> {
        let timeout_ms = self.limits.timeout_ms;
        let timeout = Duration::from_millis(timeout_ms);

        let rt =
            AsyncRuntime::new().map_err(|e| LanpathError::Script(format!("init runtime: {e}")))?;
        rt.set_memory_limit(self.limits.memory_limit_bytes).await;
        // 默认栈限制：4MB（足够常见脚本，防深度递归撑爆宿主栈）。
        rt.set_max_stack_size(4 * 1024 * 1024).await;

        // Completion, cancellation and deadline interrupts also stop synchronous JS tails.
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| LanpathError::Script("script timeout exceeds clock range".into()))?;
        let execution = api::execution::Execution::new(deadline, self.cancellation.clone());
        let completed = execution.completion_flag();
        let cancellation = self.cancellation.clone();
        rt.set_interrupt_handler(Some(Box::new(move || {
            Instant::now() >= deadline
                || cancellation.is_cancelled()
                || completed.load(std::sync::atomic::Ordering::Acquire)
        })))
        .await;

        let ctx = AsyncContext::full(&rt)
            .await
            .map_err(|e| LanpathError::Script(format!("init context: {e}")))?;

        let (done_tx, mut done_rx) = oneshot::channel::<ScriptOutput>();
        let host = Arc::new(ScriptHost::new(
            self.host.http.clone().session(),
            self.host.store.clone(),
            self.host.notifier.clone(),
        ));
        let dialect = self.dialect;
        let script_name = &self.metadata.name;
        let mut top = None;
        let run_cancellation = self.cancellation.clone();

        // setTimeout 注册表：闭包内注入时写入，闭包返回后由本层在锁内清空。
        let timers = crate::host::TimerRegistry::default();
        let timers_inner = timers.clone();

        // Keep each JS job behind the completion barrier. AsyncContext::async_with
        // drains its entire job queue after polling host futures, swallowing job
        // exceptions and allowing callbacks after a job has accepted $done.
        let initialized = ctx
            .with(|js_ctx| {
                api::install_apis(
                    &js_ctx,
                    &host,
                    api::ScriptContext {
                        dialect,
                        kind,
                        metadata: &self.metadata,
                    },
                    done_tx,
                    &timers_inner,
                    &execution,
                )
                .map_err(|error| LanpathError::Script(format!("install api: {error}")))?;
                api::inject_script_arg(&js_ctx, kind, arg.as_ref())
                    .map_err(|error| LanpathError::Script(format!("inject arg: {error}")))?;
                if let Some(argument) = argument {
                    js_ctx
                        .globals()
                        .set("$argument", argument)
                        .map_err(|error| {
                            LanpathError::Script(format!("inject argument: {error}"))
                        })?;
                }
                match js_ctx.eval::<rquickjs::Value, _>(source) {
                    Ok(value) => Ok(value
                        .into_promise()
                        .map(|promise| rquickjs::Persistent::save(&js_ctx, promise))),
                    Err(error) => {
                        let message = if js_ctx.has_exception() {
                            api::value_to_string(&js_ctx, js_ctx.catch())
                        } else {
                            error.to_string()
                        };
                        Err(LanpathError::Script(message))
                    }
                }
            })
            .await;

        let result = async {
            match initialized {
                Ok(promise) => top = promise,
                Err(error) => {
                    // Accepted completion takes precedence over a later throw or interrupt.
                    if let Ok(output) = done_rx.try_recv() { return Ok(output); }
                    return Err(error);
                }
            }
            loop {
                match done_rx.try_recv() {
                    Ok(output) => break Ok(output),
                    Err(oneshot::error::TryRecvError::Closed) => {
                        tracing::warn!(script = %script_name, "script finished without calling $done");
                        break Ok(ScriptOutput::default());
                    }
                    Err(oneshot::error::TryRecvError::Empty) => {}
                }
                if execution.cancelled() {
                    break Err(LanpathError::Script("script execution cancelled".into()));
                }
                if Instant::now() >= deadline {
                    break Err(LanpathError::Script(format!(
                        "script timeout: $done not called within {timeout_ms}ms"
                    )));
                }
                if let Some(error) = execution.take_error() {
                    break Err(LanpathError::Script(error));
                }
                ctx.with(|js_ctx| -> LanpathResult<()> {
                    if let Some(promise) = top.as_ref() {
                        let promise = promise.clone().restore(&js_ctx)
                            .map_err(|error| LanpathError::Script(format!("promise restore failed: {error}")))?;
                        if let Some(result) = promise.result::<rquickjs::Value>() {
                            drop(top.take());
                            if let Err(error) = result {
                                let message = if js_ctx.has_exception() {
                                    api::value_to_string(&js_ctx, js_ctx.catch())
                                } else { error.to_string() };
                                return Err(LanpathError::Script(format!("top-level promise rejected: {message}")));
                            }
                        }
                    }
                    Ok(())
                }).await?;

                let remaining = deadline.saturating_duration_since(Instant::now());
                let wait = timers_inner.borrow().next_due().map_or(remaining,
                    |due| due.saturating_duration_since(Instant::now()).min(remaining));
                // execute_pending_job executes exactly one QuickJS job, or polls
                // host futures when the JS queue is empty. Retain their registered
                // waker when no progress is possible instead of busy-polling HTTP.
                let progress = std::future::poll_fn(|task_ctx| {
                    let job = rt.execute_pending_job();
                    tokio::pin!(job);
                    match job.poll(task_ctx) {
                        Poll::Ready(Ok(false)) => Poll::Pending,
                        result => result,
                    }
                });
                tokio::select! {
                    biased;
                    _ = run_cancellation.cancelled() => {}
                    _ = execution.changed() => {}
                    res = &mut done_rx => {
                        break Ok(res.unwrap_or_default());
                    }
                    job = progress => {
                        if let Err(error) = job {
                            // Unlike Ctx::execute_pending_job's bool, this API
                            // preserves the throwing context and the -1 result.
                            let message = error.0.with(|js_ctx| {
                                api::value_to_string(&js_ctx, js_ctx.catch())
                            }).await;
                            execution.fail(format!("microtask callback failed: {message}"));
                        }
                    }
                    _ = tokio::time::sleep(wait) => {
                        ctx.with(|js_ctx| {
                            if !execution.active() { return; }
                            let due = timers_inner.borrow_mut().take_due(Instant::now());
                            if let Some((id, callback)) = due {
                                let called = callback.restore(&js_ctx)
                                    .and_then(|function| function.call::<_, ()>(()));
                                if let Err(error) = called {
                                    let message = if js_ctx.has_exception() {
                                        api::value_to_string(&js_ctx, js_ctx.catch())
                                    } else { error.to_string() };
                                    execution.fail(format!("timer callback failed: {message}"));
                                }
                                if execution.active() {
                                    timers_inner.borrow_mut().reschedule(id);
                                }
                            }
                        }).await;
                    }
                }
            }
        }.await;
        execution.cancel();
        // Cleanup may settle host Promises, but must never execute more user work.
        rt.set_interrupt_handler(Some(Box::new(|| true))).await;

        // 所有返回路径：在锁内清空 timer 注册表（drop 全部 Persistent），
        // 再强制 GC 与 idle，避免 runtime 释放时 gc_obj_list 断言失败。
        ctx.with(|js_ctx| {
            drop(top.take());
            timers.borrow_mut().clear();
            js_ctx.run_gc();
        })
        .await;
        rt.idle().await;

        self.cancellation = Arc::new(crate::cancellation::Cancellation::default());
        result
    }
}

#[cfg(test)]
#[path = "engine_quickjs_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "runtime_contract_tests.rs"]
mod runtime_contract_tests;
