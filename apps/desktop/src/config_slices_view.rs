mod editing;
mod inputs;
mod outbound_editing;
mod render;
mod render_dns;
mod render_outbounds;
mod render_route;
mod sync;

use std::{collections::HashSet, sync::Arc};

use gpui_kit::{
    AppContext as _, Context, EventEmitter, Subscription, Task, Window, component::WindowExt as _,
};
use lanpath_client::config_slices::{ConfigSlices, DnsSlice};

use crate::{
    business::{BusinessService, ConfigSlicesDocument},
    config_slices_view::inputs::{FormField, FormInputs},
    runtime::DesktopRuntime,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Section {
    Dns,
    Outbounds,
    RouteCache,
}

#[derive(Clone, Copy)]
enum Operation {
    Save,
    Probe,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OutboundKind {
    Vless,
    Vmess,
    Shadowsocks,
    Trojan,
    Hysteria2,
    Selector,
    UrlTest,
}

pub(crate) struct ConfigSlicesChanged;

pub(crate) struct ConfigSlicesView {
    service: Option<BusinessService>,
    saved: Option<ConfigSlices>,
    draft: Option<ConfigSlices>,
    builtin_dns: DnsSlice,
    section: Section,
    selected_dns_server: Option<usize>,
    selected_dns_rule: Option<usize>,
    selected_outbound: Option<String>,
    inputs: FormInputs,
    syncing: bool,
    dirty: bool,
    locked: bool,
    operation: Option<Operation>,
    input_errors: HashSet<FormField>,
    error: Option<String>,
    notice: Option<String>,
    task: Option<Task<()>>,
    _input_subscriptions: Vec<Subscription>,
}

impl EventEmitter<ConfigSlicesChanged> for ConfigSlicesView {}

impl ConfigSlicesView {
    pub(crate) fn new(
        core: Arc<DesktopRuntime>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (service, document) = match core.business() {
            Ok(service) => {
                let service = service.clone();
                let document = service.config_slices().map_err(|error| error.to_string());
                (Some(service), document)
            }
            Err(error) => (None, Err(error)),
        };
        let (inputs, subscriptions) = FormInputs::new(window, cx);
        let mut view = Self {
            service,
            saved: None,
            draft: None,
            builtin_dns: DnsSlice::default(),
            section: Section::Dns,
            selected_dns_server: None,
            selected_dns_rule: None,
            selected_outbound: None,
            inputs,
            syncing: false,
            dirty: false,
            locked: false,
            operation: None,
            input_errors: HashSet::new(),
            error: None,
            notice: None,
            task: None,
            _input_subscriptions: subscriptions,
        };
        match document {
            Ok(document) => view.apply_document(document, window, cx),
            Err(error) => view.error = Some(error),
        }
        view
    }

    pub(crate) fn dirty(&self) -> bool {
        self.dirty
    }

    pub(crate) fn busy(&self) -> bool {
        self.operation.is_some()
    }

    pub(crate) fn set_locked(&mut self, locked: bool, cx: &mut Context<Self>) {
        self.locked = locked;
        self.inputs.set_locked(locked, cx);
        cx.notify();
    }

    fn set_section(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        if self.reject_invalid_input(cx) {
            return;
        }
        self.section = section;
        self.sync_inputs(window, cx);
        cx.notify();
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        if self.dirty {
            let view = cx.weak_entity();
            window.open_alert_dialog(cx, move |dialog, _, _| {
                let view = view.clone();
                dialog
                    .title("放弃结构化配置修改？")
                    .description("重新载入会丢弃未保存的 DNS、出站、路由和缓存修改。")
                    .confirm()
                    .ok_text("放弃并重新载入")
                    .cancel_text("取消")
                    .on_ok(move |_, window, cx| {
                        let _ = view.update(cx, |view, cx| view.load_now(window, cx));
                        true
                    })
            });
        } else {
            self.load_now(window, cx);
        }
    }

    fn load_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self
            .service
            .as_ref()
            .ok_or_else(|| "实验数据目录不可用".to_string())
            .and_then(|service| service.config_slices().map_err(|error| error.to_string()));
        match result {
            Ok(document) => {
                self.apply_document(document, window, cx);
                self.notice = Some("已重新载入已保存的结构化配置".into());
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }

    pub(crate) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.locked || self.busy() || !self.dirty || self.reject_invalid_input(cx) {
            return;
        }
        let Some(service) = self.service.clone() else {
            self.error = Some("实验数据目录不可用".into());
            cx.notify();
            return;
        };
        let Some(mut slices) = self.draft.clone() else {
            return;
        };
        slices.dns = if slices.dns == self.builtin_dns {
            DnsSlice::default()
        } else {
            let mut dns = slices.dns;
            dns.mode = lanpath_client::config_slices::DnsMode::Takeover;
            dns
        };
        self.operation = Some(Operation::Save);
        self.error = None;
        self.notice = None;
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    service
                        .save_config_slices(&slices)
                        .map_err(|error| error.to_string())
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    this.operation = None;
                    match result {
                        Ok(document) => {
                            this.apply_document(document, window, cx);
                            this.notice = Some("结构化配置已校验并保存".into());
                            cx.emit(ConfigSlicesChanged);
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

    fn probe_selected_dns(&mut self, cx: &mut Context<Self>) {
        if self.locked || self.busy() {
            return;
        }
        let Some(index) = self.selected_dns_server else {
            return;
        };
        let Some(server) = self
            .draft
            .as_ref()
            .and_then(|slices| slices.dns.servers.get(index))
            .cloned()
        else {
            return;
        };
        let Some(service) = self.service.clone() else {
            return;
        };
        let tag = server.tag.clone();
        self.operation = Some(Operation::Probe);
        self.error = None;
        self.notice = None;
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    service
                        .probe_dns_server(server)
                        .await
                        .map_err(|error| error.to_string())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.operation = None;
                match result {
                    Ok(latency) => this.notice = Some(format!("DNS {tag}：{latency}ms")),
                    Err(error) => this.error = Some(format!("DNS {tag} 探测失败：{error}")),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn apply_document(
        &mut self,
        document: ConfigSlicesDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.builtin_dns = document.builtin_dns;
        let mut display = document.slices;
        if display.dns.mode == lanpath_client::config_slices::DnsMode::FollowSystem {
            display.dns = self.builtin_dns.clone();
        }
        self.selected_dns_server = (!display.dns.servers.is_empty()).then_some(0);
        self.selected_dns_rule = (!display.dns.rules.is_empty()).then_some(0);
        self.selected_outbound = display.outbounds.items.first().map(|item| item.id.clone());
        self.saved = Some(display.clone());
        self.draft = Some(display);
        self.dirty = false;
        self.input_errors.clear();
        self.error = None;
        self.sync_inputs(window, cx);
    }

    fn reject_invalid_input(&mut self, cx: &mut Context<Self>) -> bool {
        if self.input_errors.is_empty() {
            return false;
        }
        self.error = Some("数值字段格式无效；修正后再继续".into());
        cx.notify();
        true
    }
}
