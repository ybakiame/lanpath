//! 快速规则工作台：表单、批量输入、排序与本地匹配预览。
mod editor;
mod rows;
use crate::rules::{Rules, rule_line};
use crate::ui::{
    button::{ButtonVariant, button},
    input::input,
    layout::{page_header, section_title},
};
use lanpath_client::LocalRule;
use lanpath_common::LanpathResult;
use nsh_core::prelude::*;

#[derive(Clone)]
pub(super) struct State {
    service: Option<Rules>,
    rules: Signal<Vec<LocalRule>>,
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
        Self {
            service: service.ok(),
            rules: signal(rules),
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
    let form_state = state.clone();
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
        .gap_3()
        .child(page_header(|| t!("rules.title"), || t!("rules.subtitle")))
        .child(editor::editor(form_state))
        .child(when(
            move || error.get().is_some(),
            move || {
                text(move || error.get().unwrap_or_default())
                    .text_color(move || nsh_base::theme().destructive)
            },
        ))
        .child(when(
            move || state.saved.get(),
            || text(|| t!("rules.saved")),
        ))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child(section_title(|| t!("rules.list")))
                .child(view().flex_1())
                .child(
                    input(search)
                        .placeholder(|| t!("rules.search"))
                        .test_id("rules-search")
                        .accessible_name(|| t!("rules.search"))
                        .on_change(move |v| search.set(v)),
                ),
        )
        .child(rows::rows(state, filtered))
}
