use std::time::{SystemTime, UNIX_EPOCH};

use gpui_kit::{Context, Window, component::WindowExt as _};
use lanpath_client::{
    core_config::{BUILTIN_ROUTE_RULES, BUILTIN_RULE_SETS},
    local_override::{
        CustomRuleSet, CustomRuleSetSource, LocalRule, RuleAction, RuleAdvancedOptions,
        RuleMatchType, RuleSetFormat,
    },
};
use uuid::Uuid;

use super::{
    RulesView,
    inputs::Field,
    market::{DownloadSource, MarketEntry, MarketGroup},
};

impl RulesView {
    pub(super) fn update_field(&mut self, field: Field, value: String, cx: &mut Context<Self>) {
        match field {
            Field::RuleName | Field::RuleTarget | Field::RuleOutbound | Field::RuleNote => {
                let Some(rule) = self.selected_rule_mut() else {
                    return;
                };
                match field {
                    Field::RuleName => rule.name = value,
                    Field::RuleTarget => rule.target = value,
                    Field::RuleOutbound => rule.action = RuleAction::Outbound { tag: value },
                    Field::RuleNote => rule.note = value,
                    _ => unreachable!(),
                }
                self.changed(cx);
            }
            Field::SetName
            | Field::SetTag
            | Field::SetCategory
            | Field::SetUrl
            | Field::SetManual => {
                let Some(rule_set) = self.selected_set_mut() else {
                    return;
                };
                match field {
                    Field::SetName => rule_set.name = value,
                    Field::SetTag => rule_set.tag = value,
                    Field::SetCategory => rule_set.category = value,
                    Field::SetUrl => {
                        let format = if value.trim().to_lowercase().ends_with(".json") {
                            RuleSetFormat::Source
                        } else {
                            RuleSetFormat::Binary
                        };
                        rule_set.source = CustomRuleSetSource::Remote { url: value, format };
                    }
                    Field::SetManual => {
                        rule_set.source = CustomRuleSetSource::Manual { content: value };
                    }
                    _ => unreachable!(),
                }
                self.changed(cx);
            }
            Field::BatchCategory | Field::MarketSearch | Field::MarketCategory => cx.notify(),
        }
    }

    pub(super) fn select_rule(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_rule = Some(id);
        self.sync_rule_inputs(window, cx);
        cx.notify();
    }

    pub(super) fn select_set(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_set = Some(id);
        self.sync_set_inputs(window, cx);
        cx.notify();
    }

    pub(super) fn add_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = &mut self.draft else { return };
        let id = Uuid::new_v4().to_string();
        let sort_order = draft
            .singbox
            .rules
            .iter()
            .map(|rule| rule.sort_order)
            .max()
            .unwrap_or(-1)
            .saturating_add(1);
        draft.singbox.rules.push(LocalRule {
            id: id.clone(),
            name: String::new(),
            enabled: true,
            match_type: RuleMatchType::Domain,
            target: String::new(),
            action: RuleAction::Proxy,
            advanced: RuleAdvancedOptions::default(),
            note: String::new(),
            created_at: now(),
            sort_order,
            builtin: false,
        });
        self.selected_rule = Some(id);
        self.sync_rule_inputs(window, cx);
        self.changed(cx);
    }

    pub(super) fn toggle_rule(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(rule) = self
            .draft
            .as_mut()
            .and_then(|draft| draft.singbox.rules.iter_mut().find(|rule| rule.id == id))
        {
            rule.enabled = !rule.enabled;
            self.changed(cx);
        }
    }

    pub(super) fn move_rule(&mut self, id: &str, offset: isize, cx: &mut Context<Self>) {
        let Some(rules) = self.draft.as_mut().map(|draft| &mut draft.singbox.rules) else {
            return;
        };
        let Some(index) = rules.iter().position(|rule| rule.id == id) else {
            return;
        };
        let target = index as isize + offset;
        if !(0..rules.len() as isize).contains(&target) {
            return;
        }
        rules.swap(index, target as usize);
        for (index, rule) in rules.iter_mut().enumerate() {
            rule.sort_order = i32::try_from(index).unwrap_or(i32::MAX);
        }
        self.changed(cx);
    }

    pub(super) fn set_rule_match(&mut self, match_type: RuleMatchType, cx: &mut Context<Self>) {
        let Some(rule) = self.selected_rule_mut() else {
            return;
        };
        if matches!(match_type, RuleMatchType::Final) {
            rule.target.clear();
        }
        rule.match_type = match_type;
        self.changed(cx);
    }

    pub(super) fn set_rule_action(&mut self, action: RuleAction, cx: &mut Context<Self>) {
        let Some(rule) = self.selected_rule_mut() else {
            return;
        };
        rule.action = action;
        self.changed(cx);
    }

    pub(super) fn toggle_no_resolve(&mut self, cx: &mut Context<Self>) {
        let Some(rule) = self.selected_rule_mut() else {
            return;
        };
        rule.advanced.no_resolve = !rule.advanced.no_resolve;
        self.changed(cx);
    }

    pub(super) fn toggle_invert(&mut self, cx: &mut Context<Self>) {
        let Some(rule) = self.selected_rule_mut() else {
            return;
        };
        rule.advanced.invert = !rule.advanced.invert;
        self.changed(cx);
    }

    pub(super) fn request_delete_rule(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected_rule.clone() else {
            return;
        };
        let label = self.selected_rule_ref().map(rule_label).unwrap_or_default();
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            let id = id.clone();
            dialog
                .title(format!("删除规则“{label}”？"))
                .description("该规则会从下一次生成的核心配置中移除。")
                .confirm()
                .ok_text("删除")
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| view.delete_rule(&id, window, cx));
                    true
                })
        });
    }

    fn delete_rule(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = &mut self.draft else { return };
        draft.singbox.rules.retain(|rule| rule.id != id);
        self.selected_rule = draft.singbox.rules.first().map(|rule| rule.id.clone());
        self.sync_rule_inputs(window, cx);
        self.changed(cx);
    }

    pub(super) fn add_rule_set(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = &mut self.draft else { return };
        let id = Uuid::new_v4().to_string();
        draft.custom_rule_sets.push(CustomRuleSet {
            id: id.clone(),
            name: String::new(),
            tag: String::new(),
            category: String::new(),
            source: CustomRuleSetSource::Remote {
                url: String::new(),
                format: RuleSetFormat::Binary,
            },
            last_updated: 0,
            remote_updated_at: 0,
            remote_state: Default::default(),
            builtin: false,
        });
        self.selected_set = Some(id);
        self.sync_set_inputs(window, cx);
        self.changed(cx);
    }

    pub(super) fn set_source_kind(
        &mut self,
        remote: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(rule_set) = self.selected_set_mut() else {
            return;
        };
        rule_set.source = if remote {
            CustomRuleSetSource::Remote {
                url: String::new(),
                format: RuleSetFormat::Binary,
            }
        } else {
            CustomRuleSetSource::Manual {
                content: "{\n  \"version\": 1,\n  \"rules\": []\n}".into(),
            }
        };
        self.sync_set_inputs(window, cx);
        self.changed(cx);
    }

    pub(super) fn toggle_category_selection(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.category_selection.remove(id) {
            self.category_selection.insert(id.to_string());
        }
        cx.notify();
    }

    pub(super) fn apply_batch_category(&mut self, cx: &mut Context<Self>) {
        if self.category_selection.is_empty() {
            return;
        }
        let category = self
            .inputs
            .batch_category
            .read(cx)
            .value()
            .trim()
            .to_string();
        if let Some(draft) = &mut self.draft {
            for rule_set in &mut draft.custom_rule_sets {
                if self.category_selection.contains(&rule_set.id) {
                    rule_set.category.clone_from(&category);
                }
            }
        }
        self.changed(cx);
    }

    pub(super) fn request_delete_set(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected_set.clone() else {
            return;
        };
        let tag = self
            .selected_set_ref()
            .map(|set| set.tag.clone())
            .unwrap_or_default();
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            let id = id.clone();
            dialog
                .title(format!("删除规则集“{tag}”？"))
                .description("引用该 tag 的规则会保留，但在资源恢复前不会生成有效规则集。")
                .confirm()
                .ok_text("删除")
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| view.delete_set(&id, window, cx));
                    true
                })
        });
    }

    fn delete_set(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = &mut self.draft else { return };
        draft.custom_rule_sets.retain(|set| set.id != id);
        self.cached_rule_sets.remove(id);
        self.category_selection.remove(id);
        self.selected_set = draft.custom_rule_sets.first().map(|set| set.id.clone());
        self.sync_set_inputs(window, cx);
        self.changed(cx);
    }

    pub(super) fn restore_builtins(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = &mut self.draft else { return };
        let min_sort = draft
            .singbox
            .rules
            .iter()
            .map(|rule| rule.sort_order)
            .min()
            .unwrap_or(0);
        let mut restored_rules = Vec::new();
        for (index, spec) in BUILTIN_ROUTE_RULES.iter().enumerate() {
            if draft.singbox.rules.iter().any(|rule| rule.id == spec.id) {
                continue;
            }
            restored_rules.push(LocalRule {
                id: spec.id.into(),
                name: spec.name.into(),
                enabled: true,
                match_type: RuleMatchType::RuleSet,
                target: spec.target.into(),
                action: if spec.direct {
                    RuleAction::Direct
                } else {
                    RuleAction::Proxy
                },
                advanced: Default::default(),
                note: "内置规则：可修改、排序、删除并重新恢复".into(),
                created_at: now(),
                sort_order: min_sort
                    .saturating_sub(i32::try_from(BUILTIN_ROUTE_RULES.len() - index).unwrap_or(1)),
                builtin: true,
            });
        }
        restored_rules.append(&mut draft.singbox.rules);
        draft.singbox.rules = restored_rules;
        for spec in BUILTIN_RULE_SETS {
            if draft.custom_rule_sets.iter().any(|set| set.id == spec.id) {
                continue;
            }
            draft.custom_rule_sets.push(CustomRuleSet {
                id: spec.id.into(),
                name: spec.name.into(),
                tag: spec.tag.into(),
                category: String::new(),
                source: CustomRuleSetSource::Remote {
                    url: spec.url.into(),
                    format: RuleSetFormat::Binary,
                },
                last_updated: 0,
                remote_updated_at: 0,
                remote_state: Default::default(),
                builtin: true,
            });
        }
        self.changed(cx);
    }

    pub(super) fn set_market_group(&mut self, group: MarketGroup, cx: &mut Context<Self>) {
        self.market_group = group;
        cx.notify();
    }

    pub(super) fn set_download_source(&mut self, source: DownloadSource, cx: &mut Context<Self>) {
        self.download_source = source;
        cx.notify();
    }

    pub(super) fn add_market_entry(&mut self, entry: &MarketEntry, cx: &mut Context<Self>) {
        let category = self
            .inputs
            .market_category
            .read(cx)
            .value()
            .trim()
            .to_string();
        let source = self.download_source;
        let Some(draft) = &mut self.draft else { return };
        if draft.custom_rule_sets.iter().any(|set| match &set.source {
            CustomRuleSetSource::Remote { url, .. } => entry.equivalent_to(url),
            CustomRuleSetSource::Manual { .. } => false,
        }) {
            return;
        }
        draft.custom_rule_sets.push(CustomRuleSet {
            id: Uuid::new_v4().to_string(),
            name: entry.name.clone(),
            tag: entry.id.clone(),
            category,
            source: CustomRuleSetSource::Remote {
                url: entry.url(source),
                format: RuleSetFormat::Binary,
            },
            last_updated: 0,
            remote_updated_at: 0,
            remote_state: Default::default(),
            builtin: false,
        });
        self.changed(cx);
        self.notice = Some(format!("已将“{}”加入未保存修改", entry.name));
    }

    fn selected_rule_mut(&mut self) -> Option<&mut LocalRule> {
        let id = self.selected_rule.as_deref()?;
        self.draft
            .as_mut()?
            .singbox
            .rules
            .iter_mut()
            .find(|rule| rule.id == id)
    }

    pub(super) fn selected_rule_ref(&self) -> Option<&LocalRule> {
        let id = self.selected_rule.as_deref()?;
        self.draft
            .as_ref()?
            .singbox
            .rules
            .iter()
            .find(|rule| rule.id == id)
    }

    fn selected_set_mut(&mut self) -> Option<&mut CustomRuleSet> {
        let id = self.selected_set.as_deref()?;
        self.draft
            .as_mut()?
            .custom_rule_sets
            .iter_mut()
            .find(|set| set.id == id)
    }

    pub(super) fn selected_set_ref(&self) -> Option<&CustomRuleSet> {
        let id = self.selected_set.as_deref()?;
        self.draft
            .as_ref()?
            .custom_rule_sets
            .iter()
            .find(|set| set.id == id)
    }
}

pub(super) fn rule_label(rule: &LocalRule) -> String {
    if !rule.name.trim().is_empty() {
        rule.name.clone()
    } else if matches!(rule.match_type, RuleMatchType::Final) {
        "最终规则".into()
    } else {
        format!("{}：{}", match_label(&rule.match_type), rule.target)
    }
}

pub(super) fn match_label(match_type: &RuleMatchType) -> &'static str {
    match match_type {
        RuleMatchType::Domain => "域名",
        RuleMatchType::DomainSuffix => "域名后缀",
        RuleMatchType::DomainKeyword => "域名关键词",
        RuleMatchType::IpCidr => "IP 段",
        RuleMatchType::SourceIpCidr => "源 IP 段",
        RuleMatchType::RuleSet => "规则集",
        #[cfg(target_os = "android")]
        RuleMatchType::AppPackage => "应用包名",
        #[cfg(not(target_os = "android"))]
        RuleMatchType::ProcessName => "进程名",
        RuleMatchType::Port => "端口",
        RuleMatchType::Final => "最终规则",
    }
}

pub(super) fn action_label(action: &RuleAction) -> String {
    match action {
        RuleAction::Proxy => "代理".into(),
        RuleAction::Direct => "直连".into(),
        RuleAction::Reject => "拒绝".into(),
        RuleAction::Outbound { tag } => format!("出站：{tag}"),
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
