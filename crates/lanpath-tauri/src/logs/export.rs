//! Log export commands: desktop merge export and Android plugin export.

#[cfg(not(target_os = "android"))]
use std::path::PathBuf;

use crate::state::AppState;

/// Tauri command: export logs and return export product path.
///
/// - Desktop: merges all rolling files into `logs/export-<timestamp>.log`;
/// - Android: delegates to Kotlin plugin for zip export to public Downloads.
#[tauri::command]
pub async fn export_logs(state: tauri::State<'_, AppState>) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        let _ = &state;
        let handle = crate::core_bridge::vpn_plugin_handle()
            .ok_or_else(|| "VPN 插件未初始化，请重启应用后重试".to_string())?;
        handle
            .run_mobile_plugin_async::<String>("exportLogs", ())
            .await
            .map_err(crate::core_bridge::plugin_error)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        write_export(&state.data_dir.join("logs")).map(|path| path.to_string_lossy().to_string())
    }
}

/// Tauri command: open log export directory.
#[tauri::command]
pub async fn open_export_dir(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let _ = &app;
        let _ = &state;
        let handle = crate::core_bridge::vpn_plugin_handle()
            .ok_or_else(|| "VPN 插件未初始化，请重启应用后重试".to_string())?;
        handle
            .run_mobile_plugin_async::<serde_json::Value>("openLogsDir", ())
            .await
            .map(|_| ())
            .map_err(crate::core_bridge::plugin_error)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_opener::OpenerExt;
        app.opener()
            .open_path(
                state.data_dir.join("logs").to_string_lossy().into_owned(),
                None::<&str>,
            )
            .map_err(|e| format!("打开日志目录失败：{e}"))
    }
}

/// List daily log files for desktop export (excludes export-* files).
#[cfg(test)]
#[cfg(not(target_os = "android"))]
fn list_daily_files(logs_dir: &std::path::Path) -> Result<Vec<PathBuf>, String> {
    lanpath_client::logs::DiskLogService::new(logs_dir)
        .export_files()
        .map_err(|error| error.to_string())
}

/// Merge all rolling files into `logs_dir/export-<timestamp>.log` (desktop only).
#[cfg(not(target_os = "android"))]
fn write_export(logs_dir: &std::path::Path) -> Result<PathBuf, String> {
    lanpath_client::logs::DiskLogService::new(logs_dir)
        .export_to(logs_dir)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_merges_in_time_order() {
        let tmp = std::env::temp_dir().join(format!("pp-log-test-{}", std::process::id()));
        let logs_dir = tmp.join("logs");
        std::fs::create_dir_all(&logs_dir).unwrap();
        std::fs::write(logs_dir.join("app.log.2026-08-02"), "day2\n").unwrap();
        std::fs::write(logs_dir.join("app.log.2026-08-01"), "day1\n").unwrap();
        std::fs::write(logs_dir.join("app.log.2026-07-31"), "day0\n").unwrap();
        std::fs::write(logs_dir.join("libbox.log"), "box\n").unwrap();
        std::fs::write(logs_dir.join("export-20260101-000000.log"), "old export\n").unwrap();

        let export_path = write_export(&logs_dir).unwrap();
        let merged = std::fs::read_to_string(&export_path).unwrap();
        assert_eq!(merged, "day0\nday1\nday2\nbox\n");
        assert!(
            export_path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("export-")
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn list_daily_files_collects_all_log_files() {
        let tmp = std::env::temp_dir().join(format!("pp-log-list-{}", std::process::id()));
        let logs_dir = tmp.join("logs");
        std::fs::create_dir_all(&logs_dir).unwrap();
        std::fs::write(logs_dir.join("app.log"), "current\n").unwrap();
        std::fs::write(logs_dir.join("app.log.2026-08-01"), "day1\n").unwrap();
        std::fs::write(logs_dir.join("libbox.log"), "box\n").unwrap();
        std::fs::write(logs_dir.join("export-20260101-000000.log"), "old\n").unwrap();
        std::fs::write(logs_dir.join("README.txt"), "ignore\n").unwrap();

        let files = list_daily_files(&logs_dir).unwrap();
        let names: Vec<String> = files
            .iter()
            .filter_map(|p| p.file_name())
            .filter_map(|n| n.to_str().map(String::from))
            .collect();
        assert_eq!(names, vec!["app.log", "app.log.2026-08-01", "libbox.log"]);
        std::fs::remove_dir_all(&tmp).ok();
    }
}
