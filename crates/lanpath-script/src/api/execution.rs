use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

use rquickjs::Ctx;
use tokio::sync::Notify;

/// One run owns every host future. Cancellation drops the executor future rather
/// than merely abandoning its JavaScript Promise before runtime shutdown.
#[derive(Clone)]
pub(crate) struct Execution(Rc<State>);

struct State {
    cancelled: Cell<bool>,
    completed: Arc<AtomicBool>,
    deadline: Instant,
    changed: Notify,
    error: RefCell<Option<String>>,
    cancellation: std::sync::Arc<crate::cancellation::Cancellation>,
}

impl Execution {
    pub(crate) fn new(
        deadline: Instant,
        cancellation: std::sync::Arc<crate::cancellation::Cancellation>,
    ) -> Self {
        Self(Rc::new(State {
            cancelled: Cell::new(false),
            completed: Arc::new(AtomicBool::new(false)),
            deadline,
            changed: Notify::new(),
            error: RefCell::new(None),
            cancellation,
        }))
    }

    pub(crate) fn completion_flag(&self) -> Arc<AtomicBool> {
        self.0.completed.clone()
    }

    pub(crate) fn completed(&self) -> bool {
        self.0.completed.load(Ordering::Acquire)
    }

    pub(crate) fn complete(&self) {
        self.0.completed.store(true, Ordering::Release);
        self.0.changed.notify_waiters();
    }

    pub(crate) fn active(&self) -> bool {
        !self.completed()
            && !self.cancelled()
            && Instant::now() < self.0.deadline
            && self.0.error.borrow().is_none()
    }

    pub(crate) fn check(&self, ctx: &Ctx<'_>) -> rquickjs::Result<()> {
        if self.active() {
            return Ok(());
        }
        let message = if self.completed() {
            "script execution already completed"
        } else if self.cancelled() {
            "script execution cancelled"
        } else if Instant::now() >= self.0.deadline {
            "script execution deadline exceeded"
        } else {
            "script execution failed"
        };
        Err(rquickjs::Exception::throw_message(ctx, message))
    }

    pub(crate) fn cancel(&self) {
        self.0.cancelled.set(true);
        self.0.cancellation.cancel();
        self.0.changed.notify_waiters();
    }

    pub(crate) fn cancelled(&self) -> bool {
        self.0.cancelled.get() || self.0.cancellation.is_cancelled()
    }

    pub(crate) fn fail(&self, message: String) {
        self.0.error.borrow_mut().get_or_insert(message);
        self.0.changed.notify_one();
    }

    pub(crate) fn callback_error(&self, ctx: &Ctx<'_>, error: rquickjs::Error) {
        let message = if ctx.has_exception() {
            super::value_to_string(ctx, ctx.catch())
        } else {
            error.to_string()
        };
        self.fail(format!("HTTP callback failed: {message}"));
    }

    pub(crate) fn take_error(&self) -> Option<String> {
        self.0.error.borrow_mut().take()
    }

    pub(crate) async fn changed(&self) {
        self.0.changed.notified().await;
    }

    pub(crate) async fn run<F: Future>(&self, future: F) -> Option<F::Output> {
        if !self.active() {
            return None;
        }
        tokio::select! {
            biased;
            _ = self.0.cancellation.cancelled() => None,
            _ = self.changed() => None,
            _ = tokio::time::sleep_until(self.0.deadline.into()) => None,
            result = future => self.active().then_some(result),
        }
    }
}
