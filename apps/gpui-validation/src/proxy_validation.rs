use std::time::Duration;

use lanpath_client::{
    core_config::wait_clash_api_ready,
    proxies::{ProxyList, clash_get_proxies, clash_select_proxy},
};

#[derive(Clone)]
pub(crate) struct ProxyValidationService {
    port: u16,
}

impl ProxyValidationService {
    pub(crate) fn new(port: u16) -> Self {
        Self { port }
    }

    pub(crate) async fn wait_ready(&self) -> Result<(), String> {
        wait_clash_api_ready(self.port, "", Duration::from_secs(15))
            .await
            .map_err(|error| error.to_string())
    }

    pub(crate) async fn list(&self) -> Result<ProxyList, String> {
        clash_get_proxies(self.port, "")
            .await
            .map_err(|error| error.to_string())
    }

    pub(crate) async fn select(&self, group: &str, name: &str) -> Result<ProxyList, String> {
        clash_select_proxy(self.port, "", group, name)
            .await
            .map_err(|error| error.to_string())?;
        self.list().await
    }
}
