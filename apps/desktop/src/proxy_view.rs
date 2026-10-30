mod render;

use std::sync::Arc;

use gpui_kit::component::list::{ListEvent, ListState};
use gpui_kit::{AppContext as _, Context, Entity, EventEmitter, Subscription, Task, Window};
use lanpath_client::proxies::{GroupView, ProxyList};

use crate::{
    proxy_list::ProxyListDelegate,
    proxy_service::{ModeChange, ProxyService},
    runtime::DesktopRuntime,
};

#[derive(Clone, Copy)]
enum Operation {
    Refresh,
    Select,
    TestNode,
    TestGroup,
    SetMode,
}

pub(crate) struct ProxyConfigurationChanged;

pub(crate) struct ProxyView {
    core: Arc<DesktopRuntime>,
    service: ProxyService,
    list: Entity<ListState<ProxyListDelegate>>,
    groups: Vec<GroupView>,
    active_group: Option<String>,
    mode: String,
    running: bool,
    closing: bool,
    locked: bool,
    loaded: bool,
    generation: u64,
    operation: Option<Operation>,
    load_error: Option<String>,
    action_error: Option<String>,
    notice: Option<String>,
    task: Option<Task<()>>,
    _list_subscription: Subscription,
}

impl EventEmitter<ProxyConfigurationChanged> for ProxyView {}

impl ProxyView {
    pub(crate) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let service = ProxyService::new(core.business().ok().cloned());
        let mode = service.rule_mode().unwrap_or_else(|_| "rule".into());
        let list =
            cx.new(|cx| crate::keyboard::list_state(ProxyListDelegate::default(), window, cx));
        let subscription =
            cx.subscribe_in(&list, window, |this, state, event: &ListEvent, _, cx| {
                if let ListEvent::Confirm(ix) = event {
                    let Some(name) = state.read(cx).delegate().member(ix.row) else {
                        return;
                    };
                    this.select_proxy(name, cx);
                }
            });
        Self {
            core,
            service,
            list,
            groups: Vec::new(),
            active_group: None,
            mode,
            running: false,
            closing: false,
            locked: false,
            loaded: false,
            generation: 0,
            operation: None,
            load_error: None,
            action_error: None,
            notice: None,
            task: None,
            _list_subscription: subscription,
        }
    }

    pub(crate) fn busy(&self) -> bool {
        self.operation.is_some()
    }

    pub(crate) fn set_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        if self.locked != locked {
            self.locked = locked;
            cx.notify();
        }
    }

    pub(crate) fn set_closing(&mut self, closing: bool, cx: &mut Context<Self>) {
        self.closing = closing;
        cx.notify();
    }

    pub(crate) fn set_running(&mut self, running: bool, cx: &mut Context<Self>) {
        self.generation += 1;
        self.running = running;
        self.loaded = false;
        self.load_error = None;
        self.action_error = None;
        self.notice = None;
        self.groups.clear();
        self.active_group = None;
        self.list.update(cx, |list, cx| {
            list.delegate_mut().clear();
            cx.notify();
        });
        if running {
            self.refresh(cx);
        }
        cx.notify();
    }

    fn apply_list(&mut self, list: ProxyList, cx: &mut Context<Self>) {
        let active_group = self
            .active_group
            .as_ref()
            .filter(|name| list.groups.iter().any(|group| &group.name == *name))
            .cloned()
            .or_else(|| {
                list.groups
                    .iter()
                    .find(|group| group.name == "proxy")
                    .or_else(|| list.groups.first())
                    .map(|group| group.name.clone())
            });
        let selected = active_group
            .as_ref()
            .and_then(|name| list.groups.iter().find(|group| &group.name == name))
            .cloned();
        self.active_group = active_group;
        self.groups = list.groups;
        self.list.update(cx, |state, cx| {
            state.delegate_mut().set_nodes(list.nodes);
            state.delegate_mut().set_group(selected);
            cx.notify();
        });
        self.loaded = true;
        self.load_error = None;
    }

    fn select_group(&mut self, name: String, cx: &mut Context<Self>) {
        if self.locked || self.operation.is_some() {
            return;
        }
        let group = self.groups.iter().find(|group| group.name == name).cloned();
        if group.is_none() {
            return;
        }
        self.active_group = Some(name);
        self.action_error = None;
        self.notice = None;
        self.list.update(cx, |state, cx| {
            state.delegate_mut().set_group(group);
            cx.notify();
        });
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.running || self.closing || self.operation.is_some() {
            return;
        }
        self.operation = Some(Operation::Refresh);
        self.load_error = None;
        self.task = None;
        let generation = self.generation;
        let core = self.core.clone();
        let service = self.service.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.block_on(async { service.list().await })
                        .and_then(|result| result)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.operation = None;
                if generation == this.generation && this.running {
                    match result {
                        Ok(list) => this.apply_list(list, cx),
                        Err(error) => this.load_error = Some(error),
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn select_proxy(&mut self, name: String, cx: &mut Context<Self>) {
        if !self.running || self.closing || self.locked || self.operation.is_some() {
            self.reset_list_selection(cx);
            return;
        }
        let (group, is_selector) = {
            let state = self.list.read(cx);
            (
                state.delegate().group_name(),
                state.delegate().is_selector(),
            )
        };
        let Some(group) = group.filter(|_| is_selector) else {
            return;
        };
        self.operation = Some(Operation::Select);
        self.action_error = None;
        self.notice = None;
        let generation = self.generation;
        let core = self.core.clone();
        let service = self.service.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let (name, result) = cx
                .background_spawn(async move {
                    let result = core
                        .block_on(async { service.select(&group, &name).await })
                        .and_then(|result| result);
                    (name, result)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.operation = None;
                if generation == this.generation && this.running {
                    match result {
                        Ok(list) => {
                            this.apply_list(list, cx);
                            this.notice = Some(format!("已切换并保存：{name}"));
                        }
                        Err(error) => {
                            this.action_error = Some(error);
                            this.reset_list_selection(cx);
                        }
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn test_selected(&mut self, cx: &mut Context<Self>) {
        if !self.running || self.closing || self.locked || self.operation.is_some() {
            return;
        }
        let Some(name) = self.list.read(cx).delegate().selected_member() else {
            return;
        };
        self.operation = Some(Operation::TestNode);
        self.action_error = None;
        self.notice = None;
        self.list.update(cx, |state, cx| {
            state.delegate_mut().set_testing(Some(name.clone()));
            cx.notify();
        });
        let generation = self.generation;
        let core = self.core.clone();
        let service = self.service.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let (name, result) = cx
                .background_spawn(async move {
                    let result = core
                        .block_on(async { service.test_delay(&name).await })
                        .and_then(|result| result);
                    (name, result)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.operation = None;
                this.list.update(cx, |state, cx| {
                    state.delegate_mut().set_testing(None);
                    if generation == this.generation
                        && this.running
                        && let Ok(delay) = result.as_ref()
                    {
                        state.delegate_mut().apply_delay(&name, *delay);
                    }
                    cx.notify();
                });
                if generation == this.generation && this.running {
                    match result {
                        Ok(Some(delay)) => this.notice = Some(format!("{name}：{delay}ms")),
                        Ok(None) => this.notice = Some(format!("{name}：超时")),
                        Err(error) => this.action_error = Some(error),
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn test_group(&mut self, cx: &mut Context<Self>) {
        if !self.running || self.closing || self.locked || self.operation.is_some() {
            return;
        }
        let Some(group) = self.list.read(cx).delegate().group_name() else {
            return;
        };
        self.operation = Some(Operation::TestGroup);
        self.action_error = None;
        self.notice = None;
        let generation = self.generation;
        let core = self.core.clone();
        let service = self.service.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let (group, result) = cx
                .background_spawn(async move {
                    let result = core
                        .block_on(async { service.test_group(&group).await })
                        .and_then(|result| result);
                    (group, result)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.operation = None;
                if generation == this.generation && this.running {
                    match result {
                        Ok(delays) => {
                            this.list.update(cx, |state, cx| {
                                state.delegate_mut().apply_delays(&delays);
                                cx.notify();
                            });
                            let responsive = delays
                                .iter()
                                .filter(|result| result.delay_ms.is_some())
                                .count();
                            this.notice = Some(format!(
                                "{group} 测速完成：{responsive}/{} 个节点响应",
                                delays.len()
                            ));
                        }
                        Err(error) => this.action_error = Some(error),
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn set_mode(&mut self, mode: &'static str, cx: &mut Context<Self>) {
        if self.closing || self.locked || self.operation.is_some() || self.mode == mode {
            return;
        }
        self.operation = Some(Operation::SetMode);
        self.action_error = None;
        self.notice = None;
        let running = self.running;
        let service = self.service.clone();
        let core = self.core.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    core.block_on(async { service.set_rule_mode(mode, running).await })
                        .and_then(|result| result)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.operation = None;
                match result {
                    Ok(change) => this.apply_mode_change(change, cx),
                    Err(error) => this.action_error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn apply_mode_change(&mut self, change: ModeChange, cx: &mut Context<Self>) {
        self.mode = change.mode;
        self.notice = Some(match change.runtime_warning {
            Some(warning) => format!("模式已保存；运行核心同步失败：{warning}"),
            None if self.running => "模式已保存并同步到运行核心".into(),
            None => "模式已保存，将在下次启动时生效".into(),
        });
        cx.emit(ProxyConfigurationChanged);
    }

    fn reset_list_selection(&mut self, cx: &mut Context<Self>) {
        self.list.update(cx, |state, cx| {
            state.delegate_mut().reset_selection();
            cx.notify();
        });
    }

    fn status(&self) -> String {
        if let Some(operation) = self.operation {
            return match operation {
                Operation::Refresh => "正在读取代理策略…".into(),
                Operation::Select => "正在切换并保存节点…".into(),
                Operation::TestNode => "正在测试所选节点…".into(),
                Operation::TestGroup => "正在并行测试当前策略组…".into(),
                Operation::SetMode => "正在保存并同步路由模式…".into(),
            };
        }
        if !self.running {
            return "模式可离线保存；启动隔离核心后可读取策略组、选择节点和测速".into();
        }
        if let Some(error) = &self.load_error {
            return format!("策略读取失败，保留最后成功快照：{error}");
        }
        if self.loaded {
            format!(
                "{} 个策略组；节点选择会持久化并在启动时恢复",
                self.groups.len()
            )
        } else {
            "正在读取代理策略…".into()
        }
    }
}
