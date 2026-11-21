//! 订阅列表：参考 Quick settings 卡片层级（顶部联动图标 + 开关、主体名称/摘要、
//! 底部紧凑操作栏），保留 CRUD、绑定、草稿、刷新单飞与确认删除语义。

use lanpath_client::Subscription;
use nsh_base::{menu, menu_item, theme};
use nsh_core::prelude::*;
use uuid::Uuid;

use crate::ui::overlay::confirm_dialog;

use super::{SubscriptionPageState, spawn_refresh};

pub(super) fn subscription_list(state: SubscriptionPageState) -> impl IntoElement {
    let count_state = state.clone();
    let empty_state = state.clone();
    let message_state = state.clone();
    let notice_state = message_state.clone();
    let list_error = message_state.list_error;
    let notice = notice_state.notice;
    let busy_show = state.clone();
    let row_state = state.clone();
    let remove_dialog_state = state.clone();
    let edit_dialog_state = state.clone();
    let state_search = state.search;
    let list = state.list;
    let filtered = computed(move || {
        let query = state_search.get().to_lowercase();
        list.get()
            .into_iter()
            .filter(|s| s.name.to_lowercase().contains(&query))
            .collect::<Vec<_>>()
    });
    let source_dialog_state = state;
    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_col()
        .gap_3()
        .child(
            view()
                .flex_col()
                .gap_2()
                .child(
                    text(move || t!("subscription.count", count = count_state.list.get().len()))
                        .text_color(move || theme().muted_foreground),
                )
                .child(when(
                    move || {
                        empty_state.list.get().is_empty() && empty_state.list_error.get().is_none()
                    },
                    || {
                        text(|| t!("subscription.empty"))
                            .text_color(move || theme().muted_foreground)
                    },
                ))
                .child(when(
                    move || list_error.get().is_some(),
                    move || {
                        text(move || message_state.list_error.get().unwrap_or_default())
                            .text_color(move || theme().destructive)
                    },
                ))
                .child(when(
                    move || notice.get().is_some(),
                    move || {
                        text(move || notice_state.notice.get().unwrap_or_default())
                            .text_color(move || theme().muted_foreground)
                    },
                ))
                .child(when(
                    move || busy_show.any_refreshing(),
                    move || {
                        text(super::busy_refreshing())
                            .text_size(px(12.0))
                            .text_color(move || theme().muted_foreground)
                    },
                ))
                .child(
                    crate::ui::input(state_search)
                        .test_id("subscription-search")
                        .accessible_name(|| t!("subscription.search"))
                        .placeholder(|| t!("subscription.search"))
                        .on_change(move |v| state_search.set(v)),
                )
                .child(delete_confirm(remove_dialog_state))
                .child(when(
                    move || edit_dialog_state.edit.id.get().is_some(),
                    move || super::editor::edit_dialog(edit_dialog_state.clone()),
                ))
                .child(when(
                    move || source_dialog_state.source_view.get().is_some(),
                    move || super::source_view::source_dialog(source_dialog_state.clone()),
                )),
        )
        .child(crate::ui::cards::card_grid(
            filtered,
            184.0,
            None,
            |subscription: &Subscription| subscription.id.to_string(),
            move |item| subscription_row(row_state.clone(), item).into_element(),
        ))
}

fn subscription_row(
    state: SubscriptionPageState,
    item: ReadSignal<Subscription>,
) -> impl IntoElement {
    let row_test_id = format!("subscription-row-{}", item.get().name);
    let id = item.get().id;
    let switch_state = state.clone();
    let refresh_button = state.clone();
    let edit_button = state.clone();
    let source_button = state.clone();
    let activate_state = state.clone();
    let refresh_status_state = state.clone();
    let active_state = state.clone();
    let more_state = state;
    let enabled = item;
    let selected = active_state.active;
    let icon_active = active_state.active;
    crate::ui::cards::selection_card(
        move || enabled.get().enabled,
        move || selected.get() == Some(id),
    )
    .test_id(&row_test_id)
    .optional_label(move || Some(item.get().name.into()))
    .flex_col()
    .justify_between()
    .gap_2()
    .p(px(12.0))
    .child(
        // Quick settings 顶部行：左侧状态联动圆形图标徽章，右侧原生开关。
        view()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                view()
                    .size(px(32.0))
                    .flex_none()
                    .rounded_full()
                    .items_center()
                    .justify_center()
                    .bg(move || {
                        if item.get().enabled {
                            theme().primary
                        } else {
                            theme().muted
                        }
                    })
                    .text_color(move || {
                        if item.get().enabled {
                            theme().primary_foreground
                        } else {
                            theme().muted_foreground
                        }
                    })
                    .child(when(
                        move || item.get().enabled,
                        move || {
                            let is_active = icon_active.get() == Some(id);
                            crate::ui::icon::icon_sized(
                                if is_active {
                                    crate::ui::icon::Icon::Check
                                } else {
                                    crate::ui::icon::Icon::Subscription
                                },
                                crate::ui::icon::Tone::OnPrimary,
                                15.0,
                            )
                        },
                    ))
                    .child(when(
                        move || !item.get().enabled,
                        || {
                            crate::ui::icon::icon_sized(
                                crate::ui::icon::Icon::Subscription,
                                crate::ui::icon::Tone::Muted,
                                15.0,
                            )
                        },
                    )),
            )
            .child(enabled_switch(switch_state, item)),
    )
    .child(
        // Quick settings 主体信息：名称 + 当前生效/刷新标记 + 节点与格式摘要。
        view()
            .flex_1()
            .min_h(px(0.0))
            .flex_col()
            .gap(px(2.0))
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        text(move || item.get().name.clone())
                            .text_size(px(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .flex_1()
                            .truncate()
                            .min_w(px(0.0)),
                    )
                    .child(when(
                        move || refresh_status_state.refreshing.get().contains(&id),
                        || {
                            text(move || t!("subscription.refreshing"))
                                .text_size(px(11.0))
                                .text_color(move || theme().muted_foreground)
                        },
                    ))
                    .child(when(
                        move || {
                            active_state.active.get() == Some(id)
                                && !refresh_status_state.refreshing.get().contains(&id)
                        },
                        || {
                            text(|| t!("subscription.active_mark"))
                                .text_size(px(11.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(move || theme().primary)
                        },
                    )),
            )
            .child(
                text(move || {
                    let subscription = item.get();
                    let mut summary = if subscription.node_count > 0 {
                        t!("subscription.nodes", count = subscription.node_count)
                    } else {
                        t!("subscription.nodes_unknown")
                    };
                    if let Some(format) = subscription.format {
                        summary.push_str(&format!(
                            " · {}",
                            t!("subscription.format", format = format!("{format:?}"))
                        ));
                    }
                    summary
                })
                .text_size(px(11.5))
                .text_color(move || theme().muted_foreground)
                .truncate(),
            )
            .child(binding_summary(more_state.clone(), item))
            .child(when(
                move || item.get().error.is_some(),
                move || {
                    text(t!(
                        "subscription.error_prefix",
                        error = item.get().error.clone().unwrap_or_default()
                    ))
                    .text_size(px(11.0))
                    .text_color(move || theme().destructive)
                    .truncate()
                },
            )),
    )
    .child(
        // 底部紧凑操作栏：单行收纳常用操作与更多菜单。
        view()
            .pt(px(6.0))
            .border_t(px(1.0))
            .border_color(move || theme().border.opacity(0.45))
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(2.0))
            .child(row_button(
                refresh_button,
                item,
                || t!("subscription.action.refresh"),
                |state, subscription| {
                    run_refresh(state, subscription.id);
                },
            ))
            .child(row_button(
                edit_button,
                item,
                || t!("subscription.action.edit"),
                |state, subscription| {
                    state.edit.open(subscription);
                },
            ))
            .child(row_button(
                source_button,
                item,
                || t!("subscription.action.source"),
                |state, subscription| {
                    super::source_view::open_source(state, subscription);
                },
            ))
            .child(when(
                move || activate_state.active.get() != Some(id),
                move || {
                    row_button(
                        activate_state.clone(),
                        item,
                        || t!("subscription.action.activate"),
                        |state, subscription| {
                            run_activate(state, subscription.id);
                        },
                    )
                },
            ))
            .child(more_menu(more_state, item)),
    )
}

/// 行内覆写模板绑定：已绑定显示模板名；引用失效时显示真实错误。
fn binding_summary(
    state: SubscriptionPageState,
    item: ReadSignal<Subscription>,
) -> impl IntoElement {
    when(
        move || item.get().profile_id.is_some(),
        move || {
            let profiles = state.profiles.get();
            match crate::profiles_view::profile_name(&profiles, item.get().profile_id) {
                Some(name) => text(t!("subscription.binding", name = name))
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground)
                    .into_element(),
                None => text(|| t!("subscription.binding_invalid"))
                    .text_size(px(12.0))
                    .text_color(move || theme().destructive)
                    .into_element(),
            }
        },
    )
}

/// 启用开关（原生 Switch 语义，走 `on_change` 持久化并刷新摘要）。
fn enabled_switch(
    state: SubscriptionPageState,
    item: ReadSignal<Subscription>,
) -> impl IntoElement {
    let switch_test_id = format!("subscription-enabled-{}", item.get().id);
    let id = item.get().id;
    let change_state = state;
    view()
        .test_id(&switch_test_id)
        .child(crate::ui::select::switch_labeled(
            move || item.get().enabled,
            move || t!("subscription.enabled_a11y", name = item.get().name).into(),
            move |enabled| {
                change_state.bump_serial();
                let result = change_state
                    .service
                    .as_ref()
                    .map(|service| service.set_enabled(id, enabled));
                match result {
                    Some(Ok(())) => {
                        change_state.reload();
                        change_state.post_notice(t!("subscription.enabled_updated"));
                    }
                    Some(Err(error)) => {
                        change_state.reload();
                        change_state.post_error(t!(
                            "subscription.enable_failed",
                            error = error.to_string()
                        ));
                    }
                    None => change_state.post_error(t!("subscription.root_unavailable")),
                }
            },
        ))
}

fn more_menu(state: SubscriptionPageState, item: ReadSignal<Subscription>) -> AnyElement {
    let menu_state = state;
    let trigger = compact_action(|| t!("subscription.more"));
    let danger_bg = || {
        theme()
            .color("danger-surface")
            .unwrap_or_else(|| theme().accent)
    };
    menu()
        .offset(px(4.0))
        .popup_style(|s| crate::ui::select::popup_surface(s).min_w(px(160.0)))
        .popup_starting_style(crate::ui::select::popup_starting)
        .popup_ending_style(crate::ui::select::popup_ending)
        .trigger(trigger)
        .child(
            menu_item()
                .px_3()
                .py_2()
                .cursor(Cursor::Default)
                .rounded(move || theme().number("radius-sm").map(px).unwrap_or(px(4.0)))
                .hover(move |s| s.bg(danger_bg))
                .focus(move |s| s.bg(danger_bg))
                .text_color(move || theme().destructive)
                .child(
                    view()
                        .test_id("subscription-delete")
                        .child(text(|| t!("subscription.action.delete")).text_size(px(13.0))),
                )
                .on_select(move || menu_state.pending_remove.set(Some(item.get()))),
        )
        .into_element()
}

/// 行内紧凑操作按钮：贴合 Quick settings 卡片底部栏，避免窄卡下按钮溢出换行。
fn compact_action(label: impl Fn() -> String + 'static) -> nsh_base::ButtonElement {
    nsh_base::button()
        .px(px(6.0))
        .py(px(3.0))
        .min_h(px(24.0))
        .rounded(px(6.0))
        .cursor(Cursor::Default)
        .text_color(move || theme().muted_foreground)
        .hover(|s| {
            s.bg(move || theme().muted.opacity(0.8))
                .text_color(move || theme().foreground)
        })
        .active(|s| s.bg(move || theme().accent))
        .focus_visible(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .child(
            text(label)
                .text_size(px(11.5))
                .font_weight(FontWeight::MEDIUM),
        )
}

/// 行操作按钮：同步操作经业务层互斥排队，异步刷新由 `begin_refresh` 单飞防重。
fn row_button(
    state: SubscriptionPageState,
    item: ReadSignal<Subscription>,
    label: impl Fn() -> String + 'static,
    action: impl Fn(&SubscriptionPageState, &Subscription) + 'static,
) -> impl IntoElement {
    compact_action(label).on_press(move || action(&state, &item.get()))
}

fn run_refresh(state: &SubscriptionPageState, id: Uuid) {
    let _ = spawn_refresh(state.clone(), id);
}

fn run_activate(state: &SubscriptionPageState, id: Uuid) {
    state.bump_serial();
    match state.service.as_ref().map(|service| service.set_active(id)) {
        Some(Ok(())) => {
            state.reload();
            state.post_notice(t!("subscription.activate_done"));
        }
        Some(Err(error)) => {
            state.reload();
            state.post_error(t!(
                "subscription.activate_failed",
                error = error.to_string()
            ));
        }
        None => state.post_error(t!("subscription.root_unavailable")),
    }
}

/// 删除确认弹窗（危险变体，确认后移除记录与本地节点缓存）。
fn delete_confirm(state: SubscriptionPageState) -> AnyElement {
    let target = state.pending_remove;
    let service = state.service.clone();
    let serial = state.serial;
    confirm_dialog(move || target.get().is_some())
        .test_id("subscription-delete-dialog")
        .destructive()
        .title(move || {
            target
                .get()
                .map(|subscription| t!("subscription.delete_title", name = subscription.name))
                .unwrap_or_default()
        })
        .description(|| t!("subscription.delete_description"))
        .confirm_label(|| t!("subscription.delete_confirm"))
        .on_cancel(move || target.set(None))
        .on_confirm(move || {
            let Some(subscription) = target.get() else {
                return;
            };
            target.set(None);
            serial.update(|value| *value += 1);
            let result = service
                .as_ref()
                .map(|service| service.remove(subscription.id));
            match result {
                Some(Ok(())) => {
                    state.reload();
                    state.post_notice(t!("subscription.delete_done"));
                }
                Some(Err(error)) => {
                    state.reload();
                    state.post_error(t!("subscription.delete_failed", error = error.to_string()));
                }
                None => state.post_error(t!("subscription.root_unavailable")),
            }
        })
        .into_element()
}
