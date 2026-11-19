//! 订阅列表（阶段 1 语义组件迁移）：清晰列表行——启用开关、名称、当前
//! 生效标记、节点数/格式、行内刷新状态；常用动作“刷新/编辑/原文/设为
//! 生效”直接可见，删除收进“更多”菜单并必须确认（同时说明本地缓存移除）。
//! CRUD、绑定、草稿、刷新全局单飞与缓存语义不变（见 `super`）。展示与
//! 操作文案全部经 `t!`（zh-CN/en），共享业务错误原文作为参数原样保留。

use lanpath_client::Subscription;
use nsh_base::{menu, menu_item, theme};
use nsh_core::prelude::*;
use uuid::Uuid;

use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::layout::SurfaceLevel;
use crate::ui::overlay::confirm_dialog;

use super::{SubscriptionPageState, spawn_refresh};

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
                text(move || t!("subscription.count", count = count_state.list.get().len()))
                    .text_color(move || theme().muted_foreground),
            )
            .child(when(
                move || empty_state.list.get().is_empty() && empty_state.list_error.get().is_none(),
                || text(|| t!("subscription.empty")).text_color(move || theme().muted_foreground),
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
                    text(super::busy_refreshing())
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground)
                },
            ))
            .child(each(
                row_state.list,
                |subscription: &Subscription| subscription.id,
                move |item, _index| subscription_row(row_state.clone(), item),
            ))
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
    crate::ui::surface(SurfaceLevel::Card)
        .test_id(&row_test_id)
        .flex_col()
        .gap_1()
        .p_3()
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
                    move || active_state.active.get() == Some(id),
                    || {
                        text(|| t!("subscription.active_mark"))
                            .text_size(px(12.0))
                            .text_color(move || theme().primary)
                    },
                ))
                .child(
                    text(move || {
                        if item.get().enabled {
                            t!("subscription.enabled_on")
                        } else {
                            t!("subscription.enabled_off")
                        }
                    })
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground),
                )
                .child(view().flex_1())
                .child(when(
                    move || refresh_status_state.refreshing.get().contains(&id),
                    || {
                        text(move || t!("subscription.refreshing"))
                            .text_size(px(12.0))
                            .text_color(move || theme().muted_foreground)
                    },
                ))
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
                .child(more_menu(more_state.clone(), item)),
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
            .text_size(px(12.0))
            .text_color(move || theme().muted_foreground),
        )
        .child(binding_summary(more_state.clone(), item))
        .child(when(
            move || item.get().error.is_some(),
            move || {
                text(t!(
                    "subscription.error_prefix",
                    error = item.get().error.clone().unwrap_or_default()
                ))
                .text_size(px(12.0))
                .text_color(move || theme().destructive)
            },
        ))
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

/// 启用开关：NSH 原生 Switch 语义（role=switch、可访问名称、checked 状态、
/// 键盘焦点与 Enter/Space 激活由组件原生提供），样式用 host token 类
/// （与 `proxy_card` 同一套）。实测行内 `track_style` 闭包会使该子树不进
/// NSH 测试镜像，故 track/thumb 用类字符串。按压走 `on_change` 真实业务
/// 切换；自动化经 `id:<row> >> role:switch` 或 test_id 包裹锚点触达。
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

/// “更多”菜单：低频/危险动作进入菜单；删除选中后弹确认框（确认才删除）。
fn more_menu(state: SubscriptionPageState, item: ReadSignal<Subscription>) -> AnyElement {
    let menu_state = state;
    let trigger = button(ButtonVariant::Ghost)
        .size(ButtonSize::Sm)
        .label(|| t!("subscription.more"));
    menu()
        .trigger(trigger)
        .child(
            // test_id 挂在菜单项内部锚点上（同 Button：外层 wrapper 不在
            // 按压/选择的祖先链上，自动化按 id 找不到目标）。
            menu_item()
                .text_color(move || theme().destructive)
                .child(
                    view()
                        .test_id("subscription-delete")
                        .child(text(|| t!("subscription.action.delete")).text_size(px(13.0))),
                )
                .on_select(move || {
                    menu_state.pending_remove.set(Some(item.get()));
                }),
        )
        .into_element()
}

/// 行操作按钮（语义 Button，Ghost 变体）：同步操作（启停/生效/删除/编辑
/// 保存）经业务层互斥排队，忙碌时也可触发；异步刷新由 `begin_refresh`
/// 全局单飞防并发提交。
fn row_button(
    state: SubscriptionPageState,
    item: ReadSignal<Subscription>,
    label: impl Fn() -> String + 'static,
    action: impl Fn(&SubscriptionPageState, &Subscription) + 'static,
) -> impl IntoElement {
    button(ButtonVariant::Ghost)
        .size(ButtonSize::Sm)
        .label(label)
        .on_press(move || action(&state, &item.get()))
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

/// 删除确认（语义 ConfirmDialog，危险变体）：取消只关弹窗；确认才删除
/// 并刷新列表（同时同步 App 级共享摘要）；文案明确本地缓存一并移除、
/// 不影响其他应用数据。
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
