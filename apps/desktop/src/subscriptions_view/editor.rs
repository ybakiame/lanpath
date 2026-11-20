//! 订阅编辑使用固定头尾的原生 Dialog；放弃确认不卸载正在编辑的表单。
use super::SubscriptionPageState;
use crate::ui::{
    button::{ButtonSize, ButtonVariant, button},
    input,
};
use nsh_base::{radio, radio_group, theme};
use nsh_core::prelude::*;
use uuid::Uuid;

pub(super) fn edit_dialog(state: SubscriptionPageState) -> AnyElement {
    let Some(id) = state.edit.id.get() else {
        return view().into_element();
    };
    let title = state
        .list
        .get()
        .iter()
        .find(|item| item.id == id)
        .map(|item| item.name.clone())
        .unwrap_or_else(|| t!("subscription.unknown"));
    let focus = element_ref();
    let name_state = state.clone();
    let url_state = state.clone();
    let ua_state = state.clone();
    let cancel = state.clone();
    let footer_cancel = state.clone();
    let save = state.clone();
    let error = state.edit.error;
    let confirm = state.edit.confirm_discard;
    let discard = state.clone();
    let body = scroll().flex_1().min_h(px(0.0)).child(
        view()
            .flex_col()
            .gap_4()
            .child(
                text(|| t!("subscription.edit_url_hint"))
                    .text_size(px(12.0))
                    .text_color(move || theme().muted_foreground),
            )
            .child(
                input::field()
                    .label(|| t!("subscription.name_placeholder"))
                    .control(
                        input::input(state.edit.name)
                            .element_ref(focus)
                            .test_id("edit-name")
                            .accessible_name(|| t!("subscription.name_placeholder"))
                            .on_change(move |value| {
                                name_state.edit.name.set(value);
                                name_state.edit.dirty.set(true);
                            }),
                    ),
            )
            .child(
                input::field()
                    .label(|| t!("subscription.url_placeholder"))
                    .control(
                        input::input(state.edit.url)
                            .test_id("edit-url")
                            .accessible_name(|| t!("subscription.url_placeholder"))
                            .on_change(move |value| {
                                url_state.edit.url.set(value);
                                url_state.edit.dirty.set(true);
                            }),
                    ),
            )
            .child(
                input::field()
                    .label(|| t!("subscription.ua_placeholder"))
                    .control(
                        input::input(state.edit.user_agent)
                            .test_id("edit-user-agent")
                            .accessible_name(|| t!("subscription.ua_placeholder"))
                            .on_change(move |value| {
                                ua_state.edit.user_agent.set(value);
                                ua_state.edit.dirty.set(true);
                            }),
                    ),
            )
            .child(text(|| t!("subscription.binding_label")).text_size(px(14.0)))
            .child(binding_choices(state.clone()))
            .child(when(
                move || error.get().is_some(),
                move || {
                    text(move || error.get().unwrap_or_default())
                        .text_color(move || theme().destructive)
                },
            )),
    );
    let footer = view()
        .flex_row()
        .justify_end()
        .gap_3()
        .child(
            button(ButtonVariant::Outline)
                .size(ButtonSize::Md)
                .label(|| t!("subscription.cancel"))
                .on_press(move || footer_cancel.edit.request_cancel()),
        )
        .child(
            button(ButtonVariant::Primary)
                .size(ButtonSize::Md)
                .label(|| t!("subscription.save"))
                .on_press(move || save_edit(&save)),
        );
    view()
        .child(crate::ui::dialog::form_dialog(
            move || t!("subscription.edit_title", name = title.clone()),
            crate::ui::dialog::DialogSize::Form,
            focus,
            move || cancel.edit.request_cancel(),
            body,
            footer,
        ))
        .child(
            crate::ui::confirm_dialog(move || confirm.get())
                .test_id("subscription-discard-dialog")
                .title(|| t!("subscription.discard_title"))
                .description(|| t!("subscription.discard_description"))
                .destructive()
                .confirm_label(|| t!("subscription.discard_confirm"))
                .cancel_label(|| t!("subscription.discard_cancel"))
                .on_cancel(move || confirm.set(false))
                .on_confirm(move || discard.edit.close()),
        )
        .into_element()
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
