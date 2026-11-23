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
use std::time::Duration;

use lanpath_client::{Profile, Subscription};
use nsh_core::prelude::*;
use uuid::Uuid;

use crate::app_state;
use crate::profiles::Profiles;
use crate::subscriptions::{RefreshStatus, Subscriptions};

/// 刷新在途提示（`t!` 按当前语言取值，测试以中文断言）。
fn busy_refreshing() -> String {
    t!("subscription.refreshing_busy")
}

/// 读取刷新活动初始化快照：revision 必须先于 activity。worker 若在两次
/// 读取之间完成，基线仍是完成前版本，monitor 下一 tick 能看到变化；
/// 反过来会得到“在途 id + 新 revision”，页面永久显示刷新中。
fn read_refresh_initial<A>(
    read_revision: impl FnOnce() -> u64,
    read_activity: impl FnOnce() -> Option<A>,
) -> (u64, Option<A>) {
    (read_revision(), read_activity())
}

/// 编辑草稿：与列表数据分离，刷新/启停/删除/设置生效不会清空未保存修改。
#[derive(Clone)]
pub(super) struct EditState {
    id: Signal<Option<Uuid>>,
    name: Signal<String>,
    url: Signal<String>,
    user_agent: Signal<String>,
    /// 覆写模板绑定草稿：`None` = 无覆写；保存时业务层校验引用真实存在。
    profile_id: Signal<Option<Uuid>>,
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
            profile_id: signal(None),
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
        self.profile_id.set(subscription.profile_id);
        self.dirty.set(false);
        self.error.set(None);
        self.confirm_discard.set(false);
    }

    fn close(&self) {
        self.id.set(None);
        self.profile_id.set(None);
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
    search: Signal<String>,
    draft_name: Signal<String>,
    draft_url: Signal<String>,
    draft_user_agent: Signal<String>,
    /// 覆写模板业务：与订阅服务共用同一数据根的同一把业务锁；
    /// 列表用于绑定选择与行内模板名展示。
    profiles_service: Option<Profiles>,
    list: Signal<Vec<Subscription>>,
    profiles: Signal<Vec<Profile>>,
    active: Signal<Option<Uuid>>,
    list_error: Signal<Option<String>>,
    form_error: Signal<Option<String>>,
    /// 在途刷新的订阅 id（全局最多一个）：业务层互斥串行化写盘，这里
    /// 单飞防并发刷新写回与任务句柄竞争，兼作忙碌提示。
    refreshing: Signal<Vec<Uuid>>,
    /// 初始化时记录的刷新活动版本。必须按“先 revision、后 activity”读取；
    /// worker 在两者之间完成时，monitor 下一 tick 仍能看到版本变化并清 busy。
    refresh_activity_revision: u64,
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
        let profiles_service = Profiles::open().ok();
        let profiles = profiles_service
            .as_ref()
            .and_then(|service| service.load().ok())
            .unwrap_or_default();
        match Subscriptions::open() {
            Ok(service) => {
                // 先记录版本基线，再读取在途活动：若 worker 在两者之间完成，
                // 基线仍是完成前值，monitor 能观察到变化；反过来读会留下
                // 永久 busy。
                let (activity_revision, initial_activity) = read_refresh_initial(
                    || service.refresh_activity_revision(),
                    || service.refresh_activity(),
                );
                let state = Self {
                    service: Some(service),
                    search: signal(String::new()),
                    draft_name: signal(String::new()),
                    draft_url: signal(String::new()),
                    draft_user_agent: signal(String::new()),
                    profiles_service,
                    list: signal(Vec::new()),
                    profiles: signal(profiles),
                    active: signal(None),
                    list_error: signal(None),
                    form_error: signal(None),
                    refreshing: signal(initial_activity.into_iter().collect()),
                    refresh_activity_revision: activity_revision,
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
                search: signal(String::new()),
                draft_name: signal(String::new()),
                draft_url: signal(String::new()),
                draft_user_agent: signal(String::new()),
                profiles_service,
                list: signal(Vec::new()),
                profiles: signal(profiles),
                active: signal(None),
                list_error: signal(Some(format!("打开数据根失败:{error}"))),
                form_error: signal(None),
                refreshing: signal(Vec::new()),
                refresh_activity_revision: 0,
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
    /// 任何失败都显示真实错误，不伪造成功。读完后同步 App 级共享的
    /// 当前生效订阅摘要（概览/全局状态栏与列表同一来源，不独立轮询）。
    fn reload(&self) {
        let Some(service) = self.service.clone() else {
            return;
        };
        match service.load() {
            Ok(subscriptions) => self.list.set(subscriptions),
            Err(error) => {
                self.list_error.set(Some(t!(
                    "subscription.load_failed",
                    error = error.to_string()
                )));
            }
        }
        if let Ok(active) = service.active_id() {
            self.active.set(active);
        }
        if let Some(profiles_service) = self.profiles_service.clone() {
            match profiles_service.load() {
                Ok(profiles) => self.profiles.set(profiles),
                Err(error) => self.list_error.set(Some(t!(
                    "subscription.profiles_load_failed",
                    error = error.to_string()
                ))),
            }
        }
        app_state::refresh_active_subscription(&app_state::current());
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

    /// 同步操作结果反馈：经全局 Toast 上屏，不占用抽屉文档流。
    fn post_notice(&self, text: String) {
        self.list_error.set(None);
        crate::ui::feedback::show_toast(text);
    }

    fn post_error(&self, text: String) {
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
        Some(token)
    }

    fn finish_refreshing(&self, id: Uuid) {
        self.refreshing
            .update(|refreshing| refreshing.retain(|candidate| *candidate != id));
    }
}

pub(super) fn subscription_page() -> impl IntoElement {
    let state = SubscriptionPageState::new();
    spawn_refresh_activity_monitor(state.clone());
    let draft = state.clone();
    crate::navigation::protect(move || {
        if !crate::workspace_panels::is_open() {
            return crate::navigation::LeaveState::Clean;
        }
        if draft.edit.dirty.get()
            || !draft.draft_name.get().is_empty()
            || !draft.draft_url.get().is_empty()
            || !draft.draft_user_agent.get().is_empty()
        {
            crate::navigation::LeaveState::Dirty
        } else {
            crate::navigation::LeaveState::Clean
        }
    });
    view()
        .flex_1()
        .flex_col()
        .gap_4()
        .child(
            text(|| t!("subscription.subtitle"))
                .text_size(px(12.0))
                .text_color(move || nsh_base::theme().muted_foreground),
        )
        .child(add_form::add_subscription_form(state.clone()))
        .child(rows::subscription_list(state))
}

/// 观察同根共享的刷新活动：旧页面发出的刷新在离页后完成时，新页面仍会
/// 通过版本变化清除 busy 并重读真实列表，不依赖旧页面的 UI 回调。
fn spawn_refresh_activity_monitor(state: SubscriptionPageState) {
    let Some(service) = state.service.clone() else {
        return;
    };
    // 使用页面构造时记录的基线，避免重新读取时吞掉初始化窗口内的完成。
    let revision = signal(state.refresh_activity_revision);
    set_interval(Duration::from_millis(200), move || {
        let current = service.refresh_activity_revision();
        if current == revision.get() {
            return;
        }
        revision.set(current);
        let shared = service.refresh_activity();
        state.refreshing.set(shared.into_iter().collect::<Vec<_>>());
        if shared.is_none() {
            state.reload();
        }
    });
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
        state.post_error(t!("subscription.root_unavailable"));
        return false;
    };
    if service.refresh_activity().is_some() {
        state.finish_refreshing(id);
        state.post_notice(t!("subscription.refresh_other_in_progress"));
        return false;
    }
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
                t!(
                    "subscription.refresh_done",
                    count = node_count,
                    format = format!("{format:?}")
                ),
            );
        }
        Ok(RefreshStatus::Failed { error }) => {
            // 真实错误已随记录持久化，重读列表让行内错误可见；只在仍是
            // 最新交互时张贴，避免覆盖后续操作的反馈。
            state.reload();
            state.post_error_if_current(
                token,
                t!("subscription.refresh_failed", error = error.to_string()),
            );
        }
        Ok(RefreshStatus::Stale) => {
            state.reload();
            state.post_notice_if_current(token, t!("subscription.refresh_stale"));
        }
        Ok(RefreshStatus::Busy) => {
            state.refreshing.set(Vec::new());
            state.post_notice_if_current(token, t!("subscription.refresh_other_in_progress"));
        }
        Err(error) => {
            state.reload();
            state.post_error_if_current(
                token,
                t!("subscription.refresh_failed", error = error.to_string()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::read_refresh_initial;

    /// 可控完成窗口：worker 在 revision 读取后、activity 读取前完成。
    /// 基线必须保留完成前版本，monitor 下一次 tick 才能清掉初始 busy。
    #[test]
    fn refresh_snapshot_keeps_pre_completion_revision() {
        let revision = Cell::new(3_u64);
        let activity = Cell::new(Some("refresh-id"));
        let (base, initial) = read_refresh_initial(
            || revision.get(),
            || {
                revision.set(4);
                activity.set(None);
                activity.get()
            },
        );
        assert_eq!(base, 3, "基线必须是完成前的 revision");
        assert_eq!(initial, None, "完成后共享活动已清除");
        assert_ne!(revision.get(), base, "下一 tick 能看到版本变化");
    }
}
