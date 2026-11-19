//! Runs the Rust application directly on the machine's native backend.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
use nsh_platform_linux as platform;
#[cfg(target_os = "macos")]
use nsh_platform_macos as platform;
#[cfg(target_os = "windows")]
use nsh_platform_windows as platform;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn main() -> std::process::ExitCode {
    // 独立系统代理守护分派：必须在任何 NSH/GUI/存储/env_logger 初始化前
    // 完成；失败非 0 退出且不留新 GUI（非 Windows 明确不支持）。
    if let Some(code) = lanpath_desktop_nsh::proxy_guardian::dispatch_from_env() {
        return std::process::ExitCode::from(code as u8);
    }
    env_logger::init();
    let data_dir = match lanpath_desktop_nsh::prepare_native_data_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Lanpath 数据目录初始化失败：{error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    // `--ui-gallery`：仅开发验收的组件画廊（普通用户导航没有入口）。
    let gallery = std::env::args().any(|arg| arg == "--ui-gallery");
    let root = if gallery {
        lanpath_desktop_nsh::gallery_root()
    } else {
        lanpath_desktop_nsh::root()
    };
    platform::run_app(
        platform::AppSource::Rust(root),
        platform::AppOptions {
            title: "Lanpath NSH Desktop".to_owned(),
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            app_id: "com.ybakiame.lanpath.desktop.nsh".to_owned(),
            size: (1180, 800),
            cache: platform::CacheOptions::at(platform::CacheLocation::Dir(
                data_dir.join("cache/nsh"),
            )),
            data_dir: Some(data_dir),
            ..Default::default()
        },
    );
    std::process::ExitCode::SUCCESS
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!(
    "Run this binary on macOS, Windows or Linux; use the TSX entry with nsh run for iOS and Android."
);
