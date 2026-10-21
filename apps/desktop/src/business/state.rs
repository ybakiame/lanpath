use std::{io::Write, path::Path};

use lanpath_client::{ClientConfig, RequestProxyMode};
use lanpath_common::{LanpathError, LanpathResult};

use super::BusinessService;
use crate::configuration::resources;

impl BusinessService {
    pub(crate) fn client_config(&self) -> LanpathResult<ClientConfig> {
        let path = resources::confined(&self.root, "client.json", "client.json")?;
        let is_new = !path.is_file();
        let mut config = if is_new {
            ClientConfig::new(
                self.root.clone(),
                String::new(),
                String::new(),
                self.binary.clone().unwrap_or_default(),
            )
        } else {
            ClientConfig::load(&self.root)?
        };
        if is_new {
            config.mitm_enabled = false;
        }
        normalize(&mut config, self);
        persist(&path, &config)?;
        Ok(config)
    }

    pub(crate) fn set_active_subscription(&self, id: Option<uuid::Uuid>) -> LanpathResult<()> {
        let mut config = self.client_config()?;
        if let Some(id) = id {
            let subscriptions = self.subscription_store().load()?;
            let selected = subscriptions
                .iter()
                .find(|subscription| subscription.id == id);
            match selected {
                Some(subscription) if subscription.enabled => {}
                Some(_) => {
                    return Err(LanpathError::Client(
                        "所选订阅已停用，请先启用或刷新订阅".into(),
                    ));
                }
                None => return Err(LanpathError::Client("所选订阅不存在".into())),
            }
        }
        config.active_subscription_id = id;
        persist(&config.config_file(), &config)
    }

    pub(crate) fn active_subscription_id(&self) -> LanpathResult<Option<uuid::Uuid>> {
        Ok(self.client_config()?.active_subscription_id)
    }

    pub(crate) fn rule_mode(&self) -> LanpathResult<String> {
        Ok(self.client_config()?.normalized_rule_mode().to_string())
    }

    pub(crate) fn set_rule_mode(&self, mode: &str) -> LanpathResult<()> {
        self.client_config()?;
        lanpath_client::set_rule_mode_persist(&self.root, mode).map_err(LanpathError::Client)
    }
}

fn normalize(config: &mut ClientConfig, service: &BusinessService) {
    config.data_dir = service.root.clone();
    config.core_binary = service.binary.clone().unwrap_or_default();
    config.mixed_port = service.bindings.mixed_port();
    // MITM remains opt-in and is confined to the experimental root and loopback
    // chain. Preserve saved settings; unlike system proxy and TUN it has no
    // global effect until the user explicitly installs the generated CA.
    config.mitm.ca_dir = service.root.join("certs");
    config.system_proxy_enabled = false;
    config.tun_enabled = false;
    config.tun_auto_route = false;
    config.clash_api_enabled = true;
    config.clash_api_port = service.bindings.api_port();
    config.clash_api_secret.clear();
    config.github_proxy_prefix.clear();
    config.request_proxy_mode = RequestProxyMode::None;
    config.request_proxy_url.clear();
}

pub(super) fn persist(path: &Path, config: &ClientConfig) -> LanpathResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| LanpathError::Config("client.json has no parent directory".into()))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut file, config)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}
