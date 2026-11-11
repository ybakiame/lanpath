use gpui_kit::component::input::InputState;
use gpui_kit::{Context, Entity, Window};
use lanpath_client::local_override::{CustomRuleSetSource, RuleAction};

use super::RulesView;

impl RulesView {
    pub(super) fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_rule_inputs(window, cx);
        self.sync_set_inputs(window, cx);
    }

    pub(super) fn sync_rule_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let values = self
            .selected_rule_ref()
            .map(|rule| {
                (
                    rule.name.clone(),
                    rule.target.clone(),
                    match &rule.action {
                        RuleAction::Outbound { tag } => tag.clone(),
                        _ => String::new(),
                    },
                    rule.note.clone(),
                )
            })
            .unwrap_or_default();
        self.syncing = true;
        set_input(&self.inputs.rule_name, values.0, window, cx);
        set_input(&self.inputs.rule_target, values.1, window, cx);
        set_input(&self.inputs.rule_outbound, values.2, window, cx);
        set_input(&self.inputs.rule_note, values.3, window, cx);
        self.syncing = false;
    }

    pub(super) fn sync_set_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let values = self
            .selected_set_ref()
            .map(|rule_set| {
                let (url, manual) = match &rule_set.source {
                    CustomRuleSetSource::Remote { url, .. } => (url.clone(), String::new()),
                    CustomRuleSetSource::Manual { content } => (String::new(), content.clone()),
                };
                (
                    rule_set.name.clone(),
                    rule_set.tag.clone(),
                    rule_set.category.clone(),
                    url,
                    manual,
                )
            })
            .unwrap_or_default();
        self.syncing = true;
        set_input(&self.inputs.set_name, values.0, window, cx);
        set_input(&self.inputs.set_tag, values.1, window, cx);
        set_input(&self.inputs.set_category, values.2, window, cx);
        set_input(&self.inputs.set_url, values.3, window, cx);
        self.inputs
            .set_manual
            .update(cx, |state, cx| state.set_value(values.4, window, cx));
        self.syncing = false;
    }
}

pub(super) fn set_input(
    input: &Entity<InputState>,
    value: impl Into<String>,
    window: &mut Window,
    cx: &mut Context<RulesView>,
) {
    input.update(cx, |state, cx| state.set_value(value.into(), window, cx));
}
