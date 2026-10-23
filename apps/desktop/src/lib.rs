mod about_view;
mod appearance;
mod appearance_view;
mod backup_lifecycle;
mod backup_view;
mod business;
mod config_slices_view;
mod config_view;
mod configuration;
mod connection_list;
mod connections_view;
mod core_lifecycle;
mod core_management_view;
mod data_root;
mod diagnostics_view;
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
mod restart;
mod rules_view;
mod runtime;
mod scripts_view;
mod shell_view;
mod shutdown_view;
mod startup;
mod stats_list;
mod stats_view;
mod subscriptions_view;
#[cfg(windows)]
mod windows_file;

use about_view::AboutView;
use appearance_view::AppearanceView;
use backup_view::{BackupEvent, BackupView};
use std::sync::Arc;

use config_slices_view::{ConfigSlicesChanged, ConfigSlicesView};
use config_view::{ConfigEvent, ConfigView};
use connections_view::ConnectionsView;
use core_management_view::{CoreManagementView, CoreSelectionChanged};
use diagnostics_view::DiagnosticsView;
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
    Cores,
    Diagnostics,
    Backup,
    About,
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
    cores: Entity<CoreManagementView>,
    diagnostics: Entity<DiagnosticsView>,
    about: Entity<AboutView>,
    backups: Entity<BackupView>,
    restore_task: Option<Task<()>>,
    startup_status: String,
    business_locked: bool,
    _startup_task: Task<()>,
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
        let cores = cx.new(|cx| CoreManagementView::new(core.clone(), window, cx));
        let diagnostics = cx.new(|cx| DiagnosticsView::new(core.clone(), window, cx));
        let about = cx.new(|cx| AboutView::new(core.clone(), cx));
        let backups = cx.new(|cx| BackupView::new(core.clone(), window, cx));
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
                this.sync_request_lock(cx);
                this.about.update(cx, |view, cx| view.refresh(cx));
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
            cores,
            diagnostics,
            about,
            backups,
            restore_task: None,
            startup_status: "正在开始启动".into(),
            business_locked: false,
            _startup_task: startup_task,
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

    fn sync_request_lock(&mut self, cx: &mut Context<Self>) {
        let backup_work = self.backups.read(cx).busy() || self.backups.read(cx).is_pending();
        let lifecycle = self.closing || self.core_busy.is_some() || backup_work;
        let core_work = self.cores.read(cx).busy();
        let diagnosing = self.diagnostics.read(cx).busy();
        let config_work = self.configuration.read(cx).busy()
            || self.subscriptions.read(cx).busy()
            || self.profiles.read(cx).busy()
            || self.structured.read(cx).busy()
            || self.rules.read(cx).busy()
            || self.scripts.read(cx).busy()
            || self.mitm.read(cx).busy();
        let peer_work = config_work || self.requests.read(cx).busy() || self.proxy.read(cx).busy();
        self.backups.update(cx, |view, cx| {
            view.set_locked(
                self.closing || self.core_busy.is_some() || peer_work || core_work || diagnosing,
                cx,
            )
        });
        self.appearance.update(cx, |view, cx| {
            view.set_closing(self.closing || backup_work, cx)
        });
        self.requests.update(cx, |view, cx| {
            view.set_locked(
                lifecycle || self.core_running || config_work || core_work || diagnosing,
                cx,
            )
        });
        self.cores.update(cx, |view, cx| {
            view.set_locked(
                lifecycle || self.core_running || peer_work || diagnosing,
                cx,
            )
        });
        self.diagnostics.update(cx, |view, cx| {
            view.set_locked(lifecycle || peer_work || core_work, cx)
        });
        self.about.update(cx, |view, cx| {
            view.set_locked(lifecycle || peer_work || core_work || diagnosing, cx)
        });
        self.proxy.update(cx, |view, cx| {
            view.set_locked(lifecycle || core_work || diagnosing, cx)
        });
        // Older peers notify even on equal lock values. Publish a transition
        // once, not another notification loop through their observers.
        let locked = lifecycle || self.core_running || core_work || diagnosing;
        if self.business_locked != locked {
            self.business_locked = locked;
            self.configuration
                .update(cx, |view, cx| view.set_locked(locked, cx));
            self.subscriptions
                .update(cx, |view, cx| view.set_locked(locked, cx));
            self.profiles
                .update(cx, |view, cx| view.set_locked(locked, cx));
            self.structured
                .update(cx, |view, cx| view.set_locked(locked, cx));
            self.rules
                .update(cx, |view, cx| view.set_locked(locked, cx));
            self.scripts
                .update(cx, |view, cx| view.set_locked(locked, cx));
            self.mitm.update(cx, |view, cx| view.set_locked(locked, cx));
        }
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
        if page == Page::Cores {
            self.cores.update(cx, |view, cx| view.refresh(cx));
        } else if page == Page::About {
            self.about.update(cx, |view, cx| view.refresh(cx));
        } else if page == Page::Backup {
            self.backups.update(cx, |view, cx| view.refresh(cx));
        }
        cx.notify();
    }
}

pub fn run() {
    restart::wait_for_parent().expect("failed to receive experimental restore restart handoff");
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
