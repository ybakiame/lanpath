//! Shared outbound policy for client subscription and remote-resource requests.
use std::time::Duration;

use lanpath_common::{LanpathError, LanpathResult};

use crate::{ClientConfig, RequestProxyMode};

pub(crate) fn build_client(
    timeout: Duration,
    cfg: &ClientConfig,
) -> LanpathResult<reqwest::Client> {
    build_builder(cfg)?
        .timeout(timeout)
        .build()
        .map_err(|e| LanpathError::Client(format!("无法创建网络请求客户端: {}", e.without_url())))
}

pub(crate) fn build_builder(cfg: &ClientConfig) -> LanpathResult<reqwest::ClientBuilder> {
    cfg.validate_request_proxy()?;
    let builder = reqwest::Client::builder().cookie_store(true);
    let builder = match cfg.request_proxy_mode {
        RequestProxyMode::System => builder,
        RequestProxyMode::None => builder.no_proxy(),
        RequestProxyMode::Core | RequestProxyMode::Custom => {
            let address = if cfg.request_proxy_mode == RequestProxyMode::Core {
                format!("http://127.0.0.1:{}", cfg.mixed_port)
            } else {
                cfg.request_proxy_url.trim().to_owned()
            };
            let proxy = reqwest::Proxy::all(address)
                .map_err(|_| LanpathError::Client("无法创建请求代理，请检查网络请求设置".into()))?;
            builder.no_proxy().proxy(proxy)
        }
    };
    Ok(builder)
}
