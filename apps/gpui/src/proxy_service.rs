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

#[cfg(test)]
mod tests {
    use super::ProxyService;
    use crate::{business::BusinessService, configuration::Bindings};
    use lanpath_client::{CachedSubscriptionContent, SubFormat};
    use lanpath_common::CoreType;
    use serde_json::json;

    #[tokio::test]
    async fn saved_mode_drives_both_core_configs_without_replacing_structured_settings() {
        for core_type in [CoreType::SingBox, CoreType::Mihomo] {
            let root = tempfile::tempdir().unwrap();
            let business = BusinessService::new(root.path(), None, Bindings::new(17899, 19090));
            let mut config = business.client_config().unwrap();
            config.core_type = core_type;
            config.clash_api_enabled = true;
            config.save().unwrap();
            lanpath_client::settings::persist_app_section(
                root.path(),
                "core",
                &lanpath_client::settings::CoreSettings {
                    kind: core_type,
                    ..Default::default()
                },
            )
            .unwrap();
            let store = business.subscription_store();
            let selected = store
                .add(
                    "mode fixture",
                    "https://example.invalid/fixture",
                    true,
                    None,
                )
                .unwrap();
            store.write_cached_content(selected.id,&CachedSubscriptionContent {format:SubFormat::SingBoxJson,singbox_nodes:vec![json!({"type":"shadowsocks","tag":"fixture","server":"127.0.0.1","server_port":8388,"method":"aes-256-gcm","password":"fixture"})]}).unwrap();
            business.set_active_subscription(Some(selected.id)).unwrap();
            let network = business.system_settings().unwrap();
            let slices = business.config_slices().unwrap().slices;
            let service = ProxyService::new(Some(business.clone()));
            for mode in ["global", "direct", "rule"] {
                let change = service.set_rule_mode(mode, false).await.unwrap();
                assert_eq!(change.mode, mode);
                assert!(change.runtime_warning.is_none());
                assert_eq!(service.rule_mode().unwrap(), mode);
                let generated = business.build_config(false).await.unwrap();
                let path = if core_type == CoreType::SingBox {
                    "/experimental/clash_api/default_mode"
                } else {
                    "/mode"
                };
                assert_eq!(generated.pointer(path), Some(&json!(mode)));
                assert!(business.system_settings().unwrap() == network);
                assert_eq!(business.config_slices().unwrap().slices, slices);
            }
        }
    }
    #[tokio::test]
    async fn unavailable_runtime_keeps_saved_mode_and_reports_sync_failure() {
        let root = tempfile::tempdir().unwrap();
        let business = BusinessService::new(root.path(), None, Bindings::new(17899, 19090));
        let mut settings = business.system_settings().unwrap();
        settings.clash_api_enabled = false;
        business.save_system_settings(&settings).unwrap();
        let service = ProxyService::new(Some(business));
        let change = service.set_rule_mode("direct", true).await.unwrap();
        assert_eq!(service.rule_mode().unwrap(), "direct");
        assert!(change.runtime_warning.is_some());
        assert!(service.set_rule_mode("invalid", false).await.is_err());
        assert_eq!(service.rule_mode().unwrap(), "direct");
    }
}
