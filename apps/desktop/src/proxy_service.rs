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
    port: u16,
    business: Option<BusinessService>,
}

impl ProxyService {
    pub(crate) fn new(port: u16, business: Option<BusinessService>) -> Self {
        Self { port, business }
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
        let business = self.business()?;
        persist_group_selection(business.data_dir(), group, name)
            .map_err(|error| error.to_string())?;
        self.list().await
    }

    pub(crate) async fn test_delay(&self, name: &str) -> Result<Option<u16>, String> {
        clash_test_delay(self.port, "", name, None, 5000)
            .await
            .map_err(|error| error.to_string())
    }

    pub(crate) async fn test_group(&self, group: &str) -> Result<Vec<DelayResult>, String> {
        clash_test_group(self.port, "", group)
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
            push_clash_mode(self.port, "", mode)
                .await
                .err()
                .map(|error| error.to_string())
        } else {
            None
        };
        Ok(ModeChange {
            mode: mode.to_string(),
            runtime_warning,
        })
    }

    fn business(&self) -> Result<&BusinessService, String> {
        self.business
            .as_ref()
            .ok_or_else(|| "实验数据目录不可用".to_string())
    }
}
