//! 新增订阅表单：名称/地址/可选 User-Agent；校验失败不覆盖已有数据。
//! 新增成功后清空表单，并由页面自动后台刷新新订阅。

use nsh_base::{button, input, theme};
use nsh_core::prelude::*;

use super::SubscriptionPageState;

pub(super) fn add_subscription_form(state: SubscriptionPageState) -> impl IntoElement {
    let name = signal(String::new());
    let url = signal(String::new());
    let user_agent = signal(String::new());
    let error_state = state.clone();

    let submit_state = state.clone();
    let submit_name = name;
    let submit_url = url;
    let submit_ua = user_agent;
    let submit = move || {
        if run_add(
            &submit_state,
            submit_name.get(),
            submit_url.get(),
            submit_ua.get(),
        ) {
            submit_name.set(String::new());
            submit_url.set(String::new());
            submit_ua.set(String::new());
        }
    };

    view()
        .flex_col()
        .gap_2()
        .child(field_input("subscription-name", name, "订阅名称", false))
        .child(field_input(
            "subscription-url",
            url,
            "订阅地址（https://…）",
            false,
        ))
        .child(field_input(
            "subscription-user-agent",
            user_agent,
            "User-Agent（可选，留空用共享默认 clash.meta）",
            false,
        ))
        .child(
            button()
                .px_4()
                .py_2()
                .rounded(move || theme().radius)
                .items_center()
                .bg(move || theme().primary)
                .on_press(submit)
                .child(text("新增订阅").text_color(move || theme().primary_foreground)),
        )
        .child(
            text(move || error_state.form_error.get().unwrap_or_default())
                .text_color(move || theme().destructive),
        )
}

fn field_input(
    test_id: &'static str,
    value: Signal<String>,
    placeholder: &'static str,
    auto_correct: bool,
) -> impl IntoElement {
    let on_change = move |text: &str| value.set(text.to_owned());
    input()
        .test_id(test_id)
        .value(value)
        .placeholder(placeholder)
        .auto_correct(auto_correct)
        .on_change(on_change)
        .p_2()
        .rounded(move || theme().radius)
        .border(px(1.0))
        .border_color(move || theme().input)
        .bg(move || theme().background)
}

/// 同步校验并新增（业务层互斥，刷新在途也可安全新增）；成功后由调用方
/// 清空表单，页面接着自动刷新（GPUI parity）。返回是否新增成功。
fn run_add(state: &SubscriptionPageState, name: String, url: String, user_agent: String) -> bool {
    let Some(service) = state.service.clone() else {
        state.list_error.set(Some("打开数据根失败".to_owned()));
        return false;
    };
    match service.add(&name, &url, Some(&user_agent)) {
        Ok(subscription) => {
            state.form_error.set(None);
            state.reload();
            // 自动刷新走已保存的 URL/UA，不把未保存表单写回。先开始刷新
            // （清消息、发序号），再挂中间提示，刷新完成的反馈仍可上屏；
            // 单飞被拒（有别的刷新在途）时明确提示稍后手动刷新。
            if super::spawn_refresh(state.clone(), subscription.id) {
                state.notice.set(Some("已新增订阅，正在刷新…".to_owned()));
            } else {
                state.post_notice("已新增订阅；有刷新在途，请稍后手动刷新".to_owned());
            }
            true
        }
        Err(error) => {
            state.form_error.set(Some(format!("新增订阅失败:{error}")));
            false
        }
    }
}
