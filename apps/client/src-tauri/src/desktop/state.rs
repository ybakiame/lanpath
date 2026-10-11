//! Tauri 应用级共享状态（薄层）。

use std::path::PathBuf;

pub use lanpath_tauri::state::AppState;

/// 默认数据目录：用户主目录下的 `.lanpath`；已有旧客户端数据时沿用旧目录。
///
/// 主目录经 [`dirs::home_dir`] 解析——Windows 上 `HOME` 环境变量通常不存在
/// （此前直接读 `HOME`，未设置时回退为相对路径 `.`，数据目录落到安装目录
/// 或启动目录下，且核心子进程因 `-D` 切换工作目录后读不到相对路径配置）。
/// 主目录不可用时回退当前目录；两者均不可用时返回诊断用相对路径。
pub fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .or_else(|| std::env::current_dir().ok())
        .map(|home| {
            let current = home.join(".lanpath");
            let legacy = home.join(".proxy-panel-client");
            if !current.exists() && legacy.exists() {
                legacy
            } else {
                current
            }
        })
        .unwrap_or_else(|| PathBuf::from(".lanpath"))
}
