mod config;
mod config_slices;
mod profiles;
mod rules;
mod state;

pub(crate) use config_slices::ConfigSlicesDocument;
pub(crate) use rules::LocalOverrideDocument;
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

    pub(crate) fn validator(&self) -> crate::configuration::ConfigValidator {
        crate::configuration::ConfigValidator::new(&self.root, self.binary.clone(), self.bindings)
    }

    pub(crate) fn data_dir(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests;
