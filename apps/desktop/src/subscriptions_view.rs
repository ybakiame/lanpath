use std::sync::Arc;

use gpui_kit::component::{
    WindowExt as _,
    input::{EditorState, InputEvent, InputState},
};
use gpui_kit::{
    AppContext as _, Context, Entity, EventEmitter, Subscription as GpuiSubscription, Task, Window,
};
use lanpath_client::{Profile, Subscription};
use uuid::Uuid;

use crate::{business::BusinessService, runtime::DesktopRuntime};

mod render;

pub(crate) struct SubscriptionsChanged;

#[derive(Clone, Copy)]
enum Operation {
    Save,
    Refresh,
    Activate,
    Toggle,
    Delete,
    Export,
}

pub(crate) struct SubscriptionsView {
    core: Arc<DesktopRuntime>,
    service: Option<BusinessService>,
    subscriptions: Vec<Subscription>,
    profiles: Vec<Profile>,
    active: Option<Uuid>,
    selected: Option<Uuid>,
    draft_profile: Option<Uuid>,
    name: Entity<InputState>,
    url: Entity<InputState>,
    user_agent: Entity<InputState>,
    source: Entity<EditorState>,
    dirty: bool,
    locked: bool,
    busy: Option<Operation>,
    error: Option<String>,
    notice: Option<String>,
    task: Option<Task<()>>,
    _input_subscriptions: Vec<GpuiSubscription>,
}

impl EventEmitter<SubscriptionsChanged> for SubscriptionsView {}

impl SubscriptionsView {
    pub(crate) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let service = core.business().ok().cloned();
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("订阅名称"));
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("https://…"));
        let user_agent = cx.new(|cx| InputState::new(window, cx).placeholder("默认 clash.meta"));
        let source = cx.new(|cx| EditorState::new(window, cx).line_number(true));
        source.update(cx, |state, cx| state.set_readonly(true, cx));
        let mut input_subscriptions = Vec::new();
        for input in [&name, &url, &user_agent] {
            input_subscriptions.push(cx.subscribe(input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.dirty = true;
                    this.error = None;
                    this.notice = None;
                    cx.notify();
                }
            }));
        }
        let mut view = Self {
            core,
            subscriptions: service
                .as_ref()
                .and_then(|service| service.subscriptions().ok())
                .unwrap_or_default(),
            profiles: service
                .as_ref()
                .and_then(|service| service.profiles().ok())
                .unwrap_or_default(),
            active: service
                .as_ref()
                .and_then(|service| service.active_subscription_id().ok())
                .flatten(),
            service,
            selected: None,
            draft_profile: None,
            name,
            url,
            user_agent,
            source,
            dirty: false,
            locked: false,
            busy: None,
            error: None,
            notice: None,
            task: None,
            _input_subscriptions: input_subscriptions,
        };
        if let Some(id) = view
            .active
            .or_else(|| view.subscriptions.first().map(|item| item.id))
        {
            view.select(Some(id), window, cx);
        }
        view
    }

    pub(crate) fn dirty(&self) -> bool {
        self.dirty
    }

    pub(crate) fn busy(&self) -> bool {
        self.busy.is_some()
    }

    pub(crate) fn set_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        self.locked = locked;
        for input in [&self.name, &self.url, &self.user_agent] {
            input.update(cx, |state, cx| state.set_readonly(locked, cx));
        }
        cx.notify();
    }

    fn select(&mut self, id: Option<Uuid>, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = id;
        let selected = id.and_then(|id| self.subscriptions.iter().find(|item| item.id == id));
        self.draft_profile = selected.and_then(|item| item.profile_id);
        set_input(
            &self.name,
            selected.map(|item| item.name.as_str()).unwrap_or(""),
            window,
            cx,
        );
        set_input(
            &self.url,
            selected.map(|item| item.url.as_str()).unwrap_or(""),
            window,
            cx,
        );
        set_input(
            &self.user_agent,
            selected
                .and_then(|item| item.user_agent.as_deref())
                .unwrap_or(""),
            window,
            cx,
        );
        self.load_source(window, cx);
        self.dirty = false;
        self.error = None;
        self.notice = None;
        cx.notify();
    }

    fn load_source(&self, window: &mut Window, cx: &mut Context<Self>) {
        let source = self
            .selected
            .and_then(|id| {
                self.service
                    .as_ref()?
                    .subscription_source(id)
                    .ok()
                    .flatten()
            })
            .map(|source| source.content)
            .unwrap_or_default();
        self.source
            .update(cx, |state, cx| state.set_value(source, window, cx));
    }

    fn request_select(&mut self, id: Option<Uuid>, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        if !self.dirty {
            self.select(id, window, cx);
            return;
        }
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .title("放弃未保存的订阅修改？")
                .description("切换条目会丢弃当前表单修改，已保存订阅不受影响。")
                .confirm()
                .ok_text("放弃修改")
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| {
                        if !view.locked && !view.busy() {
                            view.select(id, window, cx);
                        }
                    });
                    true
                })
        });
    }

    fn request_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selected) = self.selected else {
            return;
        };
        let view = cx.weak_entity();
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .title("删除订阅及本地缓存？")
                .description(
                    "删除后不会影响生产 Lanpath 数据，但实验目录中的原文与节点缓存会一并移除。",
                )
                .confirm()
                .ok_text("删除")
                .cancel_text("取消")
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| {
                        if view.selected == Some(selected) {
                            view.run(Operation::Delete, window, cx);
                        }
                    });
                    true
                })
        });
    }
    pub(crate) fn reload_profiles(&mut self, cx: &mut Context<Self>) {
        let Some(service) = self.service.as_ref() else {
            self.error = Some("实验数据目录不可用".into());
            cx.notify();
            return;
        };
        match service.profiles() {
            Ok(profiles) => {
                self.profiles = profiles;
                if self
                    .draft_profile
                    .is_some_and(|id| !self.profiles.iter().any(|profile| profile.id == id))
                {
                    self.draft_profile = None;
                    self.dirty = true;
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        cx.notify();
    }

    fn cycle_profile(&mut self, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        let next = match self.draft_profile {
            None => self.profiles.first().map(|profile| profile.id),
            Some(id) => self
                .profiles
                .iter()
                .position(|profile| profile.id == id)
                .and_then(|index| self.profiles.get(index + 1).map(|profile| profile.id)),
        };
        self.draft_profile = next;
        self.dirty = true;
        self.error = None;
        cx.notify();
    }

    pub(crate) fn save_from_shortcut(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_some() && self.dirty {
            self.run(Operation::Save, window, cx);
        }
    }

    fn run(&mut self, operation: Operation, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        let selected = self.selected;
        let name = self.name.read(cx).value().to_string();
        let url = self.url.read(cx).value().to_string();
        let user_agent = self.user_agent.read(cx).value().to_string();
        let profile = self.draft_profile;
        let enabled = selected
            .and_then(|id| self.subscriptions.iter().find(|item| item.id == id))
            .is_some_and(|item| item.enabled);
        if !matches!(operation, Operation::Save) && selected.is_none() {
            return;
        }
        self.busy = Some(operation);
        self.error = None;
        self.notice = None;
        let Some(service) = self.service.clone() else {
            self.busy = None;
            self.error = Some("实验数据目录不可用".into());
            cx.notify();
            return;
        };
        let core = self.core.clone();
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    match operation {
                        Operation::Save => {
                            let result = core.block_on(async {
                                match selected {
                                    Some(id) => service.update_subscription(
                                        id,
                                        &name,
                                        &url,
                                        Some(&user_agent),
                                        profile,
                                    ),
                                    None => {
                                        service
                                            .add_subscription(
                                                &name,
                                                &url,
                                                Some(&user_agent),
                                                profile,
                                            )
                                            .await
                                    }
                                }
                            })?;
                            result.map_err(|error| error.to_string())?;
                        }
                        Operation::Refresh => {
                            let id = selected.ok_or_else(|| "未选择订阅".to_string())?;
                            let result = core.block_on(service.refresh_subscription(id))?;
                            result.map_err(|error| error.to_string())?;
                        }
                        Operation::Activate => service
                            .set_active_subscription(selected)
                            .map_err(|error| error.to_string())?,
                        Operation::Toggle => {
                            let id = selected.ok_or_else(|| "未选择订阅".to_string())?;
                            service
                                .set_subscription_enabled(id, !enabled)
                                .map_err(|error| error.to_string())?;
                        }
                        Operation::Delete => {
                            let id = selected.ok_or_else(|| "未选择订阅".to_string())?;
                            service
                                .remove_subscription(id)
                                .map_err(|error| error.to_string())?;
                        }
                        Operation::Export => {
                            let id = selected.ok_or_else(|| "未选择订阅".to_string())?;
                            service
                                .export_subscription_source(id)
                                .map_err(|error| error.to_string())?;
                        }
                    }
                    Ok::<_, String>((
                        service.subscriptions().map_err(|error| error.to_string())?,
                        service
                            .active_subscription_id()
                            .map_err(|error| error.to_string())?,
                    ))
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.busy = None;
                    match result {
                        Ok((subscriptions, active)) => {
                            this.subscriptions = subscriptions;
                            this.active = active;
                            // Cache and availability commands do not commit the edit form.
                            if matches!(operation, Operation::Save | Operation::Delete) {
                                let next = if matches!(operation, Operation::Delete) {
                                    this.subscriptions.first().map(|item| item.id)
                                } else {
                                    selected
                                        .or_else(|| this.subscriptions.last().map(|item| item.id))
                                };
                                this.select(next, window, cx);
                            } else if matches!(operation, Operation::Refresh) {
                                this.load_source(window, cx);
                            }
                            this.notice = Some(match operation {
                                Operation::Save => "订阅已保存；URL 变化后需要重新刷新".into(),
                                Operation::Refresh if this.dirty => {
                                    "已刷新已保存 URL 的缓存；未保存的表单修改保持不变".into()
                                }
                                Operation::Refresh => "订阅刷新完成".into(),
                                Operation::Activate => "已设为当前生效订阅".into(),
                                Operation::Toggle => "订阅可用状态已更新".into(),
                                Operation::Delete => "订阅与本地缓存已删除".into(),
                                Operation::Export => format!(
                                    "原文已导出到 {}",
                                    this.service
                                        .as_ref()
                                        .map(|service| service.data_dir().join("exports"))
                                        .unwrap_or_default()
                                        .display()
                                ),
                            });
                            cx.emit(SubscriptionsChanged);
                        }
                        Err(error) => this.error = Some(error),
                    }
                    cx.notify();
                })
                .ok()
            });
        }));
        cx.notify();
    }

    fn profile_label(&self) -> String {
        self.draft_profile
            .and_then(|id| self.profiles.iter().find(|profile| profile.id == id))
            .map(|profile| format!("覆写：{}", profile.name))
            .unwrap_or_else(|| "覆写：无（点击切换）".into())
    }
}

fn set_input(
    input: &Entity<InputState>,
    value: &str,
    window: &mut Window,
    cx: &mut Context<SubscriptionsView>,
) {
    input.update(cx, |state, cx| {
        state.set_value(value.to_string(), window, cx)
    });
}
