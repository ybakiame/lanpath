//! GPUI subscriptions have their own editable TOML document.
use lanpath_common::{LanpathError, LanpathResult};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SUBSCRIPTIONS_FILE: &str = "subscriptions.toml";
#[derive(Default, Serialize, Deserialize)]
pub(crate) struct SubscriptionDocument {
    #[serde(default)]
    pub(crate) subscriptions: Vec<crate::Subscription>,
}

pub fn load_subscriptions(root: &Path) -> LanpathResult<Vec<crate::Subscription>> {
    match std::fs::read_to_string(root.join(SUBSCRIPTIONS_FILE)) {
        Ok(text) => Ok(toml::from_str::<SubscriptionDocument>(&text)
            .map_err(|error| LanpathError::Config(error.to_string()))?
            .subscriptions),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if root.join("subscriptions.json").exists()
                || crate::settings::load_app_section::<serde_json::Value>(root, "resources")?
                    .is_some_and(|section| section.get("subscriptions").is_some())
            {
                return Err(LanpathError::Config(
                    "旧订阅清单尚未迁移，请重启 GPUI 完成自动迁移".into(),
                ));
            }
            Ok(Vec::new())
        }
        Err(error) => Err(error.into()),
    }
}

/// Called by the startup migration only after the old source was backed up.
pub fn persist_subscriptions(
    root: &Path,
    subscriptions: &[crate::Subscription],
) -> LanpathResult<()> {
    crate::settings::persist_document_fields(
        &root.join(SUBSCRIPTIONS_FILE),
        &SubscriptionDocument {
            subscriptions: subscriptions.to_vec(),
        },
    )
}

pub(crate) fn subscriptions_text(subscriptions: Vec<crate::Subscription>) -> LanpathResult<String> {
    toml::to_string_pretty(&SubscriptionDocument { subscriptions })
        .map_err(|error| LanpathError::Config(error.to_string()))
}
