mod about_view;
mod appearance;
mod appearance_view;
mod backup_lifecycle;
mod backup_view;
mod business;
mod config_slices_view;
mod config_view;
mod configuration;
mod configuration_workspace;
mod connection_list;
mod connections_view;
mod core_lifecycle;
mod core_management_view;
mod data_root;
mod design;
mod diagnostics_view;
mod disk_log_writer;
mod distribution;
mod distribution_lifecycle;
mod distribution_view;
mod keyboard;
mod log_buffer;
mod log_capture;
mod log_list;
mod log_stream;
mod logs_view;
mod migration;
mod mitm_view;
mod observation;
mod privileged;
mod product;
mod profiles_view;
pub mod proxy_guardian;
mod proxy_list;
mod proxy_service;
mod proxy_view;
mod request_settings_view;
mod restart;
mod rules_view;
mod runtime;
mod script_dialects;
mod scripts_view;
mod shell_view;
mod shutdown_view;
mod startup;
mod startup_activation;
mod stats_list;
mod stats_view;
mod subscriptions_view;
mod system_settings_view;
mod view_locks;
#[cfg(windows)]
mod windows_access;
#[cfg(windows)]
mod windows_file;
#[cfg(windows)]
mod windows_gui;
mod workspace;

use about_view::AboutView;
use appearance_view::AppearanceView;
use backup_view::{BackupEvent, BackupView};
use std::{rc::Rc, sync::Arc};

use config_slices_view::{ConfigSlicesChanged, ConfigSlicesView};
use config_view::{ConfigEvent, ConfigView};
use connections_view::ConnectionsView;
use core_management_view::{CoreManagementView, CoreSelectionChanged};
use diagnostics_view::DiagnosticsView;
use distribution_view::{DistributionEvent, DistributionView};
use gpui_kit::component::{TitleBar, WindowExt as _};
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, Subscription, Task, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowOptions,
};
use logs_view::LogsView;
use mitm_view::{MitmChanged, MitmView};
use profiles_view::{ProfilesChanged, ProfilesView};
use proxy_view::{ProxyConfigurationChanged, ProxyView};
use request_settings_view::RequestSettingsView;
use rules_view::{RulesChanged, RulesView};
use runtime::DesktopRuntime;
use scripts_view::{AutomaticRefreshPoll, ScriptsView};
use stats_view::StatsView;
use subscriptions_view::{SubscriptionsChanged, SubscriptionsView};
use system_settings_view::{SystemSettingsChanged, SystemSettingsView};

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
    System,
    Appearance,
    Cores,
    Diagnostics,
    Backup,
    Distribution,
    About,
}

struct DesktopView {
    page: Page,
    configuration_section: configuration_workspace::StructuredSection,
    focus_handle: FocusHandle,
    content_focus: FocusHandle,
    navigation_focus: Rc<[FocusHandle; 19]>,
    navigation_expanded: [bool; 4],
    _focus_lost_subscription: Subscription,
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
    system: Entity<SystemSettingsView>,
    appearance: Entity<AppearanceView>,
    cores: Entity<CoreManagementView>,
    diagnostics: Entity<DiagnosticsView>,
    about: Entity<AboutView>,
    backups: Entity<BackupView>,
    distribution: Entity<DistributionView>,
    restore_task: Option<Task<()>>,
    startup_status: String,
    business_locked: bool,
    _startup_task: Task<()>,
    _native_task: Task<()>,
    closing: bool,
    close_task: Option<Task<()>>,
    core_running: bool,
    core_recovery_needed: bool,
    core_generation: u64,
    core_busy: Option<CoreOperationKind>,
    core_task: Option<Task<()>>,
    core_error: Option<String>,
    core_stop_kind: Option<privileged::StopKind>,
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
            cx.subscribe(&scripts, |this, _, _: &AutomaticRefreshPoll, cx| {
                // Evaluate current root state on every tick, not a potentially
                // stale eligibility snapshot from the last child notification.
                this.sync_request_lock(cx);
                this.scripts
                    .update(cx, |view, cx| view.poll_automatic_refresh(cx));
                this.sync_request_lock(cx);
            }),
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
        let system = cx.new(|cx| SystemSettingsView::new(core.clone(), window, cx));
        let appearance = cx.new(|cx| AppearanceView::new(window, cx));
        let cores = cx.new(|cx| CoreManagementView::new(core.clone(), window, cx));
        let diagnostics = cx.new(|cx| DiagnosticsView::new(core.clone(), window, cx));
        let about = cx.new(|_| AboutView);
        let backups = cx.new(|cx| BackupView::new(core.clone(), window, cx));
        let distribution = cx.new(|cx| DistributionView::new(core.clone(), window, cx));
        _business_subscriptions.push(cx.subscribe_in(
            &distribution,
            window,
            |this, _, event: &DistributionEvent, window, cx| match event {
                DistributionEvent::RestartRequested => this.restart_distribution(window, cx),
            },
        ));
        _business_subscriptions.push(cx.subscribe_in(
            &system,
            window,
            |this, _, _: &SystemSettingsChanged, window, cx| {
                this.configuration
                    .update(cx, |view, cx| view.refresh(window, cx));
                this.sync_request_lock(cx);
            },
        ));
        _business_subscriptions.push(cx.subscribe_in(
            &backups,
            window,
            |this, _, event: &BackupEvent, window, cx| match event {
                BackupEvent::RestoreRequested { preview_id } => {
                    this.request_restore(preview_id.clone(), window, cx);
                }
                BackupEvent::RestartRequested => this.restart_restored(window, cx),
                BackupEvent::StartupNotice => this.set_page(Page::Backup, cx),
            },
        ));
        _business_subscriptions.push(cx.subscribe_in(
            &cores,
            window,
            |this, _, _: &CoreSelectionChanged, window, cx| {
                this.core.refresh_selected_core();
                this.reconcile_configuration_page(cx);
                this.sync_request_lock(cx);
                this.configuration
                    .update(cx, |view, cx| view.refresh(window, cx));
                cx.notify();
            },
        ));
        let mut startup = core.startup();
        let startup_task = cx.spawn(async move |this, cx| {
            while startup.changed().await.is_ok() {
                let progress = startup.borrow_and_update().clone();
                if this
                    .update(cx, |this, cx| {
                        this.startup_status = progress.summary();
                        this.cores
                            .update(cx, |view, cx| view.set_startup(progress, cx));
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        _business_subscriptions.extend([
            cx.observe(&configuration, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&subscriptions, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&profiles, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&structured, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&rules, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&scripts, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&mitm, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&requests, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&system, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&distribution, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&stats, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&logs, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&cores, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&diagnostics, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&backups, |this, _, cx| this.sync_request_lock(cx)),
            cx.observe(&proxy, |this, _, cx| this.sync_request_lock(cx)),
        ]);
        let view = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            view.update(cx, |view, cx| view.request_close(window, cx))
                .is_err()
        });

        let native_task = Self::start_native_poll(core.clone(), window, cx);
        let focus_lost_subscription = cx.on_focus_lost(window, |this, window, cx| {
            let target = window
                .focus_lost_restore_target(cx)
                .unwrap_or_else(|| this.focus_handle.clone());
            target.focus(window, cx);
        });
        let mut view = Self {
            page: Page::Subscriptions,
            configuration_section: configuration_workspace::StructuredSection::Dns,
            focus_handle: cx.focus_handle(),
            content_focus: cx.focus_handle().tab_stop(true),
            navigation_focus: Rc::new(std::array::from_fn(|_| cx.focus_handle().tab_stop(true))),
            navigation_expanded: [true, true, false, false],
            _focus_lost_subscription: focus_lost_subscription,
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
            system,
            appearance,
            cores,
            diagnostics,
            about,
            backups,
            distribution,
            restore_task: None,
            startup_status: "正在开始启动".into(),
            business_locked: false,
            _startup_task: startup_task,
            _native_task: native_task,
            closing: false,
            close_task: None,
            core_running: false,
            core_recovery_needed: false,
            core_generation: 0,
            core_busy: None,
            core_task: None,
            core_error: None,
            core_stop_kind: None,
            _proxy_configuration_subscription,
            _configuration_subscription,
            _business_subscriptions,
        };
        view.sync_request_lock(cx);
        view.focus_navigation(view.page, window, cx);
        view
    }

    fn set_page(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        let page = self.available_page(page);
        self.page = page;
        self.navigation_expanded[page.group_index()] = true;
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
        if page == Page::Cores {
            self.cores.update(cx, |view, cx| view.refresh(cx));
        } else if page == Page::Backup {
            self.backups.update(cx, |view, cx| view.refresh(cx));
        }
        cx.notify();
    }
}

pub fn run() {
    if privileged::dispatch_from_env().expect("privileged helper dispatch failed") {
        return;
    }
    #[cfg(windows)]
    if let Err(error) = windows_gui::ensure_unelevated() {
        windows_gui::show_startup_error(&error.to_string());
        std::process::exit(1);
    }
    #[cfg(windows)]
    {
        if distribution::payload::dispatch_from_env() {
            return;
        }
        if let Err(error) = distribution::payload::admit_application() {
            windows_gui::show_startup_error(&error.to_string());
            std::process::exit(5);
        }
    }
    restart::wait_for_parent().expect("failed to receive experimental restore restart handoff");
    if startup_activation::activate_existing().expect("GPUI instance activation failed") {
        return;
    }
    let core = DesktopRuntime::from_env();
    gpui_kit::application()
        .with_assets(product::ProductAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            appearance::initialize(&core, cx);
            config_view::init(cx);
            keyboard::init(cx);
            log_capture::install(core.logs().buffer().clone(), core.logs().disk().cloned())
                .expect("failed to initialize experimental log capture");
            tracing::info!("GPUI experimental desktop initialized");
            let (window, _) = gpui_kit::open_window(
                WindowOptions {
                    // Native window geometry is in logical pixels, independent of
                    // the user's rem-based interface zoom.
                    window_bounds: Some(gpui_kit::WindowBounds::Windowed(
                        gpui_kit::Bounds::centered(
                            None,
                            gpui_kit::size(gpui_kit::px(1180.), gpui_kit::px(800.)),
                            cx,
                        ),
                    )),
                    window_min_size: Some(gpui_kit::size(gpui_kit::px(960.), gpui_kit::px(640.))),
                    // Windows otherwise exposes the HWND before GPUI installs its
                    // close callbacks. An immediate close can orphan the event loop.
                    show: !cfg!(windows),
                    titlebar: Some(TitlebarOptions {
                        title: Some(crate::product::window_title().into()),
                        ..TitleBar::title_bar_options()
                    }),
                    window_background: if cx
                        .global::<appearance::AppearanceState>()
                        .preferences()
                        .uses_acrylic(cx.window_appearance())
                    {
                        WindowBackgroundAppearance::Blurred
                    } else {
                        WindowBackgroundAppearance::Opaque
                    },
                    ..TitleBar::window_options()
                },
                cx,
                |window, cx| cx.new(|cx| migration::session::SessionView::new(core, window, cx)),
            )
            .expect("failed to open experimental desktop window");
            if cfg!(windows) {
                window
                    .update(cx, |_, window, _| window.activate_window())
                    .expect("failed to reveal initialized desktop window");
            }
        });
}
