use lanpath_common::{LanpathError, LanpathResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestProxyMode {
    None,
    #[default]
    System,
    Core,
    Custom,
}

impl super::ClientConfig {
    /// Reject invalid custom settings without including credentials in errors.
    pub fn validate_request_proxy(&self) -> LanpathResult<()> {
        if self.request_proxy_mode != RequestProxyMode::Custom {
            return Ok(());
        }
        let invalid =
            || LanpathError::Client("自定义代理地址无效：请填写 HTTP(S) 或 SOCKS5 代理 URL".into());
        let url = reqwest::Url::parse(self.request_proxy_url.trim()).map_err(|_| invalid())?;
        if !matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h")
            || url.host_str().is_none()
            || url.port() == Some(0)
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(invalid());
        }
        Ok(())
    }
}
