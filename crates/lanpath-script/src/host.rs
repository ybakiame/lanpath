use std::sync::Arc;

use crate::types::{HttpRequestSpec, HttpResponseData};
use lanpath_common::{LanpathError, LanpathResult};

/// 执行 HTTP 请求的能力（网络仅经由此 trait，后续可审计/代理）。
#[async_trait::async_trait]
pub trait HttpExecutor: Send + Sync {
    /// A fresh execution session; only stateless executors may return themselves.
    fn session(self: Arc<Self>) -> Arc<dyn HttpExecutor>;

    async fn execute(&self, req: HttpRequestSpec) -> LanpathResult<HttpResponseData>;
}

/// Persistent storage operations report errors rather than acknowledging lost writes.
pub trait PersistentStore: Send + Sync {
    fn read(&self, scope: &str, key: &str) -> LanpathResult<Option<String>>;
    fn write(&self, scope: &str, key: &str, value: &str) -> LanpathResult<()>;
    fn erase(&self, scope: &str, key: &str) -> LanpathResult<bool>;
    fn clear(&self, scope: &str) -> LanpathResult<()>;
    fn clear_all(&self) -> LanpathResult<()>;
}

/// 通知能力。
pub trait Notifier: Send + Sync {
    fn notify(&self, title: &str, subtitle: &str, body: &str, options: Option<serde_json::Value>);
}

/// 脚本宿主：脚本运行时可用的全部外部能力集合。
pub struct ScriptHost {
    pub http: Arc<dyn HttpExecutor>,
    pub store: Arc<dyn PersistentStore>,
    pub notifier: Arc<dyn Notifier>,
}

/// One runtime-owned callback. Repeating callbacks remain registered while
/// executing so clearing the currently running interval prevents rescheduling.
pub struct TimerEntry {
    id: u32,
    due: std::time::Instant,
    period: Option<std::time::Duration>,
    callback: rquickjs::Persistent<rquickjs::Function<'static>>,
}

#[derive(Default)]
pub struct TimerState {
    next_id: u32,
    entries: Vec<TimerEntry>,
}

/// Access and Persistent destruction stay inside the single-threaded runtime lock.
pub type TimerRegistry = std::rc::Rc<std::cell::RefCell<TimerState>>;

impl TimerState {
    pub(crate) fn schedule(
        &mut self,
        callback: rquickjs::Persistent<rquickjs::Function<'static>>,
        delay: std::time::Duration,
        repeat: bool,
    ) -> Result<u32, &'static str> {
        if self.entries.len() >= 64 {
            return Err("too many pending timers");
        }
        let id = self
            .next_id
            .checked_add(1)
            .ok_or("timer handles exhausted")?;
        self.next_id = id;
        self.entries.push(TimerEntry {
            id,
            due: std::time::Instant::now() + delay,
            period: repeat.then_some(delay),
            callback,
        });
        Ok(id)
    }
    pub(crate) fn cancel(&mut self, id: u32) {
        if let Some(ix) = self.entries.iter().position(|entry| entry.id == id) {
            self.entries.swap_remove(ix);
        }
    }
    pub(crate) fn take_due(
        &mut self,
        now: std::time::Instant,
    ) -> Option<(u32, rquickjs::Persistent<rquickjs::Function<'static>>)> {
        let ix = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.due <= now)
            .min_by_key(|(_, entry)| (entry.due, entry.id))
            .map(|(ix, _)| ix)?;
        if self.entries[ix].period.is_some() {
            Some((self.entries[ix].id, self.entries[ix].callback.clone()))
        } else {
            let entry = self.entries.swap_remove(ix);
            Some((entry.id, entry.callback))
        }
    }
    pub(crate) fn reschedule(&mut self, id: u32) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id)
            && let Some(period) = entry.period
        {
            entry.due = std::time::Instant::now() + period;
        }
    }
    pub(crate) fn next_due(&self) -> Option<std::time::Instant> {
        self.entries.iter().map(|entry| entry.due).min()
    }
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

impl ScriptHost {
    pub fn new(
        http: Arc<dyn HttpExecutor>,
        store: Arc<dyn PersistentStore>,
        notifier: Arc<dyn Notifier>,
    ) -> Self {
        Self {
            http,
            store,
            notifier,
        }
    }
}

/// 基于内存 HashMap 的持久化存储实现（测试/默认用途）。
#[derive(Debug, Default)]
pub struct MemoryPersistentStore {
    inner: std::sync::RwLock<std::collections::HashMap<(String, String), String>>,
}

impl MemoryPersistentStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PersistentStore for MemoryPersistentStore {
    fn read(&self, scope: &str, key: &str) -> LanpathResult<Option<String>> {
        let inner = self.inner.read().map_err(|_| store_poisoned())?;
        Ok(inner.get(&(scope.to_string(), key.to_string())).cloned())
    }

    fn write(&self, scope: &str, key: &str, value: &str) -> LanpathResult<()> {
        let mut inner = self.inner.write().map_err(|_| store_poisoned())?;
        inner.insert((scope.to_string(), key.to_string()), value.to_string());
        Ok(())
    }

    fn erase(&self, scope: &str, key: &str) -> LanpathResult<bool> {
        let mut inner = self.inner.write().map_err(|_| store_poisoned())?;
        Ok(inner
            .remove(&(scope.to_string(), key.to_string()))
            .is_some())
    }

    fn clear(&self, scope: &str) -> LanpathResult<()> {
        let mut inner = self.inner.write().map_err(|_| store_poisoned())?;
        inner.retain(|(stored_scope, _), _| stored_scope != scope);
        Ok(())
    }
    fn clear_all(&self) -> LanpathResult<()> {
        self.inner.write().map_err(|_| store_poisoned())?.clear();
        Ok(())
    }
}

fn store_poisoned() -> LanpathError {
    LanpathError::Internal("persistent store lock poisoned".to_string())
}

pub use crate::persistent_store::FilePersistentStore;

/// 记录所有通知调用历史的 Notifier（测试钩子）。
#[derive(Debug, Default)]
pub struct RecordingNotifier {
    pub calls: std::sync::Mutex<Vec<Notification>>,
}

/// 一次通知记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub subtitle: String,
    pub body: String,
    pub options: Option<serde_json::Value>,
}

impl RecordingNotifier {
    pub fn new() -> Self {
        Self::default()
    }

    /// 读取通知调用历史（测试钩子）。
    pub fn calls(&self) -> Vec<Notification> {
        self.calls.lock().map(|c| c.clone()).unwrap_or_default()
    }
}

impl Notifier for RecordingNotifier {
    fn notify(&self, title: &str, subtitle: &str, body: &str, options: Option<serde_json::Value>) {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(Notification {
                title: title.to_string(),
                subtitle: subtitle.to_string(),
                body: body.to_string(),
                options,
            });
        }
    }
}

/// 预设响应队列的 HttpExecutor（测试用，不做真实网络请求）。
#[derive(Debug)]
pub struct MockHttpExecutor {
    queue: tokio::sync::Mutex<std::collections::VecDeque<HttpResponseData>>,
}

impl MockHttpExecutor {
    /// 按顺序出队响应；队列为空时返回 404 空响应。
    pub fn with_responses(responses: Vec<HttpResponseData>) -> Self {
        Self {
            queue: tokio::sync::Mutex::new(responses.into()),
        }
    }
}

#[async_trait::async_trait]
impl HttpExecutor for MockHttpExecutor {
    fn session(self: Arc<Self>) -> Arc<dyn HttpExecutor> {
        self
    }

    async fn execute(&self, _req: HttpRequestSpec) -> LanpathResult<HttpResponseData> {
        let mut queue = self.queue.lock().await;
        match queue.pop_front() {
            Some(resp) => Ok(resp),
            None => Err(LanpathError::Internal("mock http queue empty".to_string())),
        }
    }
}
