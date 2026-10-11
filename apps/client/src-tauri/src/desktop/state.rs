//! 桌面数据布局：安装版使用产品目录，便携版使用可执行文件旁的 data。

use lanpath_common::{LanpathError, LanpathResult};
pub use lanpath_tauri::state::AppState;
use std::path::{Path, PathBuf};
pub const PORTABLE_MARKER: &str = "lanpath.portable";

#[derive(Debug, Clone)]
pub struct DesktopStorage {
    pub data_dir: PathBuf,
    pub portable: bool,
}

impl DesktopStorage {
    pub fn resolve() -> LanpathResult<Self> {
        Self::from_paths(&std::env::current_exe()?, dirs::home_dir().as_deref())
    }

    fn from_paths(executable: &Path, home: Option<&Path>) -> LanpathResult<Self> {
        let directory = executable
            .parent()
            .ok_or_else(|| LanpathError::Config("无法确定 Lanpath 程序所在目录".into()))?;
        // 快捷方式或终端的工作目录不参与便携数据目录解析。
        let marker = directory.join(PORTABLE_MARKER);
        let portable = marker.try_exists()?;
        if portable {
            if !marker.is_file() {
                return Err(LanpathError::Config("lanpath.portable 必须是文件".into()));
            }
            if cfg!(target_os = "macos") {
                return Err(LanpathError::Config(
                    "macOS 暂不支持 Portable WebView 数据布局".into(),
                ));
            }
            return Ok(Self {
                data_dir: directory.join("data"),
                portable,
            });
        }
        let home = home.ok_or_else(|| LanpathError::Config("无法解析用户主目录".into()))?;
        let current = home.join(".lanpath");
        let legacy = home.join(".proxy-panel-client");
        let data_dir = if !current.try_exists()? && legacy.try_exists()? {
            legacy
        } else {
            current
        };
        Ok(Self { data_dir, portable })
    }

    pub fn webview_dir(&self) -> PathBuf {
        self.data_dir.join("webview")
    }
}

#[tauri::command]
pub fn is_portable(storage: tauri::State<'_, DesktopStorage>) -> bool {
    storage.portable
}

#[cfg(test)]
mod tests;
