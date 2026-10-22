mod appearance;
mod appearance_view;
mod business;
mod config_slices_view;
mod config_view;
mod configuration;
mod connection_list;
mod connections_view;
mod data_root;
mod disk_log_writer;
mod log_buffer;
mod log_capture;
mod log_list;
mod log_stream;
mod logs_view;
mod mitm_view;
mod observation;
mod profiles_view;
mod proxy_list;
mod proxy_service;
mod proxy_view;
mod request_settings_view;
mod rules_view;
mod runtime;
mod scripts_view;
mod shell_view;
mod shutdown_view;
mod stats_list;
mod stats_view;
mod subscriptions_view;
#[cfg(windows)]
mod windows_file;

use appearance_view::AppearanceView;
use std::sync::Arc;

use config_slices_view::{ConfigSlicesChanged, ConfigSlicesView};
use config_view::{ConfigEvent, ConfigView};
use connections_view::ConnectionsView;
use gpui_kit::component::WindowExt as _;
use gpui_kit::{
    AppContext as _, Context, Entity, Subscription, Task, TitlebarOptions, Window, WindowOptions,
};
use logs_view::LogsView;
use mitm_view::{MitmChanged, MitmView};
use profiles_view::{ProfilesChanged, ProfilesView};
use proxy_view::{ProxyConfigurationChanged, ProxyView};
use request_settings_view::RequestSettingsView;
use rules_view::{RulesChanged, RulesView};
use runtime::DesktopRuntime;
use scripts_view::ScriptsView;
use stats_view::StatsView;
use subscriptions_view::{SubscriptionsChanged, SubscriptionsView};

#[derive(Clone, Copy)]
enum CoreOperationKind {
    Start,
    Stop,
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Subscriptions,
    Profiles,
    Structured,
    Rules,
    Configuration,
    Scripts,
    Mitm,
    Proxies,
    Connections,
    Stats,
    Logs,
    Requests,
    Appearance,
}

struct DesktopView {
    page: Page,
    configuration: Entity<ConfigView>,
    subscriptions: Entity<SubscriptionsView>,
    profiles: Entity<ProfilesView>,
    structured: Entity<ConfigSlicesView>,
    rules: Entity<RulesView>,
    scripts: Entity<ScriptsView>,
    mitm: Entity<MitmView>,
    core: Arc<DesktopRuntime>,
    proxy: Entity<ProxyView>,
    connections: Entity<ConnectionsView>,
    stats: Entity<StatsView>,
    logs: Entity<LogsView>,
    requests: Entity<RequestSettingsView>,
    appearance: Entity<AppearanceView>,
    closing: bool,
    close_task: Option<Task<()>>,
    core_running: bool,
    core_busy: Option<CoreOperationKind>,
    core_task: Option<Task<()>>,
    core_error: Option<String>,
    _proxy_configuration_subscription: Subscription,
    _configuration_subscription: Subscription,
    _business_subscriptions: Vec<Subscription>,
}

impl DesktopView {
    fn new(core: Arc<DesktopRuntime>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let configuration = cx.new(|cx| ConfigView::new(core.clone(), window, cx));
        let _configuration_subscription =
            cx.subscribe(&configuration, |_, _, _: &ConfigEvent, cx| cx.notify());
        let subscriptions = cx.new(|cx| SubscriptionsView::new(core.clone(), window, cx));
        let structured = cx.new(|cx| ConfigSlicesView::new(core.clone(), window, cx));
        let rules = cx.new(|cx| RulesView::new(core.clone(), window, cx));
        let profiles = cx.new(|cx| ProfilesView::new(core.clone(), window, cx));
        let scripts = cx.new(|cx| ScriptsView::new(core.clone(), window, cx));
        let mitm = cx.new(|cx| MitmView::new(core.clone(), window, cx));
        let mut _business_subscriptions = vec![
            cx.subscribe_in(
                &subscriptions,
                window,
                |this, _, _: &SubscriptionsChanged, window, cx| {
                    this.configuration
                        .update(cx, |view, cx| view.refresh(window, cx));
                    cx.notify();
                },
            ),
            cx.subscribe_in(
                &profiles,
                window,
                |this, _, _: &ProfilesChanged, window, cx| {
                    this.subscriptions
                        .update(cx, |view, cx| view.reload_profiles(cx));
                    this.configuration
                        .update(cx, |view, cx| view.refresh(window, cx));
                    cx.notify();
                },
            ),
            cx.subscribe_in(
                &structured,
                window,
                |this, _, _: &ConfigSlicesChanged, window, cx| {
                    this.configuration
                        .update(cx, |view, cx| view.refresh(window, cx));
                    cx.notify();
                },
            ),
            cx.subscribe_in(&rules, window, |this, _, _: &RulesChanged, window, cx| {
                this.configuration
                    .update(cx, |view, cx| view.refresh(window, cx));
                cx.notify();
            }),
            cx.subscribe_in(&mitm, window, |this, _, _: &MitmChanged, window, cx| {
                this.configuration
                    .update(cx, |view, cx| view.refresh(window, cx));
                cx.notify();
            }),
        ];
        let proxy = cx.new(|cx| ProxyView::new(core.clone(), window, cx));
        let _proxy_configuration_subscription = cx.subscribe_in(
            &proxy,
            window,
            |this, _, _: &ProxyConfigurationChanged, window, cx| {
                this.configuration
                    .update(cx, |view, cx| view.refresh(window, cx));
                cx.notify();
            },
        );
        let connections = cx.new(|cx| ConnectionsView::new(core.clone(), window, cx));
        let stats = cx.new(|cx| StatsView::new(core.clone(), window, cx));
        let logs = cx.new(|cx| LogsView::new(core.clone(), window, cx));
        let requests = cx.new(|cx| RequestSettingsView::new(core.clone(), window, cx));
        let appearance = cx.new(|cx| AppearanceView::new(window, cx));
        _business_subscriptions.extend([
            cx.observe(&configuration, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&subscriptions, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&profiles, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&structured, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&rules, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&scripts, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&mitm, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&requests, |_, _, cx| cx.notify()),
        ]);
        let view = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            view.update(cx, |view, cx| view.request_close(window, cx))
                .is_err()
        });

        let mut view = Self {
            page: Page::Subscriptions,
            configuration,
            subscriptions,
            structured,
            rules,
            profiles,
            scripts,
            mitm,
            core,
            proxy,
            connections,
            stats,
            logs,
            requests,
            appearance,
            closing: false,
            close_task: None,
            core_running: false,
            core_busy: None,
            core_task: None,
            core_error: None,
            _proxy_configuration_subscription,
            _configuration_subscription,
            _business_subscriptions,
        };
        view.sync_request_lock(cx);
        view
    }

    fn run_core_operation(
        &mut self,
        kind: CoreOperationKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let invalid_state = match kind {
            CoreOperationKind::Start => {
                self.core_running
                    || !self.configuration.read(cx).ready_for_start()
                    || self.structured.read(cx).dirty()
                    || self.rules.read(cx).dirty()
                    || self.scripts.read(cx).dirty()
                    || self.mitm.read(cx).dirty()
                    || self.requests.read(cx).dirty()
            }
            CoreOperationKind::Stop => !self.core_running,
        };
        if self.core_busy.is_some()
            || self.closing
            || self.proxy.read(cx).busy()
            || self.configuration.read(cx).busy()
            || self.subscriptions.read(cx).busy()
            || self.profiles.read(cx).busy()
            || self.structured.read(cx).busy()
            || self.rules.read(cx).busy()
            || self.scripts.read(cx).busy()
            || self.mitm.read(cx).busy()
            || self.requests.read(cx).busy()
            || !self.core.is_configured()
            || invalid_state
        {
            return;
        }

        self.connections
            .update(cx, |view, cx| view.set_running(false, cx));
        self.proxy
            .update(cx, |view, cx| view.set_running(false, cx));
        self.core_busy = Some(kind);
        self.sync_request_lock(cx);
        self.configuration
            .update(cx, |view, cx| view.set_locked(true, cx));
        self.subscriptions
            .update(cx, |view, cx| view.set_locked(true, cx));
        self.profiles
            .update(cx, |view, cx| view.set_locked(true, cx));
        self.structured
            .update(cx, |view, cx| view.set_locked(true, cx));
        self.rules.update(cx, |view, cx| view.set_locked(true, cx));
        self.scripts
            .update(cx, |view, cx| view.set_locked(true, cx));
        self.scripts
            .update(cx, |view, cx| view.set_running(false, cx));
        self.mitm.update(cx, |view, cx| {
            view.set_locked(true, cx);
            view.set_running(false, cx);
        });
        self.core_error = None;
        self.core_task = None;
        let service = self.core.clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    match kind {
                        CoreOperationKind::Start => service.start(),
                        CoreOperationKind::Stop => service.stop(),
                    }
                })
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |this, cx| {
                    let succeeded = result.is_ok();
                    match result {
                        Ok(()) => {
                            this.core_running = matches!(kind, CoreOperationKind::Start);
                        }
                        Err(error) => {
                            this.core_error = Some(error);
                        }
                    }
                    let children_running = this.core_running && !this.closing;
                    this.proxy
                        .update(cx, |view, cx| view.set_running(children_running, cx));
                    this.connections
                        .update(cx, |view, cx| view.set_running(children_running, cx));
                    this.stats
                        .update(cx, |view, cx| view.set_running(children_running, cx));
                    this.logs.update(cx, |view, cx| view.refresh(cx));
                    this.scripts.update(cx, |view, cx| {
                        view.set_running(children_running, cx);
                        view.set_locked(this.core_running || this.closing, cx);
                    });
                    this.mitm.update(cx, |view, cx| {
                        view.set_running(children_running, cx);
                        view.set_locked(this.core_running || this.closing, cx);
                    });
                    this.core_busy = None;
                    this.configuration.update(cx, |view, cx| {
                        view.set_locked(this.core_running || this.closing, cx);
                        if succeeded && matches!(kind, CoreOperationKind::Stop) {
                            view.refresh(window, cx);
                        }
                    });
                    this.subscriptions.update(cx, |view, cx| {
                        view.set_locked(this.core_running || this.closing, cx)
                    });
                    this.profiles.update(cx, |view, cx| {
                        view.set_locked(this.core_running || this.closing, cx)
                    });
                    this.structured.update(cx, |view, cx| {
                        view.set_locked(this.core_running || this.closing, cx)
                    });
                    this.rules.update(cx, |view, cx| {
                        view.set_locked(this.core_running || this.closing, cx)
                    });
                    this.sync_request_lock(cx);
                    cx.notify();
                })
                .ok();
            });
        });
        self.core_task = Some(task);
        cx.notify();
    }

    fn core_status(&self) -> String {
        if self.closing {
            return "正在停止日志、连接跟踪与实验核心…".to_string();
        }
        if let Some(kind) = self.core_busy {
            return match kind {
                CoreOperationKind::Start => "正在生成、校验配置并启动实验核心…".to_string(),
                CoreOperationKind::Stop => "正在停止核心…".to_string(),
            };
        }
        if let Some(error) = &self.core_error {
            return format!("核心错误：{error}");
        }
        if self.core_running {
            return format!("核心运行中 · 127.0.0.1:{}", self.core.port());
        }
        if self.core.is_configured() {
            return format!("核心已停止 · 隔离端口 {}", self.core.port());
        }
        format!(
            "核心未配置 · {}",
            self.core.configuration_error().unwrap_or("不可用")
        )
    }

    fn sync_request_lock(&mut self, cx: &mut Context<Self>) {
        let locked = self.closing
            || self.core_running
            || self.core_busy.is_some()
            || self.configuration.read(cx).busy()
            || self.subscriptions.read(cx).busy()
            || self.profiles.read(cx).busy()
            || self.structured.read(cx).busy()
            || self.rules.read(cx).busy()
            || self.scripts.read(cx).busy()
            || self.mitm.read(cx).busy();
        self.requests
            .update(cx, |view, cx| view.set_locked(locked, cx));
        cx.notify();
    }

    fn set_page(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        self.page = page;
        self.connections.update(cx, |view, cx| {
            view.set_active(page == Page::Connections, cx)
        });
        self.stats
            .update(cx, |view, cx| view.set_active(page == Page::Stats, cx));
        self.logs
            .update(cx, |view, cx| view.set_active(page == Page::Logs, cx));
        self.scripts
            .update(cx, |view, cx| view.set_active(page == Page::Scripts, cx));
        self.mitm
            .update(cx, |view, cx| view.set_active(page == Page::Mitm, cx));
        cx.notify();
    }
}

pub fn run() {
    let core = Arc::new(DesktopRuntime::from_env());
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            appearance::initialize(&core, cx);
            config_view::init(cx);
            log_capture::install(core.logs().buffer().clone(), core.logs().disk().cloned())
                .expect("failed to initialize experimental log capture");
            tracing::info!("GPUI experimental desktop initialized");
            gpui_kit::open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some("Lanpath 实验版（GPUI）".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| DesktopView::new(core, window, cx)),
            )
            .expect("failed to open experimental desktop window");
        });
}
