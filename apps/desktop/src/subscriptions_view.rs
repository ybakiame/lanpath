//! 订阅页：NSH 桌面独立数据根上的真实订阅列表。新增/编辑持久化到共享
//! `lanpath-client::SubscriptionStore`；新增成功后自动后台刷新（沿用 GPUI
//! 行为），也可对每条订阅手动刷新。联网仅在新增/刷新时按需发生，不启动
//! 核心、系统代理或 TUN；请求使用已保存的地址/User-Agent，不在 UI 线程
//! 联网，不把未保存表单写回。

mod add_form;
mod editor;
mod rows;
mod source_view;

use std::sync::Arc;

use lanpath_client::{Subscription, node_tags_from_nodes, parse_subscription_body};
use nsh_base::{button, theme};
use nsh_core::prelude::*;
use nsh_core::storage::Storage;
use uuid::Uuid;

use crate::subscriptions::{RefreshStatus, Subscriptions};

const BUSY_REFRESHING: &str = "正在刷新订阅…";

/// 编辑草稿：与列表数据分离，刷新/启停/删除/设置生效不会清空未保存修改。
#[derive(Clone)]
pub(super) struct EditState {
    id: Signal<Option<Uuid>>,
    name: Signal<String>,
    url: Signal<String>,
    user_agent: Signal<String>,
    dirty: Signal<bool>,
    error: Signal<Option<String>>,
    confirm_discard: Signal<bool>,
}

impl EditState {
    fn new() -> Self {
        Self {
            id: signal(None),
            name: signal(String::new()),
            url: signal(String::new()),
            user_agent: signal(String::new()),
            dirty: signal(false),
            error: signal(None),
            confirm_discard: signal(false),
        }
    }

    fn open(&self, subscription: &Subscription) {
        self.id.set(Some(subscription.id));
        self.name.set(subscription.name.clone());
        self.url.set(subscription.url.clone());
        self.user_agent
            .set(subscription.user_agent.clone().unwrap_or_default());
        self.dirty.set(false);
        self.error.set(None);
        self.confirm_discard.set(false);
    }

    fn close(&self) {
        self.id.set(None);
        self.dirty.set(false);
        self.error.set(None);
        self.confirm_discard.set(false);
    }

    /// 取消保护：有未保存修改时先要求确认。
    fn request_cancel(&self) {
        if self.dirty.get() {
            self.confirm_discard.set(true);
        } else {
            self.close();
        }
    }
}

/// 只读原文弹窗内容（最后一次成功刷新的原文）。
#[derive(Clone, PartialEq)]
pub(super) struct SourceView {
    name: String,
    source: lanpath_client::SubscriptionSource,
}

/// 订阅页状态：全部为句柄（Signal/Arc/Stored），克隆廉价。
#[derive(Clone)]
pub(super) struct SubscriptionPageState {
    service: Option<Arc<Subscriptions>>,
    list: Signal<Vec<Subscription>>,
    active: Signal<Option<Uuid>>,
    list_error: Signal<Option<String>>,
    form_error: Signal<Option<String>>,
    notice: Signal<Option<String>>,
    /// 在途刷新的订阅 id（全局最多一个）：业务层互斥串行化写盘，这里
    /// 单飞防并发刷新写回与任务句柄竞争，兼作忙碌提示。
    refreshing: Signal<Vec<Uuid>>,
    /// 交互序号：每次用户操作/刷新开始递增；晚到的异步结果只在序号未变时
    /// 张贴，避免覆盖后续操作的反馈。
    serial: Signal<u64>,
    pending_remove: Signal<Option<Subscription>>,
    edit: EditState,
    source_view: Signal<Option<SourceView>>,
    task: Stored<Option<Spawned<()>>>,
}

impl SubscriptionPageState {
    fn new() -> Self {
        match Subscriptions::open() {
            Ok(service) => {
                let state = Self {
                    service: Some(service),
                    list: signal(Vec::new()),
                    active: signal(None),
                    list_error: signal(None),
                    form_error: signal(None),
                    notice: signal(None),
                    refreshing: signal(Vec::new()),
                    serial: signal(0),
                    pending_remove: signal(None),
                    edit: EditState::new(),
                    source_view: signal(None),
                    task: stored(None),
                };
                state.reload();
                state
            }
            Err(error) => Self {
                service: None,
                list: signal(Vec::new()),
                active: signal(None),
                list_error: signal(Some(format!("打开数据根失败:{error}"))),
                form_error: signal(None),
                notice: signal(None),
                refreshing: signal(Vec::new()),
                serial: signal(0),
                pending_remove: signal(None),
                edit: EditState::new(),
                source_view: signal(None),
                task: stored(None),
            },
        }
    }

    /// 从磁盘重读列表与生效订阅；消息通道（notice/list_error）由 post_*
    /// 独占管理，这里不触碰，避免晚到的刷新重读清掉更新操作的反馈。
    /// 任何失败都显示真实错误，不伪造成功。
    fn reload(&self) {
        let Some(service) = self.service.clone() else {
            return;
        };
        match service.load() {
            Ok(subscriptions) => self.list.set(subscriptions),
            Err(error) => self.list_error.set(Some(format!("加载订阅失败:{error}"))),
        }
        if let Ok(active) = service.active_id() {
            self.active.set(active);
        }
    }

    /// 是否有刷新在途（忙碌提示与单飞去重用）。
    fn any_refreshing(&self) -> bool {
        !self.refreshing.get().is_empty()
    }

    fn bump_serial(&self) -> u64 {
        let next = self.serial.get() + 1;
        self.serial.set(next);
        next
    }

    /// 同步操作结果反馈：总是上屏（引擎线程内与异步完成严格有序）。
    fn post_notice(&self, text: String) {
        self.list_error.set(None);
        self.notice.set(Some(text));
    }

    fn post_error(&self, text: String) {
        self.notice.set(None);
        self.list_error.set(Some(text));
    }

    /// 异步刷新结果的反馈：只在刷新仍是最新交互时张贴。
    fn post_notice_if_current(&self, token: u64, text: String) {
        if self.serial.get() == token {
            self.post_notice(text);
        }
    }

    fn post_error_if_current(&self, token: u64, text: String) {
        if self.serial.get() == token {
            self.post_error(text);
        }
    }

    /// 开始一次刷新；全局只允许一个在途刷新（第二个订阅的刷新需等当前
    /// 完成后再次触发，不是永久禁用）。开始即递增序号并清空旧消息。
    fn begin_refresh(&self, id: Uuid) -> Option<u64> {
        if !self.refreshing.get().is_empty() {
            return None;
        }
        self.refreshing.update(|refreshing| refreshing.push(id));
        let token = self.bump_serial();
        self.list_error.set(None);
        self.notice.set(None);
        Some(token)
    }

    fn finish_refreshing(&self, id: Uuid) {
        self.refreshing
            .update(|refreshing| refreshing.retain(|candidate| *candidate != id));
    }
}

pub(super) fn subscription_page() -> impl IntoElement {
    let state = SubscriptionPageState::new();
    let fixture_report = signal(
        Storage::get_item(FIXTURE_REPORT_KEY).unwrap_or_else(|| "尚未解析共享订阅。".to_owned()),
    );

    view()
        .flex_1()
        .flex_col()
        .gap_4()
        .child(text("订阅").text_size(px(24.0)))
        .child(
            text("真实订阅列表 · 保存到 NSH 桌面独立数据根 · 仅新增/刷新时按需联网，不启动核心/系统代理")
                .text_color(move || theme().muted_foreground),
        )
        .child(add_form::add_subscription_form(state.clone()))
        .child(rows::subscription_list(state.clone()))
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
                    text("调用共享 lanpath-client 解析固定测试订阅，与订阅列表数据无关")
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

/// 共享解析验证（固定测试订阅）：纯本地解析，不联网。
fn parse_fixture_report() -> String {
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

/// 手动/自动刷新共用：全局单飞（在途刷新未完成时新的刷新不启动，完成
/// 后可再刷新）；联网在后台线程执行，结果回到 UI 线程写信号。业务层
/// 互斥保证写回与同步操作不并发写坏清单。返回是否真正启动。
pub(super) fn spawn_refresh(state: SubscriptionPageState, id: Uuid) -> bool {
    let Some(token) = state.begin_refresh(id) else {
        return false;
    };
    let Some(service) = state.service.clone() else {
        state.finish_refreshing(id);
        state.post_error("打开数据根失败".to_owned());
        return false;
    };
    let done = state.clone();
    state.task.with(|slot| {
        slot.replace(spawn(async move {
            let result = background(async move { service.refresh(id).await }).await;
            let result = match result {
                Ok(result) => result,
                Err(error) => Err(lanpath_common::LanpathError::Client(format!(
                    "刷新任务中断:{error}"
                ))),
            };
            done.finish_refreshing(id);
            finish_refresh(done, token, result);
        }));
    });
    true
}

fn finish_refresh(
    state: SubscriptionPageState,
    token: u64,
    result: lanpath_common::LanpathResult<RefreshStatus>,
) {
    match result {
        Ok(RefreshStatus::Updated { node_count, format }) => {
            state.reload();
            state.post_notice_if_current(
                token,
                format!("刷新完成：{node_count} 个节点（{format:?}）"),
            );
        }
        Ok(RefreshStatus::Failed { error }) => {
            // 真实错误已随记录持久化，重读列表让行内错误可见；只在仍是
            // 最新交互时张贴，避免覆盖后续操作的反馈。
            state.reload();
            state.post_error_if_current(token, format!("刷新订阅失败:{error}"));
        }
        Ok(RefreshStatus::Stale) => {
            state.reload();
            state
                .post_notice_if_current(token, "刷新期间订阅已被修改或删除，结果未写入".to_owned());
        }
        Err(error) => {
            state.reload();
            state.post_error_if_current(token, format!("刷新订阅失败:{error}"));
        }
    }
}
