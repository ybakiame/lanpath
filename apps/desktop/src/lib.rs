mod business;
mod config_view;
mod configuration;
mod connection_list;
mod connections_view;
mod data_root;
mod log_buffer;
mod log_capture;
mod log_list;
mod log_stream;
mod logs_view;
mod observation;
mod profiles_view;
mod proxy_list;
mod proxy_service;
mod proxy_view;
mod runtime;
mod shutdown_view;
mod stats_list;
mod stats_view;
mod subscriptions_view;
#[cfg(windows)]
mod windows_file;

use std::sync::Arc;

use config_view::{ConfigEvent, ConfigView};
use connections_view::ConnectionsView;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, WindowExt as _, button::Button,
};
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Role, StatefulInteractiveElement as _, Styled as _, Subscription, Task,
    TitlebarOptions, Window, WindowOptions, div,
};
use logs_view::LogsView;
use profiles_view::{ProfilesChanged, ProfilesView};
use proxy_view::{ProxyConfigurationChanged, ProxyView};
use runtime::DesktopRuntime;
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
    Configuration,
    Proxies,
    Connections,
    Stats,
    Logs,
}

struct DesktopView {
    page: Page,
    configuration: Entity<ConfigView>,
    subscriptions: Entity<SubscriptionsView>,
    profiles: Entity<ProfilesView>,
    core: Arc<DesktopRuntime>,
    proxy: Entity<ProxyView>,
    connections: Entity<ConnectionsView>,
    stats: Entity<StatsView>,
    logs: Entity<LogsView>,
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
        let profiles = cx.new(|cx| ProfilesView::new(core.clone(), window, cx));
        let _business_subscriptions = vec![
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
        let view = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            view.update(cx, |view, cx| view.request_close(window, cx))
                .is_err()
        });

        Self {
            page: Page::Subscriptions,
            configuration,
            subscriptions,
            profiles,
            core,
            proxy,
            connections,
            stats,
            logs,
            closing: false,
            close_task: None,
            core_running: false,
            core_busy: None,
            core_task: None,
            core_error: None,
            _proxy_configuration_subscription,
            _configuration_subscription,
            _business_subscriptions,
        }
    }

    fn run_core_operation(
        &mut self,
        kind: CoreOperationKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let invalid_state = match kind {
            CoreOperationKind::Start => {
                self.core_running || !self.configuration.read(cx).ready_for_start()
            }
            CoreOperationKind::Stop => !self.core_running,
        };
        if self.core_busy.is_some()
            || self.closing
            || self.proxy.read(cx).busy()
            || self.configuration.read(cx).busy()
            || self.subscriptions.read(cx).busy()
            || self.profiles.read(cx).busy()
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
        self.configuration
            .update(cx, |view, cx| view.set_locked(true, cx));
        self.subscriptions
            .update(cx, |view, cx| view.set_locked(true, cx));
        self.profiles
            .update(cx, |view, cx| view.set_locked(true, cx));
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

    fn set_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        self.connections.update(cx, |view, cx| {
            view.set_active(page == Page::Connections, cx)
        });
        self.stats
            .update(cx, |view, cx| view.set_active(page == Page::Stats, cx));
        self.logs
            .update(cx, |view, cx| view.set_active(page == Page::Logs, cx));
        cx.notify();
    }
}

impl Render for DesktopView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let operation_busy = self.closing
            || self.core_busy.is_some()
            || self.proxy.read(cx).busy()
            || self.configuration.read(cx).busy()
            || self.subscriptions.read(cx).busy()
            || self.profiles.read(cx).busy();
        let core_status: gpui_kit::SharedString = self.core_status().into();
        let content = match self.page {
            Page::Subscriptions => self.subscriptions.clone().into_any_element(),
            Page::Profiles => self.profiles.clone().into_any_element(),
            Page::Configuration => self.configuration.clone().into_any_element(),
            Page::Proxies => self.proxy.clone().into_any_element(),
            Page::Connections => self.connections.clone().into_any_element(),
            Page::Stats => self.stats.clone().into_any_element(),
            Page::Logs => self.logs.clone().into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .p_4()
            .gap_3()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child("Lanpath 实验版（GPUI）")
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Windows 实验性并行版本；独立数据，不接管系统代理/TUN"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("page-subscriptions")
                            .label("订阅")
                            .selected(self.page == Page::Subscriptions)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(Page::Subscriptions, cx);
                            })),
                    )
                    .child(
                        Button::new("page-profiles")
                            .label("覆写")
                            .selected(self.page == Page::Profiles)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(Page::Profiles, cx);
                            })),
                    )
                    .child(
                        Button::new("page-configuration")
                            .label("预览")
                            .selected(self.page == Page::Configuration)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(Page::Configuration, cx);
                            })),
                    )
                    .child(
                        Button::new("page-proxies")
                            .label("代理策略")
                            .selected(self.page == Page::Proxies)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(Page::Proxies, cx);
                            })),
                    )
                    .child(
                        Button::new("page-connections")
                            .label("连接")
                            .selected(self.page == Page::Connections)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(Page::Connections, cx);
                            })),
                    )
                    .child(
                        Button::new("page-stats")
                            .label("统计")
                            .selected(self.page == Page::Stats)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(Page::Stats, cx);
                            })),
                    )
                    .child(
                        Button::new("page-logs")
                            .label("日志")
                            .selected(self.page == Page::Logs)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_page(Page::Logs, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("core-start")
                            .label("启动实验核心")
                            .disabled(
                                operation_busy
                                    || self.core_running
                                    || !self.core.is_configured()
                                    || !self.configuration.read(cx).ready_for_start(),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run_core_operation(CoreOperationKind::Start, window, cx);
                            })),
                    )
                    .child(
                        Button::new("core-stop")
                            .label("停止核心")
                            .disabled(operation_busy || !self.core_running)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.run_core_operation(CoreOperationKind::Stop, window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .id("core-status")
                    .role(Role::Status)
                    .aria_label(core_status.clone())
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(core_status),
            )
            .child(content)
    }
}

pub fn run() {
    let core = Arc::new(DesktopRuntime::from_env());
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            config_view::init(cx);
            log_capture::install(core.logs().buffer().clone())
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
