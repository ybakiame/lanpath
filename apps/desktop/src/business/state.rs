use std::{
    io::Write,
    path::{Path, PathBuf},
};

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
            config.request_proxy_mode = RequestProxyMode::None;
        }
        let changed = normalize(&mut config, self);
        if is_new || changed || !persisted_paths_match(&path, self)? {
            persist(&path, &config)?;
        }
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

fn normalize(config: &mut ClientConfig, service: &BusinessService) -> bool {
    let mut changed = false;
    macro_rules! enforce {
        ($field:expr, $value:expr) => {{
            let value = $value;
            if $field != value {
                $field = value;
                changed = true;
            }
        }};
    }
    if config.data_dir != service.root {
        config.data_dir.clone_from(&service.root);
        changed = true;
    }
    let binary = service.binary.as_deref().unwrap_or_else(|| Path::new(""));
    if config.core_binary != binary {
        config.core_binary = binary.to_path_buf();
        changed = true;
    }
    enforce!(config.mixed_port, service.bindings.mixed_port());
    // MITM remains opt-in within the experimental root and loopback chain.
    enforce!(config.mitm.ca_dir, service.root.join("certs"));
    enforce!(config.system_proxy_enabled, false);
    enforce!(config.tun_enabled, false);
    enforce!(config.tun_auto_route, false);
    enforce!(config.clash_api_enabled, true);
    enforce!(config.clash_api_port, service.bindings.api_port());
    if !config.clash_api_secret.is_empty() {
        config.clash_api_secret.clear();
        changed = true;
    }
    changed
}

// ClientConfig::load relocates these two paths in memory before returning.
// Inspect their stored values as well, otherwise path-only tampering would never
// be repaired on disk once unchanged reads stop rewriting client.json.
fn persisted_paths_match(path: &Path, service: &BusinessService) -> LanpathResult<bool> {
    #[derive(serde::Deserialize)]
    struct Paths {
        data_dir: Option<PathBuf>,
        mitm: Option<CaPath>,
    }
    #[derive(serde::Deserialize)]
    struct CaPath {
        ca_dir: Option<PathBuf>,
    }
    let stored: Paths = serde_json::from_reader(std::fs::File::open(path)?)?;
    Ok(stored.data_dir.as_deref() == Some(service.root.as_path())
        && stored.mitm.and_then(|mitm| mitm.ca_dir).as_deref()
            == Some(service.root.join("certs").as_path()))
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
