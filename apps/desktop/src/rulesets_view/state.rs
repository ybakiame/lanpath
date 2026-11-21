//! 规则集工作台响应式状态与异步下载/更新编排。

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lanpath_client::rulesets::{
    MarketMirror, MarketRuleSetSpec, RuleSetEntry, RuleSetFormat, RuleSetGroup, RuleSetRemoteState,
    RuleSetSource, RuleSetUpdateOutcome, RuleSetUpdateStatus,
};
use nsh_core::prelude::*;

use crate::rulesets::{RuleSetRow, RuleSetsService, RuleSetsSnapshot};

const WORK_POLL: Duration = Duration::from_millis(100);

pub(super) const DEFAULT_MANUAL_JSON: &str = "{\n  \"version\": 2,\n  \"rules\": [\n    {\n      \"domain_suffix\": [\"example.com\"]\n    }\n  ]\n}\n";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RulesetsTab {
    Managed,
    Market,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum DraftSourceKind {
    Remote,
    Manual,
}

enum WorkResult {
    One {
        tag: String,
        status: RuleSetUpdateStatus,
        snapshot: RuleSetsSnapshot,
    },
    All {
        outcome: RuleSetUpdateOutcome,
        snapshot: RuleSetsSnapshot,
    },
}

#[derive(Default)]
struct WorkSlot {
    finished: bool,
    outcome: Option<Result<WorkResult, String>>,
}

#[derive(Clone)]
pub(super) struct RulesetsState {
    pub(super) service: Option<RuleSetsService>,
    pub(super) owner: Owner,
    pub(super) tab: Signal<RulesetsTab>,
    pub(super) items: Signal<Vec<RuleSetRow>>,
    pub(super) groups: Signal<Vec<RuleSetGroup>>,
    pub(super) search: Signal<String>,
    pub(super) category: Signal<String>,
    pub(super) mirror: Signal<MarketMirror>,
    pub(super) busy_id: Signal<Option<String>>,
    pub(super) busy_all: Signal<bool>,
    pub(super) error: Signal<Option<String>>,
    pub(super) notice: Signal<Option<String>>,
    pub(super) confirm_delete: Signal<Option<(String, String)>>,
    pub(super) editor_open: Signal<bool>,
    pub(super) editing_id: Signal<Option<String>>,
    pub(super) draft_tag: Signal<String>,
    pub(super) draft_name: Signal<String>,
    pub(super) draft_category: Signal<String>,
    pub(super) draft_source_kind: Signal<DraftSourceKind>,
    pub(super) draft_url: Signal<String>,
    pub(super) draft_format: Signal<RuleSetFormat>,
    pub(super) draft_manual: Signal<String>,
    task: Stored<Option<Spawned<()>>>,
}

impl RulesetsState {
    pub(super) fn new() -> Self {
        let opened = RuleSetsService::open();
        let (snapshot, error) = match &opened {
            Ok(service) => match service.list_snapshot() {
                Ok(snap) => (snap, None),
                Err(err) => (RuleSetsSnapshot::default(), Some(err.to_string())),
            },
            Err(err) => (RuleSetsSnapshot::default(), Some(err.to_string())),
        };
        Self {
            service: opened.ok(),
            owner: Owner::current(),
            tab: signal(RulesetsTab::Managed),
            items: signal(snapshot.items),
            groups: signal(snapshot.groups),
            search: signal(String::new()),
            category: signal("all".to_owned()),
            mirror: signal(MarketMirror::JsDelivr),
            busy_id: signal(None),
            busy_all: signal(false),
            error: signal(error),
            notice: signal(None),
            confirm_delete: signal(None),
            editor_open: signal(false),
            editing_id: signal(None),
            draft_tag: signal(String::new()),
            draft_name: signal(String::new()),
            draft_category: signal("custom".to_owned()),
            draft_source_kind: signal(DraftSourceKind::Remote),
            draft_url: signal(String::new()),
            draft_format: signal(RuleSetFormat::Binary),
            draft_manual: signal(DEFAULT_MANUAL_JSON.to_owned()),
            task: stored(None),
        }
    }

    fn apply_snapshot(&self, snapshot: RuleSetsSnapshot) {
        self.items.set(snapshot.items);
        self.groups.set(snapshot.groups);
    }

    pub(super) fn is_busy(&self) -> bool {
        self.busy_all.get() || self.busy_id.get().is_some()
    }

    pub(super) fn open_add(&self) {
        self.editing_id.set(None);
        self.draft_tag.set(String::new());
        self.draft_name.set(String::new());
        self.draft_category.set("custom".to_owned());
        self.draft_source_kind.set(DraftSourceKind::Remote);
        self.draft_url.set(String::new());
        self.draft_format.set(RuleSetFormat::Binary);
        self.draft_manual.set(DEFAULT_MANUAL_JSON.to_owned());
        self.error.set(None);
        self.editor_open.set(true);
    }

    pub(super) fn open_edit(&self, entry: &RuleSetEntry) {
        self.editing_id.set(Some(entry.id.clone()));
        self.draft_tag.set(entry.tag.clone());
        self.draft_name.set(entry.name.clone());
        self.draft_category.set(if entry.category.is_empty() {
            "custom".to_owned()
        } else {
            entry.category.clone()
        });
        match &entry.source {
            RuleSetSource::Remote { url, format } => {
                self.draft_source_kind.set(DraftSourceKind::Remote);
                self.draft_url.set(url.clone());
                self.draft_format.set(*format);
                self.draft_manual.set(DEFAULT_MANUAL_JSON.to_owned());
            }
            RuleSetSource::Manual { content } => {
                self.draft_source_kind.set(DraftSourceKind::Manual);
                self.draft_url.set(String::new());
                self.draft_format.set(RuleSetFormat::Source);
                self.draft_manual.set(content.clone());
            }
        }
        self.error.set(None);
        self.editor_open.set(true);
    }

    pub(super) fn close_editor(&self) {
        self.editor_open.set(false);
        self.editing_id.set(None);
    }

    pub(super) fn save_editor(&self) {
        let Some(service) = &self.service else {
            self.error.set(Some(t!("subscription.root_unavailable")));
            return;
        };
        let source = match self.draft_source_kind.get() {
            DraftSourceKind::Remote => RuleSetSource::Remote {
                url: self.draft_url.get(),
                format: self.draft_format.get(),
            },
            DraftSourceKind::Manual => RuleSetSource::Manual {
                content: self.draft_manual.get(),
            },
        };
        let entry = RuleSetEntry {
            id: self.editing_id.get().unwrap_or_default(),
            tag: self.draft_tag.get(),
            name: self.draft_name.get(),
            category: self.draft_category.get(),
            source,
            remote_state: RuleSetRemoteState::default(),
            created_at: 0,
        };
        match service.upsert(entry) {
            Ok(snapshot) => {
                self.apply_snapshot(snapshot);
                self.error.set(None);
                self.notice.set(Some(t!("rulesets.saved")));
                self.close_editor();
            }
            Err(err) => self.error.set(Some(err.to_string())),
        }
    }

    pub(super) fn confirm_remove(&self, id: String) {
        let Some(service) = &self.service else {
            self.error.set(Some(t!("subscription.root_unavailable")));
            return;
        };
        match service.delete(&id) {
            Ok(snapshot) => {
                self.apply_snapshot(snapshot);
                self.confirm_delete.set(None);
                self.error.set(None);
                self.notice.set(Some(t!("rulesets.deleted")));
            }
            Err(err) => {
                self.confirm_delete.set(None);
                self.error.set(Some(err.to_string()));
            }
        }
    }

    pub(super) fn update_one(&self, id: String, tag: String) {
        if self.is_busy() {
            return;
        }
        let Some(service) = self.service.clone() else {
            self.error.set(Some(t!("subscription.root_unavailable")));
            return;
        };
        self.busy_id.set(Some(id.clone()));
        self.error.set(None);
        self.notice.set(None);
        self.spawn_work(async move {
            service
                .update_one_remote(&id)
                .await
                .map(|(status, snapshot)| WorkResult::One {
                    tag,
                    status,
                    snapshot,
                })
                .map_err(|err| err.to_string())
        });
    }

    pub(super) fn update_all(&self) {
        if self.is_busy() {
            return;
        }
        let Some(service) = self.service.clone() else {
            self.error.set(Some(t!("subscription.root_unavailable")));
            return;
        };
        self.busy_all.set(true);
        self.error.set(None);
        self.notice.set(None);
        self.spawn_work(async move {
            service
                .update_all_remote()
                .await
                .map(|(outcome, snapshot)| WorkResult::All { outcome, snapshot })
                .map_err(|err| err.to_string())
        });
    }

    pub(super) fn add_from_market(&self, spec: MarketRuleSetSpec) {
        if self.is_busy() {
            return;
        }
        let Some(service) = self.service.clone() else {
            self.error.set(Some(t!("subscription.root_unavailable")));
            return;
        };
        let zh = t!("nav.rulesets") == "规则集";
        let mirror = self.mirror.get();
        match service.add_from_market(spec, mirror, zh) {
            Ok((entry, snapshot)) => {
                self.apply_snapshot(snapshot);
                self.error.set(None);
                self.notice
                    .set(Some(t!("rulesets.market_added", tag = entry.tag)));
                self.update_one(entry.id, entry.tag);
            }
            Err(err) => self.error.set(Some(err.to_string())),
        }
    }

    fn spawn_work<F>(&self, future: F)
    where
        F: Future<Output = Result<WorkResult, String>> + Send + 'static,
    {
        let shared = Arc::new(Mutex::new(WorkSlot::default()));
        let done = self.clone();
        self.owner.run(move || {
            done.task.with(|slot| {
                let done_inner = done.clone();
                slot.replace(spawn(async move {
                    let worker_shared = shared.clone();
                    let _worker = background(async move {
                        let outcome = future.await;
                        if let Ok(mut guard) = worker_shared.lock() {
                            guard.outcome = Some(outcome);
                            guard.finished = true;
                        }
                    });
                    loop {
                        let (finished, outcome) = match shared.lock() {
                            Ok(mut guard) => (guard.finished, guard.outcome.take()),
                            Err(_) => (true, Some(Err("任务状态锁中毒".to_owned()))),
                        };
                        if finished {
                            done_inner.busy_id.set(None);
                            done_inner.busy_all.set(false);
                            match outcome {
                                Some(Ok(WorkResult::One {
                                    tag,
                                    status,
                                    snapshot,
                                })) => {
                                    done_inner.apply_snapshot(snapshot);
                                    done_inner.error.set(None);
                                    let msg = match status {
                                        RuleSetUpdateStatus::Updated => {
                                            t!("rulesets.update_done", tag = tag)
                                        }
                                        RuleSetUpdateStatus::Skipped => {
                                            t!("rulesets.update_up_to_date", tag = tag)
                                        }
                                        RuleSetUpdateStatus::Failed => {
                                            t!("rulesets.update_failed", tag = tag)
                                        }
                                    };
                                    done_inner.notice.set(Some(msg));
                                }
                                Some(Ok(WorkResult::All { outcome, snapshot })) => {
                                    done_inner.apply_snapshot(snapshot);
                                    done_inner.error.set(None);
                                    done_inner.notice.set(Some(t!(
                                        "rulesets.update_all_done",
                                        updated = outcome.updated,
                                        skipped = outcome.skipped,
                                        failed = outcome.failed
                                    )));
                                }
                                Some(Err(err)) => done_inner.error.set(Some(err)),
                                None => {}
                            }
                            break;
                        }
                        tokio::time::sleep(WORK_POLL).await;
                    }
                }));
            });
        });
    }
}

pub(super) const CATEGORIES: &[(&str, &str)] = &[
    ("all", "rulesets.cat.all"),
    ("cn", "rulesets.cat.cn"),
    ("proxy", "rulesets.cat.proxy"),
    ("ads", "rulesets.cat.ads"),
    ("ai", "rulesets.cat.ai"),
    ("media", "rulesets.cat.media"),
    ("custom", "rulesets.cat.custom"),
];

pub(super) fn category_label(category: &str) -> String {
    match category {
        "cn" => t!("rulesets.cat.cn"),
        "proxy" => t!("rulesets.cat.proxy"),
        "ads" => t!("rulesets.cat.ads"),
        "ai" => t!("rulesets.cat.ai"),
        "media" => t!("rulesets.cat.media"),
        "custom" => t!("rulesets.cat.custom"),
        _ => t!("rulesets.cat.custom"),
    }
}
