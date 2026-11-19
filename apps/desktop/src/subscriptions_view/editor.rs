//! 订阅编辑弹窗（阶段 1 语义组件迁移）：名称/地址/可选 User-Agent/覆写
//! 模板绑定草稿与已保存数据分离，刷新/启停/删除/设置生效不会清空未保存
//! 修改；取消时有未保存修改需确认。覆写绑定用单选列表（无覆写 + 每个
//! 现存模板），保存时业务层在同一业务锁内校验模板真实存在，失效引用
//! 显示真实错误、不静默改绑。

use nsh_base::{dialog, radio, radio_group, theme};
use nsh_core::prelude::*;
use uuid::Uuid;

use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::input;
use crate::ui::overlay::confirm_dialog;

use super::SubscriptionPageState;

pub(super) fn edit_dialog(state: SubscriptionPageState) -> AnyElement {
    let Some(id) = state.edit.id.get() else {
        return view().into_element();
    };
    let title = state
        .list
        .get()
        .iter()
        .find(|subscription| subscription.id == id)
        .map(|subscription| subscription.name.clone())
        .unwrap_or_else(|| t!("subscription.unknown"));
    let cancel_state = state.clone();
    let form_show = state.clone();
    let form_build = state.clone();
    let confirm_show = state.clone();
    let confirm_build = state;
    dialog()
        .on_cancel(move || cancel_state.edit.request_cancel())
        .child(
            view()
                .child(when(
                    move || !form_show.edit.confirm_discard.get(),
                    move || edit_form(form_build.clone(), title.clone()),
                ))
                .child(when(
                    move || confirm_show.edit.confirm_discard.get(),
                    move || discard_confirm(confirm_build.clone()),
                )),
        )
        .into_element()
}

fn edit_form(state: SubscriptionPageState, title: String) -> impl IntoElement {
    let name_state = state.clone();
    let url_state = state.clone();
    let ua_state = state.clone();
    let save_press = state.clone();
    view()
        .flex_col()
        .gap_2()
        .w(px(420.0))
        .p_4()
        .rounded(move || theme().radius)
        .bg(move || theme().card)
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(text(t!("subscription.edit_title", name = title)).text_size(px(15.0)))
        .child(
            text(|| t!("subscription.edit_url_hint"))
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            input::input(state.edit.name)
                .test_id("edit-name")
                .placeholder(|| t!("subscription.name_placeholder"))
                .on_change(move |text: String| {
                    name_state.edit.name.set(text);
                    name_state.edit.dirty.set(true);
                }),
        )
        .child(
            input::input(state.edit.url)
                .test_id("edit-url")
                .placeholder(|| t!("subscription.url_placeholder"))
                .on_change(move |text: String| {
                    url_state.edit.url.set(text);
                    url_state.edit.dirty.set(true);
                }),
        )
        .child(
            input::input(state.edit.user_agent)
                .test_id("edit-user-agent")
                .placeholder(|| t!("subscription.ua_placeholder"))
                .on_change(move |text: String| {
                    ua_state.edit.user_agent.set(text);
                    ua_state.edit.dirty.set(true);
                }),
        )
        .child(
            text(|| t!("subscription.binding_label"))
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(binding_choices(state.clone()))
        .child(
            text(move || state.edit.error.get().unwrap_or_default())
                .text_size(px(12.0))
                .text_color(move || theme().destructive),
        )
        .child(
            view()
                .flex_row()
                .gap_2()
                .justify_end()
                .child(
                    button(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .label(|| t!("subscription.cancel"))
                        .on_press(move || state.edit.request_cancel()),
                )
                .child(
                    button(ButtonVariant::Primary)
                        .size(ButtonSize::Sm)
                        .label(|| t!("subscription.save"))
                        .on_press(move || save_edit(&save_press)),
                ),
        )
}

/// 放弃未保存草稿的确认（语义 ConfirmDialog，危险变体）。
fn discard_confirm(state: SubscriptionPageState) -> impl IntoElement {
    let keep_state = state.clone();
    confirm_dialog(move || state.edit.confirm_discard.get())
        .test_id("subscription-discard-dialog")
        .destructive()
        .title(|| t!("subscription.discard_title"))
        .description(|| t!("subscription.discard_description"))
        .confirm_label(|| t!("subscription.discard_confirm"))
        .cancel_label(|| t!("subscription.discard_cancel"))
        .on_cancel(move || state.edit.confirm_discard.set(false))
        .on_confirm(move || keep_state.edit.close())
}

/// 覆写模板绑定选择：无覆写 + 每个现存模板的单选列表（模板多时也清晰
/// 可选）。选择即写入草稿并标脏；保存时业务层校验引用存在。
fn binding_choices(state: SubscriptionPageState) -> impl IntoElement {
    let mut group = radio_group().flex_col().gap_2().child(binding_radio(
        state.clone(),
        None,
        t!("subscription.bind_none"),
        "bind-profile-none",
    ));
    let profiles = state.profiles.get();
    for profile in profiles {
        group = group.child(binding_radio(
            state.clone(),
            Some(profile.id),
            profile.name.clone(),
            &format!("bind-profile-{}", profile.id),
        ));
    }
    group
}

fn binding_radio(
    state: SubscriptionPageState,
    target: Option<Uuid>,
    label: String,
    test_id: &str,
) -> impl IntoElement {
    let checked = move || state.edit.profile_id.get() == target;
    radio()
        .checked(checked)
        .on_change(move |_: &bool| {
            state.edit.profile_id.set(target);
            state.edit.dirty.set(true);
        })
        .child(
            view()
                .test_id(test_id)
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    view()
                        .size(px(16.0))
                        .rounded_full()
                        .border(px(1.0))
                        .border_color(move || theme().muted_foreground)
                        .bg(move || {
                            if checked() {
                                theme().primary
                            } else {
                                theme().card
                            }
                        }),
                )
                .child(text(label).text_size(px(13.0))),
        )
}

/// 保存编辑：写回共享存储，失败保留草稿并显示真实错误。
fn save_edit(state: &SubscriptionPageState) {
    let Some(id) = state.edit.id.get() else {
        return;
    };
    let Some(service) = state.service.clone() else {
        state
            .edit
            .error
            .set(Some(t!("subscription.root_unavailable")));
        return;
    };
    let name = state.edit.name.get();
    let url = state.edit.url.get();
    let user_agent = state.edit.user_agent.get();
    let profile_id = state.edit.profile_id.get();
    let saved_url = state
        .list
        .get()
        .iter()
        .find(|subscription| subscription.id == id)
        .map(|subscription| subscription.url.clone());
    match service.update(id, &name, &url, Some(&user_agent), profile_id) {
        Ok(updated) => {
            let url_changed = saved_url.is_some_and(|saved| saved != updated.url);
            state.edit.close();
            state.reload();
            state.bump_serial();
            state.post_notice(if url_changed {
                t!("subscription.url_changed_notice")
            } else {
                t!("subscription.saved_notice")
            });
        }
        Err(error) => state.edit.error.set(Some(t!(
            "subscription.save_failed",
            error = error.to_string()
        ))),
    }
}
