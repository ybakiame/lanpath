use nsh_base::{button, radio, radio_group, theme};
use nsh_core::prelude::*;
use nsh_core::storage::Storage;

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

/// Where the appearance page keeps the user's choice between launches.
const APPEARANCE_KEY: &str = "appearance";

#[native_module("lanpath-desktop-nsh/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
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

/// The appearance the user asks for, one of the page's three radios.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Light,
    Dark,
    System,
}

impl Mode {
    /// The label the radios and the summary show.
    fn label(self) -> &'static str {
        match self {
            Mode::Light => "浅色",
            Mode::Dark => "深色",
            Mode::System => "跟随系统",
        }
    }

    /// The value kept in storage.
    fn storage_value(self) -> &'static str {
        match self {
            Mode::Light => "light",
            Mode::Dark => "dark",
            Mode::System => "system",
        }
    }

    /// What NSH is asked for; the resolved colors follow the system while
    /// `System`.
    fn appearance(self) -> Appearance {
        match self {
            Mode::Light => Appearance::Light,
            Mode::Dark => Appearance::Dark,
            Mode::System => Appearance::System,
        }
    }

    /// The stored choice, `System` when none or not one of the three.
    fn load() -> Self {
        match Storage::get_item(APPEARANCE_KEY).as_deref() {
            Some("light") => Mode::Light,
            Some("dark") => Mode::Dark,
            _ => Mode::System,
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
        .child(when(
            move || failure_state.get().is_some(),
            move || startup_failure(failure_state),
        ))
        .child(when(
            move || main_state.get().is_none(),
            move || {
                let loaded = Mode::load();
                if loaded != Mode::System {
                    set_color_scheme(loaded.appearance());
                }
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
                            appearance_page,
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
        .w(px(208.0))
        .flex_none()
        .flex_col()
        .p_4()
        .gap_2()
        .bg(move || theme().card)
        .child(text("Lanpath NSH Desktop").text_size(px(17.0)))
        .child(
            text("纯 Rust · NSH 0.5.0 · 订阅新增/刷新与核心查询/下载按需联网；核心校验并启动仅限用户态 loopback，系统代理经独立守护精确接管（仅 Windows），TUN/VPN 不接管")
                .text_size(px(12.0))
                .text_color(move || theme().muted_foreground),
        )
        .child(nav_item(page, Page::Subscription, "订阅"))
        .child(nav_item(page, Page::RequestSettings, "请求设置"))
        .child(nav_item(page, Page::Profiles, "覆写"))
        .child(nav_item(page, Page::Core, "核心"))
        .child(nav_item(page, Page::Config, "配置"))
        .child(nav_item(page, Page::Logs, "日志"))
        .child(nav_item(page, Page::Backup, "备份"))
        .child(nav_item(page, Page::Appearance, "外观"))
}

fn nav_item(page: Signal<Page>, target: Page, label: &'static str) -> impl IntoElement {
    let selected = move || page.get() == target;
    button()
        .px_3()
        .py_2()
        .rounded(move || theme().radius)
        .items_center()
        .bg(move || {
            if selected() {
                theme().primary
            } else {
                theme().card
            }
        })
        .on_press(move || page.set(target))
        .child(text(label).text_color(move || {
            if selected() {
                theme().primary_foreground
            } else {
                theme().foreground
            }
        }))
}

fn appearance_page() -> impl IntoElement {
    let mode = signal(Mode::load());
    view()
        .flex_1()
        .flex_col()
        .gap_4()
        .child(text("外观").text_size(px(24.0)))
        .child(
            text("窗口浅色 / 深色 · 选择立即生效并保存")
                .text_color(move || theme().muted_foreground),
        )
        .child(
            radio_group()
                .flex_col()
                .gap_2()
                .child(mode_radio(mode, Mode::Light))
                .child(mode_radio(mode, Mode::Dark))
                .child(mode_radio(mode, Mode::System)),
        )
        .child(text(move || format!("已选择：{}", mode.get().label())))
        .child(
            text(move || format!("当前生效：{}", if dark() { "深色" } else { "浅色" }))
                .text_color(move || theme().muted_foreground),
        )
}

fn mode_radio(mode: Signal<Mode>, target: Mode) -> impl IntoElement {
    let checked = move || mode.get() == target;
    radio()
        .checked(checked)
        .on_change(move |_: &bool| {
            mode.set(target);
            set_color_scheme(target.appearance());
            let _ = Storage::set_item(APPEARANCE_KEY, target.storage_value());
        })
        .child(
            view()
                .test_id(match target {
                    Mode::Light => "appearance-light",
                    Mode::Dark => "appearance-dark",
                    Mode::System => "appearance-system",
                })
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    view()
                        .size(px(16.0))
                        .rounded_full()
                        .border(px(1.0))
                        .border_color(move || theme().muted_foreground)
                        .bg(move || {
                            if checked() {
                                theme().primary
                            } else {
                                theme().card
                            }
                        }),
                )
                .child(text(target.label())),
        )
}
