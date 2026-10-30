use lanpath_client::{
    DelayResult,
    core_config::push_clash_mode,
    proxies::{
        ProxyList, clash_get_proxies, clash_select_proxy, clash_test_delay, clash_test_group,
        persist_group_selection,
    },
};

use crate::business::BusinessService;

#[derive(Debug)]
pub(crate) struct ModeChange {
    pub(crate) mode: String,
    pub(crate) runtime_warning: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ProxyService {
    business: Option<BusinessService>,
}

impl ProxyService {
    pub(crate) fn new(business: Option<BusinessService>) -> Self {
        Self { business }
    }
    pub(crate) async fn list(&self) -> Result<ProxyList, String> {
        let (port, secret) = self.endpoint()?;
        clash_get_proxies(port, &secret)
            .await
            .map_err(|error| error.to_string())
    }

    pub(crate) async fn select(&self, group: &str, name: &str) -> Result<ProxyList, String> {
        let (port, secret) = self.endpoint()?;
        clash_select_proxy(port, &secret, group, name)
            .await
            .map_err(|error| error.to_string())?;
        let business = self.business()?;
        persist_group_selection(business.data_dir(), group, name)
            .map_err(|error| error.to_string())?;
        self.list().await
    }

    pub(crate) async fn test_delay(&self, name: &str) -> Result<Option<u16>, String> {
        let (port, secret) = self.endpoint()?;
        clash_test_delay(port, &secret, name, None, 5000)
            .await
            .map_err(|error| error.to_string())
    }

    pub(crate) async fn test_group(&self, group: &str) -> Result<Vec<DelayResult>, String> {
        let (port, secret) = self.endpoint()?;
        clash_test_group(port, &secret, group)
            .await
            .map_err(|error| error.to_string())
    }

    pub(crate) fn rule_mode(&self) -> Result<String, String> {
        self.business()?
            .rule_mode()
            .map_err(|error| error.to_string())
    }

    pub(crate) async fn set_rule_mode(
        &self,
        mode: &str,
        core_running: bool,
    ) -> Result<ModeChange, String> {
        self.business()?
            .set_rule_mode(mode)
            .map_err(|error| error.to_string())?;
        let runtime_warning = if core_running {
            match self.endpoint() {
                Ok((port, secret)) => push_clash_mode(port, &secret, mode)
                    .await
                    .err()
                    .map(|error| error.to_string()),
                Err(error) => Some(error),
            }
        } else {
            None
        };
        Ok(ModeChange {
            mode: mode.to_string(),
            runtime_warning,
        })
    }

    fn endpoint(&self) -> Result<(u16, String), String> {
        self.business()?
            .clash_api_endpoint()
            .map_err(|error| error.to_string())
    }

    fn business(&self) -> Result<&BusinessService, String> {
        self.business
            .as_ref()
            .ok_or_else(|| "实验数据目录不可用".to_string())
    }
}
