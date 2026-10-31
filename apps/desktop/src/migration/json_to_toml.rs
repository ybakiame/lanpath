use std::path::Path;

use lanpath_client::{ClientConfig, settings};
use lanpath_common::LanpathResult;

use super::SettingsMigration;
use crate::appearance::AppearanceStore;

pub(super) struct JsonToToml;

const LEGACY_FILES: &[&str] = &["client.json", "appearance.json"];
const OUTPUT_FILES: &[&str] = &["app.toml"];

impl SettingsMigration for JsonToToml {
    fn id(&self) -> &'static str {
        "json-to-toml-v1"
    }

    fn is_required(&self, root: &Path) -> LanpathResult<bool> {
        Ok(LEGACY_FILES.iter().any(|name| root.join(name).is_file()))
    }

    fn legacy_files(&self) -> &'static [&'static str] {
        LEGACY_FILES
    }

    fn migrate(&self, root: &Path) -> LanpathResult<()> {
        settings::ensure_app_settings(root)
    }

    fn validate(&self, root: &Path) -> LanpathResult<()> {
        ClientConfig::load(root)?;
        AppearanceStore::new(root).load()?;
        Ok(())
    }

    fn output_files(&self) -> &'static [&'static str] {
        OUTPUT_FILES
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn legacy_settings_are_backed_up_before_becoming_authoritative_toml() {
        let root = tempfile::tempdir().unwrap();
        let client = serde_json::to_vec(&ClientConfig::new(
            root.path().into(),
            String::new(),
            String::new(),
            PathBuf::new(),
        ))
        .unwrap();
        std::fs::write(root.path().join("client.json"), &client).unwrap();
        std::fs::write(
            root.path().join("appearance.json"),
            br#"{"theme":"dark","accent":"blue","pure_black":false,"scale_percent":117}"#,
        )
        .unwrap();

        assert!(super::super::is_required(root.path()).unwrap());
        let backups = super::super::run(root.path()).unwrap();
        assert_eq!(backups.len(), 1);
        assert!(!super::super::is_required(root.path()).unwrap());
        assert!(!root.path().join("client.json").exists());
        assert!(!root.path().join("appearance.json").exists());
        assert_eq!(
            std::fs::read(backups[0].join("legacy/client.json")).unwrap(),
            client
        );
        assert!(backups[0].join("legacy/client.json").is_file());
        assert!(backups[0].join("converted/app.toml").is_file());
        assert!(backups[0].join("manifest.json").is_file());
        assert!(root.path().join("app.toml").is_file());
    }

    #[test]
    fn existing_app_settings_is_preserved_while_legacy_leftovers_are_removed() {
        let root = tempfile::tempdir().unwrap();
        let config = ClientConfig::new(
            root.path().into(),
            String::new(),
            String::new(),
            PathBuf::new(),
        );
        std::fs::write(
            root.path().join("client.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        settings::ensure_app_settings(root.path()).unwrap();
        let app = std::fs::read(root.path().join("app.toml")).unwrap();
        std::fs::write(root.path().join("client.json"), "stale").unwrap();

        let backups = super::super::run(root.path()).unwrap();
        assert_eq!(std::fs::read(root.path().join("app.toml")).unwrap(), app);
        assert!(!root.path().join("client.json").exists());
        assert_eq!(
            std::fs::read_to_string(backups[0].join("legacy/client.json")).unwrap(),
            "stale"
        );
    }

    #[test]
    fn invalid_legacy_input_does_not_publish_a_new_settings_file() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("client.json"), "{broken").unwrap();

        assert!(super::super::run(root.path()).is_err());
        assert!(!root.path().join("app.toml").exists());
        assert_eq!(
            std::fs::read_to_string(root.path().join("client.json")).unwrap(),
            "{broken"
        );
    }
}
