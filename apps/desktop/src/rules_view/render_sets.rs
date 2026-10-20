use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
    input::Editor,
};
use gpui_kit::{Context, IntoElement, ParentElement as _, Styled as _, div};
use lanpath_client::local_override::{CustomRuleSetSource, RuleMatchType, parse_rule_set_tags};

use super::{
    Operation, RulesView,
    render::{field, section_title},
};

impl RulesView {
    pub(super) fn render_sets(&self, disabled: bool, cx: &Context<Self>) -> impl IntoElement {
        let sets = self
            .draft
            .as_ref()
            .map(|draft| draft.custom_rule_sets.as_slice())
            .unwrap_or_default();
        let selected = self.selected_set_ref();
        let remote_count = sets
            .iter()
            .filter(|set| matches!(set.source, CustomRuleSetSource::Remote { .. }))
            .count();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_2()
            .child(section_title(
                "规则集",
                "远程内容缓存到实验目录；手动 JSON 保存即写入本地文件。分类只影响整理。",
                cx,
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("ruleset-add")
                            .label("新建规则集…")
                            .disabled(disabled)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.add_rule_set(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("ruleset-update-all")
                            .label(match self.operation {
                                Some(Operation::UpdateAll) => "更新中…",
                                _ => "更新全部远程资源",
                            })
                            .disabled(disabled || self.dirty || remote_count == 0)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.update_rule_sets(None, window, cx)
                            })),
                    )
                    .child(
                        Button::new("ruleset-delete")
                            .label("删除")
                            .danger()
                            .disabled(disabled || selected.is_none())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.request_delete_set(window, cx)
                            })),
                    ),
            )
            .children((!self.category_selection.is_empty()).then(|| {
                div()
                    .flex()
                    .items_end()
                    .gap_2()
                    .child(field("批量分类", &self.inputs.batch_category, disabled))
                    .child(
                        Button::new("ruleset-apply-category")
                            .label(format!("应用到 {} 项", self.category_selection.len()))
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, _, cx| this.apply_batch_category(cx))),
                    )
            }))
            .children(sets.iter().map(|set| {
                let id = set.id.clone();
                let category_id = set.id.clone();
                let cached = self.cached_rule_sets.contains(&set.id);
                let references = self.reference_count(&set.tag);
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new(format!("ruleset-select-{}", set.id))
                            .label(format!(
                                "{}{} · {} · {} · 引用 {}",
                                if set.builtin { "内置 · " } else { "" },
                                if set.name.trim().is_empty() {
                                    &set.tag
                                } else {
                                    &set.name
                                },
                                if set.category.trim().is_empty() {
                                    "未分类"
                                } else {
                                    &set.category
                                },
                                if cached { "已缓存" } else { "缺少缓存" },
                                references
                            ))
                            .selected(self.selected_set.as_deref() == Some(set.id.as_str()))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_set(id.clone(), window, cx)
                            })),
                    )
                    .child(
                        Button::new(format!("ruleset-category-select-{}", set.id))
                            .label(if self.category_selection.contains(&set.id) {
                                "已选"
                            } else {
                                "选择"
                            })
                            .selected(self.category_selection.contains(&set.id))
                            .disabled(disabled)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_category_selection(&category_id, cx)
                            })),
                    )
            }))
            .children(sets.is_empty().then(|| {
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("暂无规则集；可新建、恢复默认或从市场添加。")
            }))
            .children(selected.map(|set| {
                let remote = matches!(set.source, CustomRuleSetSource::Remote { .. });
                let update_id = set.id.clone();
                let cached = self.cached_rule_sets.contains(&set.id);
                let source_detail = match &set.source {
                    CustomRuleSetSource::Remote { format, .. } => format!("远程 · {format:?}"),
                    CustomRuleSetSource::Manual { .. } => "手动 JSON · source".into(),
                };
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(section_title(
                        "规则集详情",
                        format!(
                            "{}；{}；本地更新时间 {}，远端时间 {}",
                            source_detail,
                            if cached {
                                "缓存可用"
                            } else {
                                "缓存缺失，启动时会尝试补齐引用资源"
                            },
                            timestamp(set.last_updated),
                            timestamp(set.remote_updated_at)
                        ),
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("ruleset-source-remote")
                                    .label("远程 URL")
                                    .selected(remote)
                                    .disabled(disabled)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.set_source_kind(true, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("ruleset-source-manual")
                                    .label("手动 JSON")
                                    .selected(!remote)
                                    .disabled(disabled)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.set_source_kind(false, window, cx)
                                    })),
                            )
                            .children(remote.then(|| {
                                Button::new("ruleset-update-one")
                                    .label(match self.operation {
                                        Some(Operation::UpdateOne) => "更新中…",
                                        _ => "更新此资源",
                                    })
                                    .disabled(disabled || self.dirty)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.update_rule_sets(Some(update_id.clone()), window, cx)
                                    }))
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(field("名称", &self.inputs.set_name, disabled))
                            .child(field("引用 tag", &self.inputs.set_tag, disabled))
                            .child(field("分类", &self.inputs.set_category, disabled)),
                    )
                    .children(remote.then(|| field("远程 URL", &self.inputs.set_url, disabled)))
                    .children((!remote).then(|| {
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_sm().child("sing-box rule_set source JSON"))
                            .child(
                                div().h_64().child(
                                    Editor::new(&self.inputs.set_manual)
                                        .aria_label("手动规则集 JSON")
                                        .readonly(disabled)
                                        .size_full(),
                                ),
                            )
                    }))
            }))
    }

    fn reference_count(&self, tag: &str) -> usize {
        self.draft
            .as_ref()
            .map(|draft| {
                draft
                    .singbox
                    .rules
                    .iter()
                    .filter(|rule| {
                        matches!(rule.match_type, RuleMatchType::RuleSet)
                            && parse_rule_set_tags(&rule.target)
                                .iter()
                                .any(|item| item == tag)
                    })
                    .count()
            })
            .unwrap_or(0)
    }
}

fn timestamp(value: u64) -> String {
    if value == 0 {
        "无".into()
    } else {
        value.to_string()
    }
}
