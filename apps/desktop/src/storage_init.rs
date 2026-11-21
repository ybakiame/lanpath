//! 统一 TOML 存储初始化（breaking change，仅支持新布局）。
//!
//! 本应用是无实际用户的本地迭代产品，配置一律使用统一 TOML 布局：新数据根
//! 直接初始化 `app.toml`（`[client]` 新装默认：直连 `request_proxy_mode=none`、
//! `mitm_enabled=false`、`data_dir` 本根；`[core]` 核心选择由共享
//! `settings`/`core_management` 正常管理）与 `subscriptions.toml`。已有统一
//! 根原样保留，绝不重复改写。
//!
//! 早期切片的 JSON 测试布局（`client.json` / `subscriptions.json` /
//! `subscription_cache/` / `profile_cache/`）不受支持：存在时明确报错，
//! 不迁移、不静默忽略或清空，也不读取其他应用的数据根——请使用新的数据根。

use std::path::Path;

use lanpath_client::resource_storage;
use lanpath_client::{ClientConfig, RequestProxyMode, settings};
use lanpath_common::{LanpathError, LanpathResult};

/// 旧 JSON 测试布局的入口（文件或目录）；存在即拒绝启动。
const LEGACY_LAYOUT: &[&str] = &[
    "client.json",
    "subscriptions.json",
    "subscription_cache",
    "profile_cache",
];

/// 数据根准入（归属/树/独占锁）完成后运行：拒绝旧布局，确保统一 TOML
/// 布局存在。任何失败都真实报错，不用空数据覆盖已有文件。
pub(crate) fn initialize(root: &Path) -> LanpathResult<()> {
    for name in LEGACY_LAYOUT {
        if root.join(name).exists() {
            return Err(LanpathError::Config(format!(
                "旧测试配置布局（{name}）不受支持，请使用新的数据根"
            )));
        }
    }
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
