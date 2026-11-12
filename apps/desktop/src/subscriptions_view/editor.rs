//! 订阅编辑弹窗：名称/地址/可选 User-Agent 草稿与已保存数据分离，
//! 刷新/启停/删除/设置生效不会清空未保存修改；取消时有未保存修改需确认。

use nsh_base::{button, dialog, input, theme};
use nsh_core::prelude::*;

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
        .unwrap_or_else(|| "未知订阅".to_owned());
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
        .child(text(format!("编辑订阅“{title}”")).text_size(px(15.0)))
        .child(
            text("地址变更会清除旧原文/缓存，保存后需重新刷新")
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(
            input()
                .test_id("edit-name")
                .value(state.edit.name)
                .placeholder("订阅名称")
                .on_change(move |text: &str| {
                    name_state.edit.name.set(text.to_owned());
                    name_state.edit.dirty.set(true);
                })
                .p_2()
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().input)
                .bg(move || theme().background),
        )
        .child(
            input()
                .test_id("edit-url")
                .value(state.edit.url)
                .placeholder("订阅地址（https://…）")
                .auto_correct(false)
                .on_change(move |text: &str| {
                    url_state.edit.url.set(text.to_owned());
                    url_state.edit.dirty.set(true);
                })
                .p_2()
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().input)
                .bg(move || theme().background),
        )
        .child(
            input()
                .test_id("edit-user-agent")
                .value(state.edit.user_agent)
                .placeholder("User-Agent（可选，留空用共享默认 clash.meta）")
                .auto_correct(false)
                .on_change(move |text: &str| {
                    ua_state.edit.user_agent.set(text.to_owned());
                    ua_state.edit.dirty.set(true);
                })
                .p_2()
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().input)
                .bg(move || theme().background),
        )
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
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .border(px(1.0))
                        .border_color(move || theme().border)
                        .on_press(move || state.edit.request_cancel())
                        .child(text("取消")),
                )
                .child(
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .bg(move || theme().primary)
                        .on_press(move || save_edit(&save_press))
                        .child(text("保存修改").text_color(move || theme().primary_foreground)),
                ),
        )
}

fn discard_confirm(state: SubscriptionPageState) -> impl IntoElement {
    let keep_state = state.clone();
    view()
        .flex_col()
        .gap_3()
        .w(px(420.0))
        .p_4()
        .rounded(move || theme().radius)
        .bg(move || theme().card)
        .border(px(1.0))
        .border_color(move || theme().border)
        .child(text("放弃未保存的修改？"))
        .child(
            text("草稿尚未保存，放弃后将丢失本次修改。")
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
                        .on_press(move || state.edit.confirm_discard.set(false))
                        .child(text("继续编辑")),
                )
                .child(
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .bg(move || theme().destructive)
                        .on_press(move || keep_state.edit.close())
                        .child(text("放弃修改").text_color(move || theme().primary_foreground)),
                ),
        )
}

/// 保存编辑：写回共享存储，失败保留草稿并显示真实错误。
fn save_edit(state: &SubscriptionPageState) {
    let Some(id) = state.edit.id.get() else {
        return;
    };
    let Some(service) = state.service.clone() else {
        state.edit.error.set(Some("打开数据根失败".to_owned()));
        return;
    };
    let name = state.edit.name.get();
    let url = state.edit.url.get();
    let user_agent = state.edit.user_agent.get();
    let saved_url = state
        .list
        .get()
        .iter()
        .find(|subscription| subscription.id == id)
        .map(|subscription| subscription.url.clone());
    match service.update(id, &name, &url, Some(&user_agent)) {
        Ok(updated) => {
            let url_changed = saved_url.is_some_and(|saved| saved != updated.url);
            state.edit.close();
            state.reload();
            state.bump_serial();
            state.post_notice(if url_changed {
                "已保存修改；URL 已变更，请重新刷新获取新内容".to_owned()
            } else {
                "已保存修改".to_owned()
            });
        }
        Err(error) => state.edit.error.set(Some(format!("保存修改失败:{error}"))),
    }
}
