//! 岚径 Lanpath 客户端壳（Tauri 2，单壳双目标：Linux/Windows/macOS + Android）。
//!
//! 合并架构（docs/plans/2026-10-03-client-merge-evaluation.md，M1）：双端通用命令
//! 单份实现于共享 crate [`lanpath_tauri`]（35+ 条 commands + capabilities + logs），
//! 本壳只做平台组装，平台差异为编译期事实：
//!
//! - [`desktop`] 适配层（`#[cfg(not(any(android, ios)))]`）：桌面专属命令
//!   （mitm / core_mgmt / remote / platform）、数据目录与 WSL workaround；
//! - [`mobile`] 适配层（`#[cfg(target_os = "android")]`）：Android 应用私有数据
//!   目录、`core_bridge` 数据目录注入与 VPN 插件注册；
//! - 依赖差异由 `Cargo.toml` target 依赖表表达：Android 构建图不含 `lanpath-mitm`
//!   （`lanpath-tauri/mitm` feature 仅桌面目标激活，M0-① 实证）。
//!
//! 同时作为 lib 与 bin 构建：Android/iOS 端由 [`tauri::mobile_entry_point`] 注入
//! 移动入口（`main` 不参与编译）；桌面端由 `main` 调用 [`run()`]。

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod desktop;
#[cfg(target_os = "android")]
mod mobile;

use tauri::Manager;

/// 应用入口。
///
/// Android/iOS 构建时经 `#[cfg_attr(mobile, tauri::mobile_entry_point)]` 生成
/// 移动端 JNI/入口；桌面构建时由 `main` 调用本函数。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 必须在任何 WebKit 相关初始化之前执行（桌面 Linux 专属；target_os = "linux"
    // 不含 Android，无需额外排除）。
    #[cfg(all(
        target_os = "linux",
        not(any(target_os = "android", target_os = "ios"))
    ))]
    desktop::configure_wsl_webkit_workaround();

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let storage = desktop::DesktopStorage::resolve().expect("无法解析 Lanpath 数据目录");

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init());

    // 自动更新（ADR-0008 D2）：仅桌面目标注册（Android 不经 updater 分发，
    // 对应 capability 亦按 platforms 门控于桌面）。
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = {
        let builder = builder.plugin(tauri_plugin_process::init());
        let builder = if storage.portable {
            builder
        } else {
            builder.plugin(tauri_plugin_updater::Builder::new().build())
        };
        builder.manage(storage)
    };

    let builder = builder
        .setup(|app| {
            // 桌面目录在创建 WebView 前确定；Android 保持应用私有目录。
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            let data_dir = app.state::<desktop::DesktopStorage>().data_dir.clone();
            #[cfg(target_os = "android")]
            let data_dir = mobile::resolve_data_dir(app);
            // 便携目录不可写时明确终止，不悄悄使用安装版目录。
            std::fs::create_dir_all(&data_dir)?;
            lanpath_tauri::prepare_startup_backup(&data_dir)?;
            // tracing 全局 subscriber 只能初始化一次；日志初始化紧随 data_dir
            // 解析，guard 存入 AppState 持有，保证进程生命周期内文件写入线程存活。
            let log_guard = lanpath_tauri::logs::init_logging(&data_dir);
            tracing::info!("岚径 Lanpath 客户端数据目录：{}", data_dir.display());
            app.manage(lanpath_tauri::state::AppState::new(
                data_dir.clone(),
                log_guard,
            ));
            // 安装包内置种子核心首启释放（ADR-0008 D5）：仅桌面目标；无种子资源
            // （dev / Linux / macOS 构建）或已装核心时静默跳过，失败仅告警不阻塞启动。
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            if let Ok(resource_dir) = app.path().resource_dir() {
                match lanpath_client::cores::seed_bundled_core(&data_dir, &resource_dir) {
                    Ok(Some(core)) => {
                        tracing::info!(version = %core.version, "种子核心已释放并就绪");
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "种子核心释放失败，可在核心管理中手动下载");
                    }
                }
            }
            // 向 Android 核心桥注入数据目录（VPN 通知偏好/订阅名解析回源
            // client.json / subscriptions.json）。
            #[cfg(target_os = "android")]
            lanpath_tauri::core_bridge::set_data_dir(data_dir.clone());
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                let webview_dir = app.state::<desktop::DesktopStorage>().webview_dir();
                std::fs::create_dir_all(&webview_dir)?;
                for config in &app.config().app.windows {
                    let window = tauri::WebviewWindowBuilder::from_config(app, config)?
                        .data_directory(webview_dir.clone())
                        .min_inner_size(760.0, 560.0)
                        .center();
                    // 逻辑像素随 DPI 换算；保留窗口边框空间，小屏不能被默认尺寸撑出工作区。
                    let window = if let Ok(Some(monitor)) = app.primary_monitor() {
                        let area = monitor
                            .work_area()
                            .size
                            .to_logical::<f64>(monitor.scale_factor());
                        let width = (area.width - 32.0).max(1.0);
                        let height = (area.height - 48.0).max(1.0);
                        window
                            .inner_size(config.width.min(width), config.height.min(height))
                            .min_inner_size(760.0_f64.min(width), 560.0_f64.min(height))
                    } else {
                        window
                    };
                    #[cfg(target_os = "windows")]
                    let window = window.decorations(false);
                    window.build()?;
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::state::is_portable,
            // ---- 双端通用命令（共享层单份实现，全路径注册）----
            lanpath_tauri::capabilities::get_capabilities,
            lanpath_tauri::capabilities::platform_info,
            lanpath_tauri::logs::get_logs,
            lanpath_tauri::logs::export_logs,
            lanpath_tauri::logs::open_export_dir,
            lanpath_tauri::logs::list_log_files,
            lanpath_tauri::logs::read_log_file_tail,
            lanpath_tauri::logs::clear_logs,
            lanpath_tauri::logs::log_frontend,
            lanpath_tauri::commands::backup_generate_password,
            lanpath_tauri::commands::backup_export,
            lanpath_tauri::commands::backup_preview,
            lanpath_tauri::commands::backup_prepare_restore,
            lanpath_tauri::commands::backup_restored,
            lanpath_tauri::commands::backup_acknowledge,
            lanpath_tauri::commands::backup_restart,
            lanpath_tauri::commands::backup_pending,
            lanpath_tauri::commands::backup_previous,
            lanpath_tauri::commands::get_config,
            lanpath_tauri::commands::save_config,
            lanpath_tauri::commands::start_proxy,
            lanpath_tauri::commands::stop_proxy,
            lanpath_tauri::commands::proxy_status,
            lanpath_tauri::commands::proxy_startup_progress,
            lanpath_tauri::commands::set_rule_mode,
            lanpath_tauri::commands::list_profiles,
            lanpath_tauri::commands::create_profile,
            lanpath_tauri::commands::get_profile,
            lanpath_tauri::commands::update_profile,
            lanpath_tauri::commands::delete_profile,
            lanpath_tauri::commands::preview_core_config,
            lanpath_tauri::commands::list_subscriptions,
            lanpath_tauri::commands::add_subscription,
            lanpath_tauri::commands::update_subscription,
            lanpath_tauri::commands::remove_subscription,
            lanpath_tauri::commands::set_subscription_enabled,
            lanpath_tauri::commands::set_active_subscription,
            lanpath_tauri::commands::refresh_subscription,
            lanpath_tauri::commands::subscription_node_tags,
            lanpath_tauri::commands::subscription_source,
            lanpath_tauri::commands::export_subscription_source,
            lanpath_tauri::commands::set_subscription_profile,
            lanpath_tauri::commands::proxies_list,
            lanpath_tauri::commands::proxies_select,
            lanpath_tauri::commands::proxies_test_delay,
            lanpath_tauri::commands::proxies_test_group,
            lanpath_tauri::commands::connections_active,
            lanpath_tauri::commands::connections_closed,
            lanpath_tauri::commands::connections_close,
            lanpath_tauri::commands::stats_today,
            lanpath_tauri::commands::stats_daily,
            lanpath_tauri::commands::stats_records,
            lanpath_tauri::commands::stats_clear,
            lanpath_tauri::commands::list_tasks,
            lanpath_tauri::commands::run_task,
            lanpath_tauri::commands::local_override_get,
            lanpath_tauri::commands::local_override_save,
            lanpath_tauri::commands::local_override_update_rulesets_now,
            lanpath_tauri::commands::local_override_update_rule_set,
            lanpath_tauri::commands::config_slices_get,
            lanpath_tauri::commands::config_slices_save,
            lanpath_tauri::commands::baseline_view_get,
            lanpath_tauri::commands::builtin_dns_slice_get,
            lanpath_tauri::commands::diagnose_connectivity_run,
            lanpath_tauri::commands::dns_server_probe,
            // ---- 桌面专属命令（desktop 适配层；移动端编译期裁剪）----
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::tun_auth_status,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::authorize_tun,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::list_traffic,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::get_mitm_ca,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::install_mitm_ca,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::mitm_ca_trust_status,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::get_mitm_whitelist,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::list_remotes,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::add_remote,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::update_remote,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::get_remote_icon,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::detect_remote,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::remove_remote,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::fetch_remotes,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::test_github_proxy,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::import_config,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::list_cores,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::list_remote_core_channels,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::list_downloaded_versions,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::download_core,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::set_active_core,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::delete_core,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::gpu_acceleration,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            desktop::commands::toast_mode_override,
            // ---- Android 专属命令（共享层 core_bridge；桌面编译期裁剪）----
            #[cfg(target_os = "android")]
            lanpath_tauri::core_bridge::request_vpn_permission,
            #[cfg(target_os = "android")]
            lanpath_tauri::core_bridge::vpn_last_error,
            #[cfg(target_os = "android")]
            lanpath_tauri::core_bridge::core_version,
            #[cfg(target_os = "android")]
            lanpath_tauri::core_bridge::notify_prefs_changed,
        ]);

    // Android 核心由 Kotlin 侧 libbox 驱动：注册 `vpn` 插件（VpnPlugin 真实桥，setup
    // 内注册 Kotlin 插件并安装核心引擎桥，见 lanpath-tauri::core_bridge::vpn_plugin）。
    #[cfg(target_os = "android")]
    let builder = builder.plugin(lanpath_tauri::core_bridge::vpn_plugin());

    let context = tauri::generate_context!();
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let context = {
        let mut context = context;
        for window in &mut context.config_mut().app.windows {
            window.create = false;
        }
        context
    };
    let app = builder
        .build(context)
        .expect("error while building tauri application");

    // ADR-0012 D3：退出清理（桌面）。正常退出路径（关窗/退出菜单）时
    // best-effort 停止核心并恢复系统代理；冻结被杀等异常路径收不到退出事件，
    // 由 lanpath-engine 的 OS 级父子绑定与 PID 收割兜底，本层不为此负责。
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    app.run(|app_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            desktop::cleanup_on_exit(app_handle);
        }
    });
    #[cfg(any(target_os = "android", target_os = "ios"))]
    app.run(|_, _| {});
}
