//! 「连接」页响应式状态与后台轮询、筛选、断开及一键写规则编排。

use std::collections::BTreeSet;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lanpath_client::ConnectionView;
use nsh_core::prelude::*;

use crate::app_state;
use crate::connections::{ConnectionsService, ConnectionsSnapshot};
use crate::ui::feedback::show_toast;

const REFRESH_INTERVAL: Duration = Duration::from_secs(1);
const WORKER_POLL: Duration = Duration::from_millis(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConnTab {
    Active,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConnSort {
    Time,
    Total,
    Download,
    Upload,
}

enum AsyncOutcome {
    Snapshot(ConnectionsSnapshot),
    ClosedOne,
    ClosedAll,
    Cleared,
}

#[derive(Default)]
struct ResultSlot {
    done: bool,
    outcome: Option<Result<AsyncOutcome, String>>,
}

#[derive(Clone)]
pub(super) struct ConnectionsState {
    pub(super) service: Option<ConnectionsService>,
    pub(super) owner: Owner,
    pub(super) tab: Signal<ConnTab>,
    pub(super) search: Signal<String>,
    pub(super) outbound_filter: Signal<String>,
    pub(super) sort: Signal<ConnSort>,
    pub(super) paused: Signal<bool>,
    pub(super) active: Signal<Vec<ConnectionView>>,
    pub(super) closed: Signal<Vec<ConnectionView>>,
    pub(super) upload_total: Signal<u64>,
    pub(super) download_total: Signal<u64>,
    pub(super) clash_ready: Signal<bool>,
    pub(super) error: Signal<Option<String>>,
    tasks: Stored<Vec<Spawned<()>>>,
}

impl ConnectionsState {
    pub(super) fn new() -> Self {
        let opened = ConnectionsService::open();
        let initial_error = opened.as_ref().err().map(ToString::to_string);
        Self {
            service: opened.ok(),
            owner: Owner::current(),
            tab: signal(ConnTab::Active),
            search: signal(String::new()),
            outbound_filter: signal(String::new()),
            sort: signal(ConnSort::Time),
            paused: signal(false),
            active: signal(Vec::new()),
            closed: signal(Vec::new()),
            upload_total: signal(0),
            download_total: signal(0),
            clash_ready: signal(false),
            error: signal(initial_error),
            tasks: stored(Vec::new()),
        }
    }

    /// 启动页面级 1 秒连接快照轮询（切离页面后随 owner 自动终止）。
    pub(super) fn start_polling(&self) {
        let Some(service) = self.service.clone() else {
            return;
        };
        let state = self.clone();
        self.owner.run(move || {
            let handle = spawn(async move {
                loop {
                    if !state.paused.get_untracked() {
                        let snap = service.snapshot().await;
                        state.apply_snapshot(snap);
                    }
                    tokio::time::sleep(REFRESH_INTERVAL).await;
                }
            });
            self.tasks.with(|list| list.push(handle));
        });
    }

    pub(super) fn refresh_now(&self) {
        let Some(service) = self.service.clone() else {
            return;
        };
        self.spawn_async(async move { Ok(AsyncOutcome::Snapshot(service.snapshot().await)) });
    }

    pub(super) fn close_one(&self, id: String) {
        let Some(service) = self.service.clone() else {
            return;
        };
        self.error.set(None);
        self.spawn_async(async move {
            service
                .close_connection(&id)
                .await
                .map(|()| AsyncOutcome::ClosedOne)
                .map_err(|e| e.to_string())
        });
    }

    pub(super) fn close_all(&self) {
        let Some(service) = self.service.clone() else {
            return;
        };
        self.error.set(None);
        self.spawn_async(async move {
            service
                .close_all_connections()
                .await
                .map(|()| AsyncOutcome::ClosedAll)
                .map_err(|e| e.to_string())
        });
    }

    pub(super) fn clear_history(&self) {
        let Some(service) = self.service.clone() else {
            return;
        };
        self.error.set(None);
        self.spawn_async(async move {
            service
                .clear_history()
                .await
                .map(|()| AsyncOutcome::Cleared)
                .map_err(|e| e.to_string())
        });
    }

    pub(super) fn quick_add_rule(&self, conn: &ConnectionView, action: &'static str) {
        let Some(service) = self.service.as_ref() else {
            self.error.set(Some(t!("subscription.root_unavailable")));
            return;
        };
        match service.quick_add_rule(conn, action) {
            Ok(outcome) => {
                self.error.set(None);
                if outcome.already_exists {
                    show_toast(t!("connections.rule_exists", rule = outcome.line));
                } else {
                    show_toast(t!("connections.rule_added", rule = outcome.line));
                }
            }
            Err(err) => {
                self.error.set(Some(err.to_string()));
            }
        }
    }

    /// 返回当前页签内出现过的去重出站列表（供快捷筛选芯片使用）。
    pub(super) fn distinct_outbounds(&self) -> Vec<String> {
        let source = match self.tab.get() {
            ConnTab::Active => self.active,
            ConnTab::Closed => self.closed,
        };
        source.with(|list| {
            let mut set = BTreeSet::new();
            for item in list {
                let tag = item.outbound.trim();
                if !tag.is_empty() {
                    set.insert(tag.to_owned());
                }
            }
            set.into_iter().collect()
        })
    }

    /// 按页签、搜索词、出站过滤与排序规则计算当前展示列表。
    pub(super) fn filtered_list(&self) -> Vec<ConnectionView> {
        let source = match self.tab.get() {
            ConnTab::Active => self.active,
            ConnTab::Closed => self.closed,
        };
        let query = self.search.get().trim().to_ascii_lowercase();
        let outbound = self.outbound_filter.get();
        let sort = self.sort.get();
        let mut items: Vec<ConnectionView> = source.with(|list| {
            list.iter()
                .filter(|c| {
                    if !outbound.is_empty() && c.outbound != outbound {
                        return false;
                    }
                    if query.is_empty() {
                        return true;
                    }
                    c.host.to_ascii_lowercase().contains(&query)
                        || c.target.to_ascii_lowercase().contains(&query)
                        || c.destination_ip.to_ascii_lowercase().contains(&query)
                        || c.rule.to_ascii_lowercase().contains(&query)
                        || c.rule_payload.to_ascii_lowercase().contains(&query)
                        || c.outbound.to_ascii_lowercase().contains(&query)
                        || c.chain.to_ascii_lowercase().contains(&query)
                        || c.process.to_ascii_lowercase().contains(&query)
                        || c.network.to_ascii_lowercase().contains(&query)
                })
                .cloned()
                .collect()
        });
        items.sort_by(|a, b| match sort {
            ConnSort::Time => b.start.cmp(&a.start).then_with(|| b.id.cmp(&a.id)),
            ConnSort::Total => (b.upload.saturating_add(b.download))
                .cmp(&(a.upload.saturating_add(a.download)))
                .then_with(|| b.start.cmp(&a.start)),
            ConnSort::Download => b
                .download
                .cmp(&a.download)
                .then_with(|| b.start.cmp(&a.start)),
            ConnSort::Upload => b.upload.cmp(&a.upload).then_with(|| b.start.cmp(&a.start)),
        });
        items
    }

    fn apply_snapshot(&self, snap: ConnectionsSnapshot) {
        self.active.set(snap.active);
        self.closed.set(snap.closed);
        self.upload_total.set(snap.upload_total);
        self.download_total.set(snap.download_total);
        self.clash_ready.set(snap.clash_ready);
    }

    fn apply_outcome(&self, outcome: Result<AsyncOutcome, String>) {
        match outcome {
            Ok(AsyncOutcome::Snapshot(snap)) => self.apply_snapshot(snap),
            Ok(AsyncOutcome::ClosedOne) => {
                show_toast(t!("connections.closed_one"));
                self.refresh_now();
            }
            Ok(AsyncOutcome::ClosedAll) => {
                show_toast(t!("connections.closed_all"));
                self.refresh_now();
            }
            Ok(AsyncOutcome::Cleared) => {
                self.closed.set(Vec::new());
                app_state::refresh_traffic_stats(&app_state::current());
                show_toast(t!("connections.cleared"));
            }
            Err(err) => {
                self.error.set(Some(err));
            }
        }
    }

    fn spawn_async<F>(&self, future: F)
    where
        F: Future<Output = Result<AsyncOutcome, String>> + Send + 'static,
    {
        let slot = Arc::new(Mutex::new(ResultSlot::default()));
        let state = self.clone();
        self.owner.run(move || {
            let handle = spawn(async move {
                let worker_slot = slot.clone();
                let _worker = background(async move {
                    let result = future.await;
                    if let Ok(mut guard) = worker_slot.lock() {
                        guard.outcome = Some(result);
                        guard.done = true;
                    }
                });
                loop {
                    let (done, outcome) = match slot.lock() {
                        Ok(mut guard) => (guard.done, guard.outcome.take()),
                        Err(_) => (true, Some(Err("任务状态锁中毒".to_owned()))),
                    };
                    if done {
                        if let Some(res) = outcome {
                            state.apply_outcome(res);
                        }
                        break;
                    }
                    tokio::time::sleep(WORKER_POLL).await;
                }
            });
            self.tasks.with(|list| list.push(handle));
        });
    }
}
