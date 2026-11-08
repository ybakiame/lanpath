mod config;
mod config_slices;
mod mitm;
mod profiles;
mod request_settings;
mod rules;
mod scripts;
mod state;
mod system_settings;

pub(crate) use config_slices::ConfigSlicesDocument;
pub(crate) use mitm::MitmSettingsDocument;
pub(crate) use request_settings::RequestSettingsDocument;
pub(crate) use rules::LocalOverrideDocument;
pub(crate) use system_settings::SystemSettingsDocument;
mod subscriptions;

use std::path::{Path, PathBuf};

use crate::configuration::Bindings;

#[derive(Clone)]
pub(crate) struct BusinessService {
    root: PathBuf,
    binary: Option<PathBuf>,
    bindings: Bindings,
}

impl BusinessService {
    pub(crate) fn new(root: &Path, binary: Option<PathBuf>, bindings: Bindings) -> Self {
        Self {
            root: root.to_owned(),
            binary,
            bindings,
        }
    }

    pub(crate) fn validator(
        &self,
        config: &lanpath_client::ClientConfig,
    ) -> crate::configuration::ConfigValidator {
        let bindings = Bindings::from_config(config);
        let binary = self.binary.as_ref().unwrap_or(&config.core_binary);
        let core_type = lanpath_client::cores::infer_core_type(binary).unwrap_or(config.core_type);
        crate::configuration::ConfigValidator::new(
            &self.root,
            (!binary.as_os_str().is_empty()).then(|| binary.clone()),
            core_type,
            bindings,
        )
    }

    pub(crate) fn data_dir(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod config_snapshot_tests;
#[cfg(test)]
mod core_selection_tests;
#[cfg(test)]
mod mitm_resource_tests;
#[cfg(test)]
mod mitm_tests;
#[cfg(test)]
mod request_settings_tests;
#[cfg(test)]
mod scripts_tests;
#[cfg(test)]
mod system_settings_tests;
#[cfg(test)]
mod tests;
