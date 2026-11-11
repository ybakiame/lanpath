#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    if let Some(code) = lanpath_desktop_experimental::proxy_guardian::dispatch_from_env() {
        std::process::exit(code);
    }
    lanpath_desktop_experimental::run();
}
