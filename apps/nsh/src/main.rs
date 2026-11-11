//! Runs the Rust application directly on the machine's native backend.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
use nsh_platform_linux as platform;
#[cfg(target_os = "macos")]
use nsh_platform_macos as platform;
#[cfg(target_os = "windows")]
use nsh_platform_windows as platform;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn main() -> impl std::process::Termination {
    env_logger::init();
    let tokens = nsh_base::Theme::from_json(include_str!("../theme.json"))
        .expect("embedded NSH theme must be valid")
        .into_tokens();
    let root = platform::Root::of::<nsh_app_lanpath_nsh::App>().tokens(tokens);
    platform::run_app(
        platform::AppSource::Rust(root),
        platform::AppOptions {
            title: "Lanpath NSH".to_owned(),
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            app_id: "com.ybakiame.lanpath.nsh".to_owned(),
            size: (480, 640),
            ..Default::default()
        },
    )
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!(
    "Run this binary on macOS, Windows or Linux; use the TSX entry with nsh run for iOS and Android."
);
