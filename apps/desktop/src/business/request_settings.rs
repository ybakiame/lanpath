use lanpath_client::{ClientConfig, RequestProxyMode};
use lanpath_common::LanpathResult;

use super::{BusinessService, state::persist};

/// The request-only settings seam; proxy credentials are deliberately not Debug.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct RequestSettingsDocument {
    mode: RequestProxyMode,
    custom_url: String,
    github_prefix: String,
}

impl RequestSettingsDocument {
    pub(crate) fn new(mode: RequestProxyMode, custom_url: String, github_prefix: String) -> Self {
        Self {
            mode,
            custom_url,
            github_prefix,
        }
    }

    pub(crate) fn mode(&self) -> RequestProxyMode {
        self.mode
    }

    pub(crate) fn custom_url(&self) -> &str {
        &self.custom_url
    }

    pub(crate) fn github_prefix(&self) -> &str {
        &self.github_prefix
    }

    fn from_config(config: &ClientConfig) -> Self {
        Self::new(
            config.request_proxy_mode,
            config.request_proxy_url.clone(),
            config.github_proxy_prefix.clone(),
        )
    }
}

impl BusinessService {
    pub(crate) fn request_settings(&self) -> LanpathResult<RequestSettingsDocument> {
        self.client_config()
            .map(|config| RequestSettingsDocument::from_config(&config))
    }

    pub(crate) fn save_request_settings(
        &self,
        settings: &RequestSettingsDocument,
    ) -> LanpathResult<RequestSettingsDocument> {
        // Reload at commit time so an editor snapshot cannot replace unrelated
        // selection, routing or MITM changes made since the page was opened.
        let mut config = self.client_config()?;
        config.request_proxy_mode = settings.mode;
        config.request_proxy_url.clone_from(&settings.custom_url);
        config
            .github_proxy_prefix
            .clone_from(&settings.github_prefix);
        config.validate_request_proxy()?;
        persist(&config.config_file(), &config)?;
        Ok(RequestSettingsDocument::from_config(&config))
    }

    /// Capture the saved settings and the full effective request policy together.
    /// The caller owns this snapshot across the async probe, never a draft.
    pub(crate) fn saved_request_policy(
        &self,
    ) -> LanpathResult<(RequestSettingsDocument, ClientConfig)> {
        let config = self.client_config()?;
        Ok((RequestSettingsDocument::from_config(&config), config))
    }
}
