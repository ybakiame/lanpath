//! 订阅页：NSH 桌面独立数据根上的真实订阅列表，以及调用共享
//! `lanpath-client` 解析固定测试订阅的共享解析验证。与订阅列表无关的
//! 纯解析检查不发起网络请求。

use std::sync::Arc;

use lanpath_client::{Subscription, node_tags_from_nodes, parse_subscription_body};
use nsh_base::{button, dialog, input, switch, theme};
use nsh_core::prelude::*;
use nsh_core::storage::Storage;

use crate::subscriptions::Subscriptions;

const FIXTURE: &str = r#"proxies:
  - name: 东京测试节点
    type: ss
    server: 192.0.2.1
    port: 8388
    cipher: aes-256-gcm
    password: test-only
  - name: 新加坡测试节点
    type: ss
    server: 192.0.2.2
    port: 8388
    cipher: aes-256-gcm
    password: test-only
"#;

/// Parses the fixed test subscription with the shared client, the same call
/// the other targets make. Kept as an independent shared-parse check; the
/// subscription list below never fetches.
fn parse_fixture_report() -> String {
    match parse_subscription_body(FIXTURE, None) {
        Ok(result) => {
            let nodes = node_tags_from_nodes(&result.singbox_nodes);
            let names = nodes
                .iter()
                .map(|node| node.name.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "解析成功：{} 个节点\n格式：{:?}\n警告：{} 条\n\n{}",
                nodes.len(),
                result.format,
                result.warnings.len(),
                names
            )
        }
        Err(error) => format!("解析失败：{error}"),
    }
}

/// 订阅页状态：共享存储与两个错误通道（列表加载/操作 vs 表单新增）。
/// 全部为句柄（Signal/Arc），克隆廉价。
#[derive(Clone)]
struct SubscriptionPageState {
    service: Option<Arc<Subscriptions>>,
    list: Signal<Vec<Subscription>>,
    list_error: Signal<Option<String>>,
    form_error: Signal<Option<String>>,
    pending_remove: Signal<Option<Subscription>>,
}

impl SubscriptionPageState {
    fn new() -> Self {
        match Subscriptions::open() {
            Ok(service) => {
                let (list, list_error) = match service.load() {
                    Ok(list) => (list, None),
                    Err(error) => (Vec::new(), Some(format!("加载订阅失败:{error}"))),
                };
                Self {
                    service: Some(service),
                    list: signal(list),
                    list_error: signal(list_error),
                    form_error: signal(None),
                    pending_remove: signal(None),
                }
            }
            Err(error) => Self {
                service: None,
                list: signal(Vec::new()),
                list_error: signal(Some(format!("打开数据根失败:{error}"))),
                form_error: signal(None),
                pending_remove: signal(None),
            },
        }
    }

    fn refresh(&self) {
        let Some(service) = self.service.clone() else {
            return;
        };
        match service.load() {
            Ok(subscriptions) => {
                self.list.set(subscriptions);
                self.list_error.set(None);
            }
            Err(error) => self.list_error.set(Some(format!("加载订阅失败:{error}"))),
        }
    }
}

pub(super) fn subscription_page() -> impl IntoElement {
    let state = SubscriptionPageState::new();
    let name = signal(String::new());
    let url = signal(String::new());
    let fixture_report = signal(
        Storage::get_item(FIXTURE_REPORT_KEY).unwrap_or_else(|| "尚未解析共享订阅。".to_owned()),
    );

    view()
        .flex_1()
        .flex_col()
        .gap_4()
        .child(text("订阅").text_size(px(24.0)))
        .child(
            text("真实订阅列表 · 保存到 NSH 桌面独立数据根 · 不发起网络请求")
                .text_color(move || theme().muted_foreground),
        )
        .child(add_subscription_form(state.clone(), name, url))
        .child(subscription_list(state))
        .child(
            view()
                .flex_col()
                .gap_2()
                .p_3()
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().border)
                .child(text("共享解析验证").text_size(px(15.0)))
                .child(
                    text("调用共享 lanpath-client 解析固定测试订阅，与订阅列表无关")
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                )
                .child(
                    button()
                        .px_4()
                        .py_2()
                        .rounded(move || theme().radius)
                        .bg(move || theme().secondary)
                        .on_press(move || {
                            // 解析验证结果随 NSH storage 持久化，与订阅列表数据无关。
                            let report = parse_fixture_report();
                            let _ = Storage::set_item(FIXTURE_REPORT_KEY, &report);
                            fixture_report.set(report);
                        })
                        .child(
                            text("解析共享订阅").text_color(move || theme().secondary_foreground),
                        ),
                )
                .child(
                    text(move || fixture_report.get())
                        .selectable(true)
                        .text_size(px(12.0))
                        .text_color(move || theme().muted_foreground),
                ),
        )
}

const FIXTURE_REPORT_KEY: &str = "fixture_parse_report";

fn add_subscription_form(
    state: SubscriptionPageState,
    name: Signal<String>,
    url: Signal<String>,
) -> impl IntoElement {
    let submit_state = state.clone();
    let submit = move || {
        let Some(service) = submit_state.service.clone() else {
            return;
        };
        match service.add(&name.get(), &url.get()) {
            Ok(_) => {
                name.set(String::new());
                url.set(String::new());
                submit_state.form_error.set(None);
                submit_state.refresh();
            }
            Err(error) => submit_state
                .form_error
                .set(Some(format!("新增订阅失败:{error}"))),
        }
    };
    let error_state = state.clone();
    view()
        .flex_col()
        .gap_2()
        .child(
            input()
                .test_id("subscription-name")
                .value(name)
                .placeholder("订阅名称")
                .on_change(move |text: &str| name.set(text.to_owned()))
                .p_2()
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().input)
                .bg(move || theme().background),
        )
        .child(
            input()
                .test_id("subscription-url")
                .value(url)
                .placeholder("订阅地址（https://…）")
                .auto_correct(false)
                .on_change(move |text: &str| url.set(text.to_owned()))
                .p_2()
                .rounded(move || theme().radius)
                .border(px(1.0))
                .border_color(move || theme().input)
                .bg(move || theme().background),
        )
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

fn subscription_list(state: SubscriptionPageState) -> impl IntoElement {
    let count_state = state.clone();
    let empty_state = state.clone();
    let error_state = state.clone();
    let row_state = state.clone();
    let dialog_state = state.clone();
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
                text(move || error_state.list_error.get().unwrap_or_default())
                    .text_color(move || theme().destructive),
            )
            .child(each(
                state.list,
                |subscription: &Subscription| subscription.id,
                move |item, _index| subscription_row(row_state.clone(), item),
            ))
            .child(when(
                move || dialog_state.pending_remove.get().is_some(),
                move || delete_dialog(dialog_state.clone()),
            )),
    )
}

fn subscription_row(
    state: SubscriptionPageState,
    item: ReadSignal<Subscription>,
) -> impl IntoElement {
    let row_test_id = format!("subscription-row-{}", item.get().name);
    let toggle_state = state.clone();
    let delete_state = state;
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
                .child(
                    view()
                        .test_id(&format!("subscription-enabled-{}", item.get().name))
                        .on_press(move || {
                            let subscription = item.get();
                            let result = toggle_state.service.as_ref().map(|service| {
                                service.set_enabled(subscription.id, !subscription.enabled)
                            });
                            toggle_state.refresh();
                            if let Some(Err(error)) = result {
                                toggle_state
                                    .list_error
                                    .set(Some(format!("切换启用状态失败:{error}")));
                            }
                        })
                        .child(
                            switch()
                                .track_class(
                                    "w-9 h-5 rounded-full bg-muted checked:bg-primary p-0.5",
                                )
                                .thumb_class(
                                    "w-4 h-4 rounded-full bg-background checked:translate-x-4",
                                )
                                .checked(move || item.get().enabled),
                        ),
                )
                .child(
                    text(move || item.get().name.clone())
                        .text_size(px(15.0))
                        .truncate()
                        .min_w(px(0.0)),
                )
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
                .child(
                    button()
                        .px_3()
                        .py_1()
                        .rounded(move || theme().radius)
                        .border(px(1.0))
                        .border_color(move || theme().destructive)
                        .on_press(move || delete_state.pending_remove.set(Some(item.get().clone())))
                        .child(text("删除").text_color(move || theme().destructive)),
                ),
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
        .child(when(
            move || item.get().error.is_some(),
            move || {
                text(format!("错误：{}", item.get().error.unwrap_or_default()))
                    .text_size(px(12.0))
                    .text_color(move || theme().destructive)
            },
        ))
}

fn delete_dialog(state: SubscriptionPageState) -> AnyElement {
    match state.pending_remove.get() {
        Some(target) => {
            let remove_id = target.id;
            let service = state.service.clone();
            let cancel_state = state.clone();
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
                                        .on_press(move || cancel_state.pending_remove.set(None))
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
                                            let result = service
                                                .as_ref()
                                                .map(|service| service.remove(remove_id));
                                            confirm_state.refresh();
                                            if let Some(Err(error)) = result {
                                                confirm_state
                                                    .list_error
                                                    .set(Some(format!("删除订阅失败:{error}")));
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
