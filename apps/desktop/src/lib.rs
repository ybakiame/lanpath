use nsh_base::theme;
use nsh_core::element::Root;
use nsh_core::prelude::*;

mod backup;
mod backup_view;
mod config_composer;
mod config_validator;
mod config_view;
#[cfg(test)]
mod core_fixture;
mod core_inventory;
mod core_runtime;
mod core_view;
mod data_root;
mod disk_log_writer;
mod disk_logs_view;
mod i18n;
mod log_buffer;
mod log_capture;
mod log_service;
mod logs_view;
mod memory_logs_view;
mod profiles;
mod profiles_view;
mod request_settings;
mod request_settings_view;
mod storage_init;
mod subscriptions;
mod subscriptions_view;
mod system_proxy;
pub mod ui;
#[cfg(windows)]
mod windows_access;
#[cfg(windows)]
mod windows_file;

/// 独立系统代理守护的入口分派（`--proxy-guardian`）；必须是 crate 公开
/// 接口以便二进制在任何 NSH/GUI/存储初始化前调用。
pub mod proxy_guardian;

use logs_view::logs_page;
use profiles_view::profiles_page;
use request_settings_view::request_settings_page;
use subscriptions_view::subscription_page;

use backup_view::backup_page;
use config_view::config_page;
use core_view::core_page;

/// 应用根：注入编译期嵌入的翻译与默认主题 token；持久化的主题/模式/
/// 语言由 App 启动时经 `prefs().install()` 应用到运行时（`set_tokens`/
/// `set_color_scheme`/`set_locale`），无需重建组件树。
pub fn root() -> Root {
    let translations = i18n::translations().expect("embedded locales must parse");
    Root::of::<App>()
        .tokens(ui::theme::theme_tokens(ui::theme::ThemeId::Amethyst))
        .translations(translations)
}

/// 组件画廊根：仅 `--ui-gallery` 启动；普通用户导航没有入口。
pub fn gallery_root() -> Root {
    let translations = i18n::translations().expect("embedded locales must parse");
    Root::of::<ui::gallery::Gallery>()
        .tokens(ui::theme::theme_tokens(ui::theme::ThemeId::Amethyst))
        .translations(translations)
}

/// The sidebar's pages: only what this slice implements is listed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Subscription,
    RequestSettings,
    Profiles,
    Core,
    Config,
    Logs,
    Backup,
    Appearance,
}

impl Page {
    /// 导航项的 i18n key（意图稳定，不随语言变化）。
    fn label_key(self) -> &'static str {
        match self {
            Page::Subscription => "nav.subscriptions",
            Page::RequestSettings => "nav.request_settings",
            Page::Profiles => "nav.profiles",
            Page::Core => "nav.core",
            Page::Config => "nav.config",
            Page::Logs => "nav.logs",
            Page::Backup => "nav.backup",
            Page::Appearance => "nav.appearance",
        }
    }

    fn test_id(self) -> &'static str {
        match self {
            Page::Subscription => "nav-subscriptions",
            Page::RequestSettings => "nav-request-settings",
            Page::Profiles => "nav-profiles",
            Page::Core => "nav-core",
            Page::Config => "nav-config",
            Page::Logs => "nav-logs",
            Page::Backup => "nav-backup",
            Page::Appearance => "nav-appearance",
        }
    }
}

/// The desktop shell: a sidebar with the app's pages and their content.
/// Data stays in this app's own identity; nothing of Tauri/GPUI/`apps/nsh`
/// is read or migrated. The core page starts/stops the selected inventory
/// core in user mode only (loopback mixed listener + loopback Clash API,
/// after a real binary config check); Windows system proxy is taken over
/// and precisely restored through an independent ordinary-user guardian
/// (never by writing the registry from the GUI); TUN/VPN stays refused.
/// Network access is limited to subscription refreshes and core release
/// metadata/downloads using the saved request policy.
#[component]
pub fn App() -> impl IntoElement {
    // 启动事务门：待恢复备份已在数据根独占锁内由首个打开根的服务原子
    // 应用（DataRoot::open）。日志服务打开失败（目录被占用/准入拒绝/
    // 恢复日志含糊）时不进入任何业务页——半恢复状态绝不暴露，真实错误
    // 全窗上屏。主界面在 `when` 的惰性分支里构建，错误时不会创建。
    let (startup_error, log_service) = match log_service::LogService::open() {
        Ok(service) => (None, Some(service)),
        Err(error) => (Some(format!("{error}")), None),
    };
    let _log_service = stored(log_service);
    // 核心运行时按数据根共享（Weak 缓存）；App 级 Stored 持有到退出——
    // 正常退出 drop 时按记录的 OS 进程身份回收本次核心子进程。启动
    // 失败时不创建，避免在不可用根上重复报错。
    let _core_runtime = stored(if startup_error.is_none() {
        crate::core_runtime::CoreRuntime::open().ok()
    } else {
        None
    });
    let startup_error = signal(startup_error);
    let failure_state = startup_error;
    let main_state = startup_error;
    let page = signal(Page::Subscription);
    view()
        .flex_1()
        .flex_row()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(ui::feedback::toast_host())
        .child(when(
            move || failure_state.get().is_some(),
            move || startup_failure(failure_state),
        ))
        .child(when(
            move || main_state.get().is_none(),
            move || {
                // 外观安装：把持久化的主题/模式/语言应用到运行时。
                // 只改 token/外观/locale，组件树与业务运行状态不重建。
                ui::prefs::prefs().install();
                view().flex_1().flex_row().child(sidebar(page)).child(
                    view()
                        .flex_1()
                        .min_w(px(0.0))
                        .p_6()
                        .child(when(
                            move || page.get() == Page::Subscription,
                            subscription_page,
                        ))
                        .child(when(
                            move || page.get() == Page::RequestSettings,
                            request_settings_page,
                        ))
                        .child(when(move || page.get() == Page::Profiles, profiles_page))
                        .child(when(move || page.get() == Page::Core, core_page))
                        .child(when(move || page.get() == Page::Config, config_page))
                        .child(when(move || page.get() == Page::Logs, logs_page))
                        .child(when(move || page.get() == Page::Backup, backup_page))
                        .child(when(
                            move || page.get() == Page::Appearance,
                            ui::appearance_page::appearance_page,
                        )),
                )
            },
        ))
}

/// 启动失败全窗提示：数据根打不开（被占用/准入拒绝/恢复事务冲突）时
/// 不进入任何业务页，真实错误与处置建议直接上屏；原配置未被修改。
fn startup_failure(error: Signal<Option<String>>) -> impl IntoElement {
    view()
        .flex_1()
        .flex_col()
        .gap_3()
        .p_6()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(text("Lanpath NSH Desktop").text_size(px(24.0)))
        .child(
            text("启动失败：数据根无法打开（可能被其他实例占用、准入拒绝，或恢复事务存在冲突）")
                .text_color(move || theme().destructive),
        )
        .child(text(move || error.get().unwrap_or_default()))
        .child(
            text("原配置未被修改。请关闭其他实例或按错误提示处理，然后重新启动应用。")
                .text_color(move || theme().muted_foreground),
        )
}

fn sidebar(page: Signal<Page>) -> impl IntoElement {
    view()
        .w(px(216.0))
        .flex_none()
        .flex_col()
        .p_4()
        .gap_2()
        .bg(move || theme().sidebar)
        .border_r(px(1.0))
        .border_color(move || theme().sidebar_border)
        .text_color(move || theme().sidebar_foreground)
        .child(
            text(|| t!("app.title"))
                .text_size(px(17.0))
                .font_weight(FontWeight::MEDIUM),
        )
        .child(
            text(|| t!("app.tagline"))
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(ui::layout::separator())
        .child(nav_item(page, Page::Subscription))
        .child(nav_item(page, Page::RequestSettings))
        .child(nav_item(page, Page::Profiles))
        .child(nav_item(page, Page::Core))
        .child(nav_item(page, Page::Config))
        .child(nav_item(page, Page::Logs))
        .child(nav_item(page, Page::Backup))
        .child(nav_item(page, Page::Appearance))
}

fn nav_item(page: Signal<Page>, target: Page) -> impl IntoElement {
    let selected = move || page.get() == target;
    let label_key = target.label_key();
    let el = nsh_base::button()
        .selected(selected)
        .px(px(12.0))
        .py(px(8.0))
        .rounded(move || theme().radius)
        .items_center()
        .cursor_pointer()
        .transition_colors()
        .bg(move || {
            if selected() {
                theme().sidebar_accent
            } else {
                transparent_black().into()
            }
        })
        .text_color(move || {
            if selected() {
                theme().sidebar_accent_foreground
            } else {
                theme().sidebar_foreground
            }
        })
        .on_press(move || page.set(target))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .child(when(selected, || {
                    view()
                        .size(px(6.0))
                        .rounded_full()
                        .bg(move || theme().sidebar_primary)
                }))
                .child(text(move || t!(label_key)).text_size(px(14.0))),
        )
        .into_element();
    view().test_id(target.test_id()).child(el).into_element()
}
