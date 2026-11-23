//! 统一 TOML 存储初始化。
//!
//! 新数据根直接初始化 `app.toml`（`[client]` 新装默认：直连
//! `request_proxy_mode=none`、`mitm_enabled=false`、`data_dir` 本根；
//! `[core]` 核心选择由共享 `settings`/`core_management` 正常管理）与
//! `subscriptions.toml`。已有根原样保留，绝不重复改写。

use std::path::Path;

use lanpath_client::resource_storage;
use lanpath_client::{ClientConfig, RequestProxyMode, settings};
use lanpath_common::LanpathResult;

/// 数据根准入（归属/树/独占锁）完成后运行，确保统一 TOML 布局存在。
/// 任何失败都真实报错，不用空数据覆盖已有文件。
pub(crate) fn initialize(root: &Path) -> LanpathResult<()> {
    if settings::load_app_section::<serde_json::Value>(root, "client")?.is_none() {
        let mut config = ClientConfig::new(
            root.to_path_buf(),
            String::new(),
            String::new(),
            std::path::PathBuf::new(),
        );
        config.request_proxy_mode = RequestProxyMode::None;
        config.mitm_enabled = false;
        // 新数据根默认本地监听端口（mixed 17899 / Clash API 19090）；已有根保留已保存值。
        config.mixed_port = crate::core_runtime::DEFAULT_MIXED_PORT;
        config.clash_api_port = crate::core_runtime::DEFAULT_CLASH_API_PORT;
        settings::persist_client(
            &settings::app_settings_path(root),
            &serde_json::to_value(&config)?,
        )?;
    }
    if !root.join(resource_storage::SUBSCRIPTIONS_FILE).is_file() {
        resource_storage::persist_subscriptions(root, &[])?;
    }
    lanpath_client::singbox_overwrite::SingboxOverwriteStore::new(root.to_path_buf())
        .ensure_initialized()?;
    Ok(())
}

#[cfg(test)]
#[path = "storage_init_tests.rs"]
mod tests;
