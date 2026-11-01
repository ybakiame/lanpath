use std::path::{Path, PathBuf};

use lanpath_client::{ClientConfig, RequestProxyMode};
use lanpath_common::{LanpathError, LanpathResult};

use super::BusinessService;

impl BusinessService {
    pub(crate) fn client_config(&self) -> LanpathResult<ClientConfig> {
        let is_new =
            !crate::configuration::resources::confined(&self.root, "client.json", "client.json")?
                .is_file()
                && !lanpath_client::settings::app_settings_path(&self.root).is_file();
        lanpath_client::settings::ensure_app_settings(&self.root)?;
        let path = lanpath_client::settings::app_settings_path(&self.root);
        let mut config = if is_new {
            ClientConfig::new(
                self.root.clone(),
                String::new(),
                String::new(),
                PathBuf::new(),
            )
        } else {
            ClientConfig::load(&self.root)?
        };
        if is_new {
            config.mitm_enabled = false;
            config.request_proxy_mode = RequestProxyMode::None;
            config.mixed_port = self.bindings.mixed_port();
            config.clash_api_port = self.bindings.api_port();
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
    if !config.core_binary.as_os_str().is_empty()
        && let Some(core_type) = lanpath_client::cores::infer_core_type(&config.core_binary)
        && config.core_type != core_type
    {
        config.core_type = core_type;
        changed = true;
    }
    if !config.core_binary.as_os_str().is_empty()
        && lanpath_client::core_management::CoreManagementService::validate_managed_binary(
            &service.root,
            config.core_type,
            &config.core_binary,
        )
        .is_err()
    {
        // A missing or escaped managed core cannot become an executable fallback.
        config.core_binary.clear();
        changed = true;
    }
    enforce!(config.mitm.ca_dir, service.root.join("certs"));
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
    let stored: Paths = serde_json::from_value(lanpath_client::settings::load_client(path)?)?;
    Ok(stored.data_dir.as_deref() == Some(service.root.as_path())
        && stored.mitm.and_then(|mitm| mitm.ca_dir).as_deref()
            == Some(service.root.join("certs").as_path()))
}

pub(super) fn persist(path: &Path, config: &ClientConfig) -> LanpathResult<()> {
    lanpath_client::settings::persist_client(path, &serde_json::to_value(config)?)
}
