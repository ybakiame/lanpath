//! 快速规则业务：将用户规则持久化在 `<data_root>/rules.toml`（与订阅/备份共用业务锁），
//! 仅保存自定义规则列表，不包含最终覆写切片；合成时分别注入 sing-box 与 Mihomo 配置。
mod mihomo;
pub(crate) mod syntax;
mod trace;

pub(crate) use mihomo::{apply_mihomo_rules, normalize_singbox_ports};
pub(crate) use syntax::{parse_lines, rule_line};
pub(crate) use trace::{RuleTrace, trace_rules};

use crate::data_root::{DataRoot, open_root, resolve_data_location};
#[cfg(test)]
pub(crate) use lanpath_client::rules::RULES_FILE;
use lanpath_client::rules::{LocalRule, load_rules, persist_rules};
use lanpath_common::{LanpathError, LanpathResult};
use serde_json::Value;
use std::{path::Path, sync::Arc};

/// 将启用规则按序注入 sing-box `route.rules`。
pub(crate) fn apply_singbox_rules(config: &mut Value, rules: &[LocalRule]) {
    lanpath_client::rules::apply_singbox_rules(config, rules);
}

#[derive(Clone)]
pub(crate) struct Rules {
    root: Arc<DataRoot>,
}

impl Rules {
    pub(crate) fn open() -> LanpathResult<Self> {
        Self::for_root(&resolve_data_location()?.path)
    }

    pub(crate) fn for_root(path: &Path) -> LanpathResult<Self> {
        Ok(Self {
            root: open_root(path)?,
        })
    }

    fn read_unlocked(&self) -> LanpathResult<Vec<LocalRule>> {
        load_rules(self.root.path())
    }

    fn write_unlocked(&self, rules: &[LocalRule]) -> LanpathResult<()> {
        persist_rules(self.root.path(), rules)
    }

    pub(crate) fn load(&self) -> LanpathResult<Vec<LocalRule>> {
        let _guard = self.root.business_guard();
        self.read_unlocked()
    }

    pub(crate) fn add_lines(&self, source: &str, first: bool) -> LanpathResult<Vec<LocalRule>> {
        let incoming = parse_lines(source)?;
        self.mutate(move |rules| {
            if first {
                rules.splice(0..0, incoming);
            } else {
                rules.extend(incoming);
            }
            Ok(())
        })
    }

    pub(crate) fn edit(&self, id: &str, source: &str) -> LanpathResult<Vec<LocalRule>> {
        let mut parsed = parse_lines(source)?;
        if parsed.len() != 1 {
            return Err(LanpathError::Validation("编辑时请输入一条规则".into()));
        }
        let mut replacement = parsed.remove(0);
        self.mutate(|rules| {
            let rule = rules
                .iter_mut()
                .find(|r| r.id == id)
                .ok_or_else(|| LanpathError::Validation("规则已不存在".into()))?;
            replacement.id = rule.id.clone();
            replacement.created_at = rule.created_at;
            replacement.enabled = rule.enabled;
            replacement.name = rule.name.clone();
            replacement.note = rule.note.clone();
            *rule = replacement;
            Ok(())
        })
    }

    pub(crate) fn toggle(&self, id: &str) -> LanpathResult<Vec<LocalRule>> {
        self.mutate(|rules| {
            let rule = find_rule(rules, id)?;
            rule.enabled = !rule.enabled;
            Ok(())
        })
    }

    pub(crate) fn remove(&self, id: &str) -> LanpathResult<Vec<LocalRule>> {
        self.mutate(|rules| {
            rules.retain(|r| r.id != id);
            Ok(())
        })
    }

    pub(crate) fn move_rule(&self, id: &str, delta: isize) -> LanpathResult<Vec<LocalRule>> {
        self.mutate(|rules| {
            let at = rules
                .iter()
                .position(|r| r.id == id)
                .ok_or_else(|| LanpathError::Validation("规则已不存在".into()))?;
            let to = at
                .saturating_add_signed(delta)
                .min(rules.len().saturating_sub(1));
            rules.swap(at, to);
            Ok(())
        })
    }

    fn mutate(
        &self,
        change: impl FnOnce(&mut Vec<LocalRule>) -> LanpathResult<()>,
    ) -> LanpathResult<Vec<LocalRule>> {
        let _guard = self.root.business_guard();
        self.root.ensure_writable()?;
        let mut rules = self.read_unlocked()?;
        change(&mut rules)?;
        for (index, rule) in rules.iter_mut().enumerate() {
            rule.sort_order = index as i32;
        }
        self.write_unlocked(&rules)?;
        Ok(rules)
    }
}

fn find_rule<'a>(rules: &'a mut [LocalRule], id: &str) -> LanpathResult<&'a mut LocalRule> {
    rules
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| LanpathError::Validation("规则已不存在".into()))
}

#[cfg(test)]
#[path = "rules_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "rules_native_tests.rs"]
mod native_tests;
