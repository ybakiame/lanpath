//! 快速规则工作台：表单、批量输入、排序与本地匹配预览。
mod editor;
mod rows;

use crate::rules::{Rules, rule_line};
use crate::rulesets::{RuleSetRow, RuleSetsService};
use crate::rulesets_view::{GroupSheetController, groups_sheet};
use crate::ui::{
    BadgeVariant, badge,
    button::{ButtonVariant, button},
    input::input,
    layout::{AlertTone, alert_banner, page_header, section_title},
};
use lanpath_client::LocalRule;
use lanpath_client::rulesets::RuleSetGroup;
use lanpath_common::LanpathResult;
use nsh_base::theme;
use nsh_core::prelude::*;

#[derive(Clone)]
pub(super) struct State {
    service: Option<Rules>,
    rules: Signal<Vec<LocalRule>>,
    pub(super) ruleset_items: Signal<Vec<RuleSetRow>>,
    pub(super) ruleset_groups: Signal<Vec<RuleSetGroup>>,
    pub(super) group_sheet: GroupSheetController,
    mode: Signal<usize>,
    kind: Signal<String>,
    target: Signal<String>,
    action: Signal<String>,
    batch: Signal<String>,
    first: Signal<bool>,
    editing: Signal<Option<String>>,
    search: Signal<String>,
    error: Signal<Option<String>>,
    saved: Signal<bool>,
    pending_remove: Signal<Option<LocalRule>>,
}

impl State {
    fn new(seed: String) -> Self {
        let service = Rules::open();
        let (rules, error) = match &service {
            Ok(service) => match service.load() {
                Ok(rules) => (rules, None),
                Err(error) => (vec![], Some(error.to_string())),
            },
            Err(error) => (vec![], Some(error.to_string())),
        };
        let rs_snapshot = RuleSetsService::open()
            .ok()
            .and_then(|s| s.list_snapshot().ok())
            .unwrap_or_default();
        let ruleset_items = signal(rs_snapshot.items);
        let ruleset_groups = signal(rs_snapshot.groups);
        let group_sheet = GroupSheetController::new(ruleset_items, ruleset_groups);
        Self {
            service: service.ok(),
            rules: signal(rules),
            ruleset_items,
            ruleset_groups,
            group_sheet,
            mode: signal(0),
            kind: signal("suffix".into()),
            target: signal(seed),
            action: signal("proxy".into()),
            batch: signal(String::new()),
            first: signal(true),
            editing: signal(None),
            search: signal(String::new()),
            error: signal(error),
            saved: signal(false),
            pending_remove: signal(None),
        }
    }

    fn apply(&self, operation: impl FnOnce(&Rules) -> LanpathResult<Vec<LocalRule>>) {
        let Some(service) = &self.service else {
            self.error.set(Some(t!("subscription.root_unavailable")));
            return;
        };
        match operation(service) {
            Ok(rules) => {
                self.rules.set(rules);
                self.error.set(None);
                self.saved.set(true);
            }
            Err(error) => self.error.set(Some(error.to_string())),
        }
    }

    fn save(&self) {
        let source = self.source();
        self.error.set(None);
        self.apply(|service| match self.editing.get() {
            Some(id) => service.edit(&id, &source),
            None => service.add_lines(&source, self.first.get()),
        });
        if self.error.get().is_none() {
            self.editing.set(None);
            self.target.set(String::new());
            self.batch.set(String::new());
        }
    }

    fn source(&self) -> String {
        if self.mode.get() == 1 {
            self.batch.get()
        } else {
            format!(
                "{} {} -> {}",
                self.kind.get(),
                self.target.get(),
                self.action.get()
            )
        }
    }

    fn edit(&self, rule: &LocalRule) {
        if !crate::rules::syntax::editable(rule) {
            return;
        }
        self.mode.set(1);
        self.batch.set(rule_line(rule));
        self.editing.set(Some(rule.id.clone()));
        self.saved.set(false);
    }
}

pub(crate) fn rules_page(seed: Signal<String>) -> impl IntoElement {
    let state = State::new(seed.get());
    seed.set(String::new());
    let draft = state.clone();
    crate::navigation::protect(move || {
        if !draft.target.get().is_empty()
            || !draft.batch.get().is_empty()
            || draft.group_sheet.is_dirty()
        {
            crate::navigation::LeaveState::Dirty
        } else {
            crate::navigation::LeaveState::Clean
        }
    });
    let form_state = state.clone();
    let remove_state = state.clone();
    let sheet_ctrl = state.group_sheet.clone();
    let pending = state.pending_remove;
    let error = state.error;
    let search = state.search;
    let filtered_state = state.clone();
    let filtered = computed(move || {
        let query = filtered_state.search.get().to_ascii_lowercase();
        filtered_state
            .rules
            .get()
            .into_iter()
            .filter(|r| rule_line(r).to_ascii_lowercase().contains(&query))
            .collect::<Vec<_>>()
    });
    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_col()
        .gap_4()
        .child(page_header(|| t!("rules.title"), || t!("rules.subtitle")))
        .child(editor::editor(form_state))
        .child(when(
            move || error.get().is_some(),
            move || {
                alert_banner(
                    AlertTone::Danger,
                    text(move || error.get().unwrap_or_default())
                        .text_size(px(12.5))
                        .text_color(move || theme().destructive),
                )
            },
        ))
        .child(when(
            move || state.saved.get(),
            || {
                alert_banner(
                    AlertTone::Success,
                    text(|| t!("rules.saved"))
                        .text_size(px(12.5))
                        .text_color(move || theme().color("success").unwrap_or(theme().primary)),
                )
            },
        ))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_3()
                .child(section_title(|| t!("rules.list")))
                .child(badge(BadgeVariant::Outline, move || {
                    filtered.with(Vec::len).to_string()
                }))
                .child(view().flex_1())
                .child(
                    view().w(px(240.0)).child(
                        input(search)
                            .placeholder(|| t!("rules.search"))
                            .test_id("rules-search")
                            .accessible_name(|| t!("rules.search"))
                            .on_change(move |v| search.set(v)),
                    ),
                ),
        )
        .child(rows::rows(state, filtered))
        .child(groups_sheet(sheet_ctrl))
        .child(
            crate::ui::confirm_dialog(move || pending.get().is_some())
                .destructive()
                .test_id("rule-delete-dialog")
                .title(|| t!("rules.delete_title"))
                .description(move || {
                    pending
                        .get()
                        .map(|r| format!("{}\n{}", rule_line(&r), t!("rules.delete_hint")))
                        .unwrap_or_default()
                })
                .confirm_label(|| t!("rules.delete_confirm"))
                .on_cancel(move || pending.set(None))
                .on_confirm(move || {
                    if let Some(rule) = pending.get() {
                        remove_state.apply(|s| s.remove(&rule.id));
                    }
                    pending.set(None);
                }),
        )
}
