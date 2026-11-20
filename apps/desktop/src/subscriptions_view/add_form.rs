//! 新增订阅表单（阶段 1 语义组件迁移）：名称/地址/可选 User-Agent；
//! 校验失败不覆盖已有数据。新增成功后清空表单，并由页面自动后台刷新。

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::ui::button::{ButtonSize, ButtonVariant, button};
use crate::ui::input;

use super::SubscriptionPageState;

pub(super) fn add_subscription_form(state: SubscriptionPageState) -> impl IntoElement {
    let name = state.draft_name;
    let url = state.draft_url;
    let user_agent = state.draft_user_agent;
    let error_state = state.clone();
    let has_error = state.form_error;

    let submit_state = state.clone();
    let submit_name = name;
    let submit_url = url;
    let submit_ua = user_agent;
    let submit = std::rc::Rc::new(move || {
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
    });

    crate::ui::surface(crate::ui::SurfaceLevel::Card)
        .p_4()
        .flex_col()
        .gap_2()
        .child(
            view()
                .flex_col()
                .gap_3()
                .child(view().flex_1().min_w(px(0.0)).child(field_input(
                    "subscription-name",
                    name,
                    submit.clone(),
                    || t!("subscription.name_placeholder"),
                )))
                .child(view().flex_1().min_w(px(0.0)).child(field_input(
                    "subscription-url",
                    url,
                    submit.clone(),
                    || t!("subscription.url_placeholder"),
                ))),
        )
        .child(field_input(
            "subscription-user-agent",
            user_agent,
            submit.clone(),
            || t!("subscription.ua_placeholder"),
        ))
        .child(
            button(ButtonVariant::Primary)
                .size(ButtonSize::Md)
                .label(|| t!("subscription.add"))
                .on_press(move || submit()),
        )
        .child(when(
            move || has_error.get().is_some(),
            move || {
                text(move || error_state.form_error.get().unwrap_or_default())
                    .text_color(move || theme().destructive)
            },
        ))
}

fn field_input(
    test_id: &'static str,
    value: Signal<String>,
    submit: std::rc::Rc<dyn Fn()>,
    placeholder: impl Fn() -> String + 'static,
) -> impl IntoElement {
    let on_change = move |text: String| value.set(text);
    let label = std::rc::Rc::new(placeholder);
    let accessible = label.clone();
    input::field().label(move || label()).control(
        input::input(value)
            .test_id(test_id)
            .accessible_name(move || accessible())
            .on_change(on_change)
            .on_submit(move || submit()),
    )
}

/// 同步校验并新增（业务层互斥，刷新在途也可安全新增）；成功后由调用方
/// 清空表单，页面接着自动刷新（GPUI parity）。返回是否新增成功。
fn run_add(state: &SubscriptionPageState, name: String, url: String, user_agent: String) -> bool {
    let Some(service) = state.service.clone() else {
        state
            .list_error
            .set(Some(t!("subscription.root_unavailable")));
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
                state.notice.set(Some(t!("subscription.adding_refreshing")));
            } else {
                state.post_notice(t!("subscription.added_refresh_busy"));
            }
            true
        }
        Err(error) => {
            state.form_error.set(Some(t!(
                "subscription.add_failed",
                error = error.to_string()
            )));
            false
        }
    }
}
