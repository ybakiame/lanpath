mod editing;
mod inputs;
mod market;
mod render;
mod render_market;
mod render_rules;
mod render_sets;
mod sync;

use std::{collections::HashSet, sync::Arc};

use gpui_kit::{
    AppContext as _, Context, EventEmitter, Subscription, Task, Window, component::WindowExt as _,
};
use lanpath_client::local_override::{LocalOverride, RuleSetUpdateOutcome};

use crate::{
    business::{BusinessService, LocalOverrideDocument},
    rules_view::inputs::FormInputs,
    runtime::DesktopRuntime,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Section {
    Rules,
    RuleSets,
    Market,
}

#[derive(Clone, Copy)]
pub(super) enum Operation {
    Save,
    UpdateAll,
    UpdateOne,
}

pub(crate) struct RulesChanged;

pub(crate) struct RulesView {
    core: Arc<DesktopRuntime>,
    service: Option<BusinessService>,
    saved: Option<LocalOverride>,
    draft: Option<LocalOverride>,
    cached_rule_sets: HashSet<String>,
    selected_rule: Option<String>,
    selected_set: Option<String>,
    category_selection: HashSet<String>,
    section: Section,
    inputs: FormInputs,
    syncing: bool,
    dirty: bool,
    locked: bool,
    operation: Option<Operation>,
    error: Option<String>,
    notice: Option<String>,
    task: Option<Task<()>>,
    market_group: market::MarketGroup,
    download_source: market::DownloadSource,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<RulesChanged> for RulesView {}

impl RulesView {
    pub(crate) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let service = core.business().ok().cloned();
        let document = service
            .as_ref()
            .ok_or_else(|| "实验数据目录不可用".to_string())
            .and_then(|service| service.local_override().map_err(|error| error.to_string()));
        let (inputs, subscriptions) = FormInputs::new(window, cx);
        let mut view = Self {
            core,
            service,
            saved: None,
            draft: None,
            cached_rule_sets: HashSet::new(),
            selected_rule: None,
            selected_set: None,
            category_selection: HashSet::new(),
            section: Section::Rules,
            inputs,
            syncing: false,
            dirty: false,
            locked: false,
            operation: None,
            error: None,
            notice: None,
            task: None,
            market_group: market::MarketGroup::All,
            download_source: market::DownloadSource::GitHub,
            _subscriptions: subscriptions,
        };
        match document {
            Ok(document) => view.apply_document(document, window, cx),
            Err(error) => view.error = Some(error),
        }
        view
    }

    pub(crate) fn dirty(&self) -> bool {
        self.dirty
    }

    pub(crate) fn busy(&self) -> bool {
        self.operation.is_some()
    }

    pub(crate) fn set_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        self.locked = locked;
        self.inputs.set_locked(locked, cx);
        cx.notify();
    }

    fn set_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        cx.notify();
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.dirty = self.draft != self.saved;
        self.error = None;
        self.notice = None;
        cx.notify();
    }

    fn apply_document(
        &mut self,
        document: LocalOverrideDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cached_rule_sets = document.cached_rule_sets;
        self.saved = Some(document.value.clone());
        self.draft = Some(document.value);
        self.selected_rule = self.selected_rule.take().filter(|id| {
            self.draft
                .as_ref()
                .is_some_and(|draft| draft.singbox.rules.iter().any(|rule| &rule.id == id))
        });
        if self.selected_rule.is_none() {
            self.selected_rule = self
                .draft
                .as_ref()
                .and_then(|draft| draft.singbox.rules.first())
                .map(|rule| rule.id.clone());
        }
        self.selected_set = self.selected_set.take().filter(|id| {
            self.draft
                .as_ref()
                .is_some_and(|draft| draft.custom_rule_sets.iter().any(|set| &set.id == id))
        });
        if self.selected_set.is_none() {
            self.selected_set = self
                .draft
                .as_ref()
                .and_then(|draft| draft.custom_rule_sets.first())
                .map(|set| set.id.clone());
        }
        self.category_selection.retain(|id| {
            self.draft
                .as_ref()
                .is_some_and(|draft| draft.custom_rule_sets.iter().any(|set| &set.id == id))
        });
        self.dirty = false;
        self.error = None;
        self.sync_inputs(window, cx);
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        if self.dirty {
            let view = cx.weak_entity();
            window.open_alert_dialog(cx, move |dialog, _, _| {
                let view = view.clone();
                dialog
                    .title("放弃规则修改？")
                    .description("重新载入会丢弃未保存的规则、规则集和分类修改。")
                    .confirm()
                    .ok_text("放弃修改")
                    .cancel_text("取消")
                    .on_ok(move |_, window, cx| {
                        let _ = view.update(cx, |view, cx| view.load_now(window, cx));
                        true
                    })
            });
        } else {
            self.load_now(window, cx);
        }
    }

    fn load_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self
            .service
            .as_ref()
            .ok_or_else(|| "实验数据目录不可用".to_string())
            .and_then(|service| service.local_override().map_err(|error| error.to_string()));
        match result {
            Ok(document) => {
                self.apply_document(document, window, cx);
                self.notice = Some("已重新载入规则配置".into());
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() || !self.dirty {
            return;
        }
        let (Some(service), Some(draft)) = (self.service.clone(), self.draft.clone()) else {
            return;
        };
        self.operation = Some(Operation::Save);
        self.error = None;
        self.notice = None;
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    service
                        .save_local_override(draft)
                        .map_err(|error| error.to_string())
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.operation = None;
                    match result {
                        Ok(document) => {
                            this.apply_document(document, window, cx);
                            this.notice =
                                Some("规则配置已保存；运行中的核心需停止后重新启动才能应用".into());
                            cx.emit(RulesChanged);
                        }
                        Err(error) => this.error = Some(error),
                    }
                    cx.notify();
                })
                .ok();
            });
        }));
        cx.notify();
    }

    fn update_rule_sets(
        &mut self,
        id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.locked || self.busy() || self.dirty {
            return;
        }
        let (Some(service), core) = (self.service.clone(), self.core.clone()) else {
            return;
        };
        self.operation = Some(if id.is_some() {
            Operation::UpdateOne
        } else {
            Operation::UpdateAll
        });
        self.error = None;
        self.notice = None;
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.block_on(async {
                        match id {
                            Some(id) => service.update_rule_set(&id).await,
                            None => service.update_rule_sets().await,
                        }
                    })?
                    .map_err(|error| error.to_string())
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.operation = None;
                    match result {
                        Ok((outcome, document)) => {
                            this.apply_document(document, window, cx);
                            this.notice = Some(update_notice(outcome));
                            cx.emit(RulesChanged);
                        }
                        Err(error) => this.error = Some(error),
                    }
                    cx.notify();
                })
                .ok();
            });
        }));
        cx.notify();
    }
}

fn update_notice(outcome: RuleSetUpdateOutcome) -> String {
    format!(
        "规则集更新完成：更新 {}，已最新 {}，失败 {}",
        outcome.updated, outcome.skipped, outcome.failed
    )
}
