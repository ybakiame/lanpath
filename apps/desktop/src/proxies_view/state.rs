//! 运行时「策略与节点」页响应式状态与异步 Clash API 编排。

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lanpath_client::{DelayResult, GroupView, NodeView};
use nsh_core::prelude::*;

use crate::proxies::{LiveProxiesSnapshot, ProxiesService};
use crate::ui::feedback::show_toast;

const POLL_INTERVAL: Duration = Duration::from_millis(60);

/// 节点实时测速状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DelayCell {
    Testing,
    Measured(u16),
    Timeout,
}

enum AsyncOutcome {
    Refreshed(LiveProxiesSnapshot),
    Selected {
        group: String,
        member: String,
    },
    NodeTested {
        name: String,
        delay: Option<u16>,
    },
    GroupTested {
        group: String,
        results: Vec<DelayResult>,
    },
    ModeChanged(String),
}

#[derive(Default)]
struct ResultSlot {
    done: bool,
    outcome: Option<Result<AsyncOutcome, String>>,
}

#[derive(Clone)]
pub(super) struct ProxiesState {
    pub(super) service: Option<ProxiesService>,
    pub(super) owner: Owner,
    pub(super) groups: Signal<Vec<GroupView>>,
    pub(super) node_map: Signal<HashMap<String, NodeView>>,
    pub(super) delays: Signal<HashMap<String, DelayCell>>,
    pub(super) selected_group: Signal<String>,
    pub(super) mode: Signal<String>,
    pub(super) search: Signal<String>,
    pub(super) sort_by_delay: Signal<bool>,
    pub(super) loading: Signal<bool>,
    pub(super) testing_group: Signal<bool>,
    pub(super) live_ready: Signal<bool>,
    pub(super) error: Signal<Option<String>>,
    tasks: Stored<Vec<Spawned<()>>>,
}

impl ProxiesState {
    pub(super) fn new() -> Self {
        let opened = ProxiesService::open();
        let (saved_mode, initial_error) = match &opened {
            Ok(svc) => (svc.load_saved_rule_mode(), None),
            Err(err) => ("rule".to_owned(), Some(err.to_string())),
        };
        Self {
            service: opened.ok(),
            owner: Owner::current(),
            groups: signal(Vec::new()),
            node_map: signal(HashMap::new()),
            delays: signal(HashMap::new()),
            selected_group: signal(String::new()),
            mode: signal(saved_mode),
            search: signal(String::new()),
            sort_by_delay: signal(false),
            loading: signal(false),
            testing_group: signal(false),
            live_ready: signal(false),
            error: signal(initial_error),
            tasks: stored(Vec::new()),
        }
    }

    pub(super) fn clash_api_configured(&self) -> bool {
        self.service
            .as_ref()
            .is_none_or(ProxiesService::saved_clash_api_enabled)
    }

    pub(super) fn active_group(&self) -> Option<GroupView> {
        let wanted = self.selected_group.get();
        self.groups.with(|list| {
            list.iter()
                .find(|g| g.name == wanted)
                .or_else(|| list.first())
                .cloned()
        })
    }

    pub(super) fn clear_live(&self) {
        self.live_ready.set(false);
        self.groups.set(Vec::new());
        self.node_map.set(HashMap::new());
        self.delays.set(HashMap::new());
        self.loading.set(false);
        self.testing_group.set(false);
    }

    pub(super) fn refresh(&self) {
        let Some(service) = self.service.clone() else {
            self.error.set(Some(t!("subscription.root_unavailable")));
            return;
        };
        if service.active_endpoint().is_none() {
            self.clear_live();
            return;
        }
        self.loading.set(true);
        self.error.set(None);
        self.spawn_async(async move {
            service
                .fetch_live_proxies()
                .await
                .map(AsyncOutcome::Refreshed)
                .map_err(|e| e.to_string())
        });
    }

    pub(super) fn select_member(&self, group: String, member: String) {
        let Some(service) = self.service.clone() else {
            return;
        };
        self.error.set(None);
        let g = group.clone();
        let m = member.clone();
        self.spawn_async(async move {
            service
                .select_proxy(&g, &m)
                .await
                .map(|()| AsyncOutcome::Selected { group, member })
                .map_err(|e| e.to_string())
        });
    }

    pub(super) fn test_node(&self, name: String) {
        let Some(service) = self.service.clone() else {
            return;
        };
        self.delays.update(|map| {
            map.insert(name.clone(), DelayCell::Testing);
        });
        let target = name.clone();
        self.spawn_async(async move {
            service
                .test_node_delay(&target)
                .await
                .map(|delay| AsyncOutcome::NodeTested { name, delay })
                .map_err(|e| e.to_string())
        });
    }

    pub(super) fn test_active_group(&self) {
        let Some(group) = self.active_group() else {
            return;
        };
        let Some(service) = self.service.clone() else {
            return;
        };
        self.testing_group.set(true);
        self.error.set(None);
        let members = group.members.clone();
        self.delays.update(|map| {
            for member in &members {
                map.insert(member.clone(), DelayCell::Testing);
            }
        });
        let group_name = group.name;
        let group_arg = group_name.clone();
        self.spawn_async(async move {
            service
                .test_group_delay(&group_arg)
                .await
                .map(|results| AsyncOutcome::GroupTested {
                    group: group_name,
                    results,
                })
                .map_err(|e| e.to_string())
        });
    }

    pub(super) fn set_mode(&self, next_mode: &str) {
        let Some(service) = self.service.clone() else {
            return;
        };
        let target = next_mode.to_owned();
        self.mode.set(target.clone());
        self.error.set(None);
        self.spawn_async(async move {
            service
                .set_rule_mode(&target)
                .await
                .map(AsyncOutcome::ModeChanged)
                .map_err(|e| e.to_string())
        });
    }

    fn apply_outcome(&self, outcome: Result<AsyncOutcome, String>) {
        match outcome {
            Ok(AsyncOutcome::Refreshed(snapshot)) => {
                self.loading.set(false);
                self.live_ready.set(true);
                self.mode.set(snapshot.mode);
                let mut nodes = HashMap::with_capacity(snapshot.nodes.len());
                let mut next_delays = self.delays.get_untracked();
                for node in snapshot.nodes {
                    if let Some(ms) = node.delay_ms {
                        next_delays.insert(node.name.clone(), DelayCell::Measured(ms));
                    }
                    nodes.insert(node.name.clone(), node);
                }
                self.delays.set(next_delays);
                self.node_map.set(nodes);
                let current_sel = self.selected_group.get_untracked();
                let still_exists = snapshot.groups.iter().any(|g| g.name == current_sel);
                if !still_exists && let Some(first) = snapshot.groups.first() {
                    self.selected_group.set(first.name.clone());
                }
                self.groups.set(snapshot.groups);
                self.error.set(None);
            }
            Ok(AsyncOutcome::Selected { group, member }) => {
                self.groups.update(|list| {
                    if let Some(entry) = list.iter_mut().find(|g| g.name == group) {
                        entry.now.clone_from(&member);
                    }
                });
                show_toast(t!("proxies.switched", group = group, member = member));
                self.error.set(None);
            }
            Ok(AsyncOutcome::NodeTested { name, delay }) => {
                self.delays.update(|map| {
                    map.insert(
                        name,
                        match delay {
                            Some(ms) => DelayCell::Measured(ms),
                            None => DelayCell::Timeout,
                        },
                    );
                });
            }
            Ok(AsyncOutcome::GroupTested { group, results }) => {
                self.testing_group.set(false);
                let mut ok_count = 0usize;
                self.delays.update(|map| {
                    for res in &results {
                        if let Some(ms) = res.delay_ms {
                            ok_count += 1;
                            map.insert(res.name.clone(), DelayCell::Measured(ms));
                        } else {
                            map.insert(res.name.clone(), DelayCell::Timeout);
                        }
                    }
                });
                show_toast(t!(
                    "proxies.group_tested",
                    group = group,
                    ok = ok_count,
                    total = results.len()
                ));
            }
            Ok(AsyncOutcome::ModeChanged(mode)) => {
                self.mode.set(mode.clone());
                let label = mode_display_label(&mode);
                show_toast(t!("proxies.mode_updated", mode = label));
                self.error.set(None);
            }
            Err(err) => {
                self.loading.set(false);
                self.testing_group.set(false);
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
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
            });
            self.tasks.with(|list| list.push(handle));
        });
    }
}

pub(super) fn mode_display_label(mode: &str) -> String {
    match mode {
        "global" => t!("proxies.mode.global"),
        "direct" => t!("proxies.mode.direct"),
        _ => t!("proxies.mode.rule"),
    }
}
