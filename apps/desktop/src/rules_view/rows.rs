use super::*;
use crate::ui::button::ButtonSize;
use nsh_core::prelude::*;

pub(super) fn rows(state: State, filtered: Memo<Vec<LocalRule>>) -> impl IntoElement {
    let keys = filtered;
    virtual_list(
        move || filtered.with(Vec::len),
        move |at| {
            let row = computed(move || filtered.with(|rows| rows.get(at.index()).cloned()));
            let edit = state.clone();
            let toggle = state.clone();
            let up = state.clone();
            let down = state.clone();
            let remove = state.clone();
            view()
                .flex_col()
                .gap_1()
                .py_2()
                .child(text(move || {
                    row.get()
                        .map(|r| {
                            format!("{}  {}", if r.enabled { "●" } else { "○" }, rule_line(&r))
                        })
                        .unwrap_or_default()
                }))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .gap_1()
                        .child(
                            button(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .label(|| t!("rules.edit"))
                                .disabled(move || {
                                    row.get()
                                        .is_none_or(|r| !crate::rules::syntax::editable(&r))
                                })
                                .on_press(move || {
                                    if let Some(rule) = row.get() {
                                        edit.edit(&rule);
                                    }
                                }),
                        )
                        .child(
                            button(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .label(move || {
                                    row.get()
                                        .map(|r| {
                                            if r.enabled {
                                                t!("rules.disable")
                                            } else {
                                                t!("rules.enable")
                                            }
                                        })
                                        .unwrap_or_default()
                                })
                                .on_press(move || {
                                    if let Some(rule) = row.get() {
                                        toggle.apply(|s| s.toggle(&rule.id));
                                    }
                                }),
                        )
                        .child(
                            button(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .label(|| t!("rules.up"))
                                .on_press(move || {
                                    if let Some(rule) = row.get() {
                                        up.apply(|s| s.move_rule(&rule.id, -1));
                                    }
                                }),
                        )
                        .child(
                            button(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .label(|| t!("rules.down"))
                                .on_press(move || {
                                    if let Some(rule) = row.get() {
                                        down.apply(|s| s.move_rule(&rule.id, 1));
                                    }
                                }),
                        )
                        .child(
                            button(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .label(|| t!("rules.delete"))
                                .on_press(move || {
                                    if let Some(rule) = row.get() {
                                        remove.apply(|s| s.remove(&rule.id));
                                    }
                                }),
                        ),
                )
        },
    )
    .keys(move || keys.with(|rows| rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>()))
    .estimated_item_size(px(80.0))
    .empty(text(|| t!("rules.empty")))
    .flex_1()
    .min_h(px(120.0))
}
