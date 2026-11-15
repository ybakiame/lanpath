//! 请求设置业务：订阅刷新与远程资源请求的网络请求策略（代理模式 +
//! 自定义代理 URL + GitHub 镜像前缀），读写本应用数据根 `app.toml` 的
//! `[client]` 段（统一 TOML 布局，数据根初始化时创建）。
//! 从 apps/gpui 的 `business/request_settings.rs` 迁移并适配为 NSH 桌面
//! 数据根（`load_for_requests` 语义）：不读取/迁移其他应用数据，不启动
//! 核心、系统代理或 TUN。
//!
//! 保存时在同一业务锁内重新加载最新 `ClientConfig`，只修改请求相关三个
//! 字段——打开页面后发生的 active 选择、system/tun、MITM、端口等无关
//! 修改不会被本页草稿覆盖；自定义代理经共享 `validate_request_proxy`
//! 校验，非法输入不落盘，错误不回显凭据。日志只记模式，不记地址/前缀。

use std::path::Path;
use std::sync::Arc;

use lanpath_client::{ClientConfig, RequestProxyMode};
use lanpath_common::LanpathResult;

use crate::data_root::{DataRoot, open_root, resolve_data_location};

/// 请求设置文档；与 GPUI `RequestSettingsDocument` 对齐。代理凭据
/// 刻意不实现 Debug，避免任何日志/调试路径带出认证信息。
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct RequestSettingsDocument {
    mode: RequestProxyMode,
    custom_url: String,
    github_prefix: String,
}

impl RequestSettingsDocument {
    pub(crate) fn new(mode: RequestProxyMode, custom_url: String, github_prefix: String) -> Self {
        Self {
            mode,
            custom_url,
            github_prefix,
        }
    }

    pub(crate) fn mode(&self) -> RequestProxyMode {
        self.mode
    }

    pub(crate) fn custom_url(&self) -> &str {
        &self.custom_url
    }

    pub(crate) fn github_prefix(&self) -> &str {
        &self.github_prefix
    }

    fn from_config(config: &ClientConfig) -> Self {
        Self::new(
            config.request_proxy_mode,
            config.request_proxy_url.clone(),
            config.github_proxy_prefix.clone(),
        )
    }
}

/// 请求设置业务句柄：仅持有共享数据根，克隆廉价；所有操作临时取业务锁，
/// 与订阅/覆写业务的清单、配置写操作在同一临界区排队。
#[derive(Clone)]
pub(crate) struct RequestSettings {
    root: Arc<DataRoot>,
}

impl RequestSettings {
    /// 解析本进程数据根并打开请求设置业务（应用启动路径）。
    pub(crate) fn open() -> LanpathResult<Self> {
        let location = resolve_data_location()?;
        Self::for_root(&location.path)
    }

    /// 指定数据根打开（测试与显式调用路径，同样走完整准入）。
    pub(crate) fn for_root(path: &Path) -> LanpathResult<Self> {
        Ok(Self {
            root: open_root(path)?,
        })
    }

    /// 读取已保存的请求设置；只读不写——页面打开/刷新不会改写配置文件。
    /// 首次无配置返回新装直连默认（见 [`load_request_config`]），已有配置
    /// 损坏返回真实错误，绝不重写为默认。
    pub(crate) fn load(&self) -> LanpathResult<RequestSettingsDocument> {
        let _guard = self.root.business_guard();
        let config = load_request_config(self.root.path())?;
        Ok(RequestSettingsDocument::from_config(&config))
    }

    /// 保存请求设置：同一业务锁内重新加载最新配置后只改
    /// `request_proxy_mode` / `request_proxy_url` / `github_proxy_prefix`
    /// 三个字段，再经共享 `validate_request_proxy` 校验（Custom 非法地址
    /// 拒绝且不写盘，错误不含凭据/完整 URL），最后落盘并返回已保存文档。
    /// 其他模式同样保存 `custom_url` 草稿，供稍后切回 Custom（与 GPUI 一致）。
    pub(crate) fn save(
        &self,
        settings: &RequestSettingsDocument,
    ) -> LanpathResult<RequestSettingsDocument> {
        let _guard = self.root.business_guard();
        self.root.ensure_writable()?;
        let mut config = load_request_config(self.root.path())?;
        config.request_proxy_mode = settings.mode;
        config.request_proxy_url.clone_from(&settings.custom_url);
        config
            .github_proxy_prefix
            .clone_from(&settings.github_prefix);
        config.validate_request_proxy()?;
        config.save()?;
        tracing::info!(target: "lanpath_desktop_nsh::request_settings", "保存请求设置：模式 {:?}", settings.mode);
        Ok(RequestSettingsDocument::from_config(&config))
    }
}

/// 本应用的网络请求配置：共享 `load_for_requests` 读请求设置，`data_dir`
/// 由它固定为本应用数据根，不信任文件里的旧路径。统一 TOML 布局下配置
/// 固定为 `app.toml`（数据根初始化已建并持久化新装默认：直连 `None`、
/// `mitm_enabled=false`，与 GPUI 为新配置设定的策略一致，避免环境系统
/// 代理把订阅地址转发到第三方）。已有配置原样保留
/// `request_proxy_mode` / `request_proxy_url` / `github_proxy_prefix` 等
/// 请求字段（active 的保存不改写它们，刷新遵守已保存策略）。
///
/// 订阅刷新（`Subscriptions::refresh`）与本页保存共用这一个入口，
/// 保证新装默认与请求策略读取不分叉。
pub(crate) fn load_request_config(root: &Path) -> LanpathResult<ClientConfig> {
    ClientConfig::load_for_requests(root)
}

#[cfg(test)]
#[path = "request_settings_tests.rs"]
mod tests;
