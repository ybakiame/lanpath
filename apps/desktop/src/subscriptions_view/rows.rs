//! 订阅列表：每行展示名称/启用状态/节点摘要/生效标记，提供刷新、编辑、
//! 原文、设为生效与删除操作；底部挂载删除/编辑/原文三个弹窗。

use lanpath_client::Subscription;
use nsh_base::{button, dialog, switch, theme};
use nsh_core::prelude::*;
use uuid::Uuid;

use super::editor::edit_dialog;
use super::source_view::source_dialog;
use super::{BUSY_REFRESHING, SubscriptionPageState, spawn_refresh};

pub(super) fn subscription_list(state: SubscriptionPageState) -> impl IntoElement {
    let count_state = state.clone();
    let empty_state = state.clone();
    let message_state = state.clone();
    let notice_state = message_state.clone();
    let busy_show = state.clone();
    let row_state = state.clone();
    let remove_dialog_state = state.clone();
    let edit_dialog_state = state.clone();
    let source_dialog_state = state;
    scroll().flex_1().child(
        view()
            .flex_col()
            .gap_2()
            .child(
                text(move || format!("共 {} 个订阅", count_state.list.get().len()))
                    .text_color(move || theme().muted_foreground),
            )
            .child(when(
                move || empty_state.list.get().is_empty() && empty_state.list_error.get().is_none(),
                || text("尚无订阅。").text_color(move || theme().muted_foreground),
            ))
            .child(
                text(move || message_state.list_error.get().unwrap_or_default())
                    .text_color(move || theme().destructive),
            )
            .child(
                text(move || notice_state.notice.get().unwrap_or_default())
                    .text_color(move || theme().muted_foreground),
            )
            .child(when(
                move || busy_show.any_refreshing(),
                move || {
                    text(BUSY_REFRESHING)
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground)
                },
            ))
            .child(each(
                row_state.list,
                |subscription: &Subscription| subscription.id,
                move |item, _index| subscription_row(row_state.clone(), item),
            ))
            .child(when(
                move || remove_dialog_state.pending_remove.get().is_some(),
                move || delete_dialog(remove_dialog_state.clone()),
            ))
            .child(when(
                move || edit_dialog_state.edit.id.get().is_some(),
                move || edit_dialog(edit_dialog_state.clone()),
            ))
            .child(when(
                move || source_dialog_state.source_view.get().is_some(),
                move || source_dialog(source_dialog_state.clone()),
            )),
    )
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
    view()
        .test_id(&row_test_id)
        .flex_col()
        .gap_1()
        .p_3()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .bg(move || theme().card)
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .child(enabled_switch(switch_state, item))
                .child(
                    text(move || item.get().name.clone())
                        .text_size(px(15.0))
                        .truncate()
                        .min_w(px(0.0)),
                )
                .child(when(
                    move || state.active.get() == Some(id),
                    || {
                        text("当前生效")
                            .text_size(px(12.0))
                            .text_color(move || theme().primary)
                    },
                ))
                .child(
                    text(move || {
                        if item.get().enabled {
                            "已启用"
                        } else {
                            "已停用"
                        }
                    })
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground),
                )
                .child(view().flex_1())
                .child(row_button(
                    refresh_button,
                    item,
                    "刷新",
                    |state, subscription| {
                        run_refresh(state, subscription.id);
                    },
                ))
                .child(row_button(
                    edit_button,
                    item,
                    "编辑",
                    |state, subscription| {
                        state.edit.open(subscription);
                    },
                ))
                .child(row_button(
                    source_button,
                    item,
                    "原文",
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
                            "设为生效",
                            |state, subscription| {
                                run_activate(state, subscription.id);
                            },
                        )
                    },
                )),
        )
        .child(
            text(move || {
                let subscription = item.get();
                let mut summary = if subscription.node_count > 0 {
                    format!("{} 个节点", subscription.node_count)
                } else {
                    "未获取节点".to_owned()
                };
                if let Some(format) = subscription.format {
                    summary.push_str(&format!(" · 格式 {format:?}"));
                }
                summary
            })
            .text_size(px(12.0))
            .text_color(move || theme().muted_foreground),
        )
        .child(binding_summary(state.clone(), item))
        .child(when(
            move || item.get().error.is_some(),
            move || {
                text(format!(
                    "错误：{}",
                    item.get().error.clone().unwrap_or_default()
                ))
                .text_size(px(12.0))
                .text_color(move || theme().destructive)
            },
        ))
        .child(row_delete_button(state, item))
}

/// 行内覆写模板绑定：已绑定显示模板名；模板被外部删除或清单损坏导致
/// 引用失效时显示真实错误提示，不静默改绑。
fn binding_summary(
    state: SubscriptionPageState,
    item: ReadSignal<Subscription>,
) -> impl IntoElement {
    when(
        move || item.get().profile_id.is_some(),
        move || {
            let profiles = state.profiles.get();
            match crate::profiles_view::profile_name(&profiles, item.get().profile_id) {
                Some(name) => text(format!("覆写：{name}"))
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground)
                    .into_element(),
                None => text("覆写模板引用失效，请重新绑定")
                    .text_size(px(12.0))
                    .text_color(move || theme().destructive)
                    .into_element(),
            }
        },
    )
}

/// 删除独立成行（跟在错误文本后），保持操作区简洁。
fn row_delete_button(
    state: SubscriptionPageState,
    item: ReadSignal<Subscription>,
) -> impl IntoElement {
    row_button(state, item, "删除", |state, subscription| {
        state.pending_remove.set(Some(subscription.clone()));
    })
}

fn enabled_switch(
    state: SubscriptionPageState,
    item: ReadSignal<Subscription>,
) -> impl IntoElement {
    let switch_test_id = format!("subscription-enabled-{}", item.get().id);
    view()
        .test_id(&switch_test_id)
        .on_press(move || {
            let subscription = item.get();
            state.bump_serial();
            let result = state
                .service
                .as_ref()
                .map(|service| service.set_enabled(subscription.id, !subscription.enabled));
            match result {
                Some(Ok(())) => {
                    state.reload();
                    state.post_notice("订阅可用状态已更新".to_owned());
                }
                Some(Err(error)) => {
                    state.reload();
                    state.post_error(format!("切换启用状态失败:{error}"));
                }
                None => state.post_error("打开数据根失败".to_owned()),
            }
        })
        .child(
            switch()
                .track_class("w-9 h-5 rounded-full bg-muted checked:bg-primary p-0.5")
                .thumb_class("w-4 h-4 rounded-full bg-background checked:translate-x-4")
                .checked(move || item.get().enabled),
        )
}

/// 行操作按钮。同步操作（启停/生效/删除/编辑保存）经业务层互斥排队，
/// 忙碌时也可触发；异步刷新由 `begin_refresh` 全局单飞防并发提交。
fn row_button(
    state: SubscriptionPageState,
    item: ReadSignal<Subscription>,
    label: &'static str,
    action: impl Fn(&SubscriptionPageState, &Subscription) + 'static,
) -> impl IntoElement {
    button()
        .px_3()
        .py_1()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().border)
        .on_press(move || action(&state, &item.get()))
        .child(text(label).text_size(px(12.0)))
}

/// 手动刷新：使用已保存的 URL/UA，联网在后台线程执行；
/// 全局单飞——有刷新在途时不启动，完成后可再刷新。
fn run_refresh(state: &SubscriptionPageState, id: Uuid) {
    let _ = spawn_refresh(state.clone(), id);
}

/// 设置当前生效订阅；拒绝不存在或已停用项时显示共享层真实错误。
fn run_activate(state: &SubscriptionPageState, id: Uuid) {
    state.bump_serial();
    let result = state.service.as_ref().map(|service| service.set_active(id));
    match result {
        Some(Ok(())) => {
            state.reload();
            state.post_notice("已设为当前生效订阅".to_owned());
        }
        Some(Err(error)) => {
            state.reload();
            state.post_error(format!("设置生效订阅失败:{error}"));
        }
        None => state.post_error("打开数据根失败".to_owned()),
    }
}

fn delete_dialog(state: SubscriptionPageState) -> AnyElement {
    match state.pending_remove.get() {
        Some(target) => {
            let remove_id = target.id;
            let service = state.service.clone();
            let cancel_state = state.clone();
            let cancel_button_state = state.clone();
            let confirm_state = state;
            dialog()
                .on_cancel(move || cancel_state.pending_remove.set(None))
                .child(
                    view()
                        .flex_col()
                        .gap_3()
                        .w(px(360.0))
                        .p_4()
                        .rounded(move || theme().radius)
                        .bg(move || theme().card)
                        .border(px(1.0))
                        .border_color(move || theme().border)
                        .child(text(format!("删除订阅“{}”？", target.name)))
                        .child(
                            text(
                                "仅移除 NSH 桌面数据根中的记录，不影响 Tauri/GPUI/apps/nsh 数据。",
                            )
                            .text_size(px(12.0))
                            .text_color(move || theme().muted_foreground),
                        )
                        .child(
                            view()
                                .flex_row()
                                .gap_2()
                                .justify_end()
                                .child(
                                    button()
                                        .px_3()
                                        .py_1()
                                        .rounded(move || theme().radius)
                                        .border(px(1.0))
                                        .border_color(move || theme().border)
                                        .on_press(move || {
                                            cancel_button_state.pending_remove.set(None)
                                        })
                                        .child(text("取消")),
                                )
                                .child(
                                    button()
                                        .px_3()
                                        .py_1()
                                        .rounded(move || theme().radius)
                                        .bg(move || theme().destructive)
                                        .on_press(move || {
                                            confirm_state.pending_remove.set(None);
                                            confirm_state.bump_serial();
                                            let result = service
                                                .as_ref()
                                                .map(|service| service.remove(remove_id));
                                            match result {
                                                Some(Ok(())) => {
                                                    confirm_state.reload();
                                                    confirm_state.post_notice(
                                                        "订阅与本地缓存已删除".to_owned(),
                                                    );
                                                }
                                                Some(Err(error)) => {
                                                    confirm_state.reload();
                                                    confirm_state.post_error(format!(
                                                        "删除订阅失败:{error}"
                                                    ));
                                                }
                                                None => confirm_state
                                                    .post_error("打开数据根失败".to_owned()),
                                            }
                                        })
                                        .child(
                                            text("确认删除")
                                                .text_color(move || theme().primary_foreground),
                                        ),
                                ),
                        ),
                )
                .into_element()
        }
        None => view().into_element(),
    }
}
