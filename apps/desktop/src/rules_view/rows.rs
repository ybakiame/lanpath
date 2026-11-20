use super::*;
use crate::ui::button::ButtonSize;
use crate::ui::icon::Icon;
use nsh_base::theme;
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
            crate::ui::surface(crate::ui::SurfaceLevel::Card)
                .px_4()
                .py_2()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    view()
                        .flex_1()
                        .min_w(px(200.0))
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .child(view().size(px(8.0)).flex_none().rounded_full().bg(move || {
                            if row.get().is_some_and(|r| r.enabled) {
                                theme().color("success").unwrap_or(theme().primary)
                            } else {
                                theme().muted_foreground.opacity(0.4)
                            }
                        }))
                        .child(
                            text(move || row.get().map(|r| rule_line(&r)).unwrap_or_default())
                                .text_size(px(13.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(move || {
                                    if row.get().is_some_and(|r| r.enabled) {
                                        theme().foreground
                                    } else {
                                        theme().muted_foreground
                                    }
                                })
                                .selectable(true),
                        ),
                )
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
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
                                .test_id("rule-remove")
                                .label(|| t!("rules.delete"))
                                .on_press(move || {
                                    if let Some(rule) = row.get() {
                                        remove.pending_remove.set(Some(rule));
                                    }
                                }),
                        ),
                )
        },
    )
    .keys(move || keys.with(|rows| rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>()))
    .estimated_item_size(px(58.0))
    .empty(crate::ui::empty_with_icon(
        Some(Icon::Rules),
        || t!("rules.list"),
        || t!("rules.empty"),
    ))
    .flex_1()
    .min_h(px(120.0))
}
