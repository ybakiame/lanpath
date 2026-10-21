use lanpath_client::{ScriptDialect, mitm::MitmWhitelistView};
use lanpath_common::{LanpathError, LanpathResult};

use super::{BusinessService, state::persist};

#[derive(Debug, Clone)]
pub(crate) struct MitmSettingsDocument {
    pub enabled: bool,
    pub hostnames: Vec<String>,
    pub script_dialect: ScriptDialect,
    pub whitelist: MitmWhitelistView,
}

impl BusinessService {
    pub(crate) fn mitm_settings(&self) -> LanpathResult<MitmSettingsDocument> {
        let config = self.client_config()?;
        let service = lanpath_client::mitm::MitmService::new(&self.root);
        Ok(MitmSettingsDocument {
            hostnames: config.mitm.hostnames.clone(),
            enabled: config.mitm_enabled,
            script_dialect: config.mitm.script_dialect,
            whitelist: service.whitelist(&config),
        })
    }

    pub(crate) fn save_mitm_settings(
        &self,
        enabled: bool,
        hostnames: Vec<String>,
        script_dialect: ScriptDialect,
    ) -> LanpathResult<MitmSettingsDocument> {
        let mut normalized = Vec::new();
        for hostname in hostnames {
            let hostname = hostname.trim();
            if hostname.is_empty() {
                continue;
            }
            let pattern = hostname
                .strip_prefix('-')
                .or_else(|| hostname.strip_prefix('!'))
                .unwrap_or(hostname)
                .trim();
            if pattern.is_empty() {
                return Err(LanpathError::Validation(
                    "Hostname 排除项必须包含域名模式".into(),
                ));
            }
            if !normalized.iter().any(|saved| saved == hostname) {
                normalized.push(hostname.to_string());
            }
        }

        let mut config = self.client_config()?;
        config.mitm_enabled = enabled;
        config.mitm.hostnames = normalized;
        config.mitm.script_dialect = script_dialect;
        persist(&config.config_file(), &config)?;
        self.mitm_settings()
    }
}
