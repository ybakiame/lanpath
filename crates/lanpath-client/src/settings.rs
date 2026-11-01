//! Application-owned settings stored as sections in `app.toml`.
//!
//! Legacy `client.json` remains the default for other shells. GPUI explicitly
//! calls [`ensure_app_settings`], which publishes a unified document without
//! deleting the legacy files. Section writes preserve unrelated TOML content.

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use lanpath_common::{LanpathError, LanpathResult};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use toml_edit::{DocumentMut, Item};

pub const APP_SETTINGS_FILE: &str = "app.toml";
pub const LEGACY_CLIENT_FILE: &str = "client.json";
pub const LEGACY_APPEARANCE_FILE: &str = "appearance.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CoreSettings {
    #[serde(rename = "type")]
    pub kind: String,
    pub channel: String,
    pub current: Option<CoreVersionSettings>,
    pub previous: Option<CoreVersionSettings>,
}

impl Default for CoreSettings {
    fn default() -> Self {
        Self {
            kind: "sing-box".into(),
            channel: "stable".into(),
            current: None,
            previous: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreVersionSettings {
    pub version: String,
    pub path: PathBuf,
}

impl CoreVersionSettings {
    fn from_path(root: &Path, path: &Path) -> Self {
        let path = path.strip_prefix(root).unwrap_or(path).to_path_buf();
        let version = path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        Self {
            version,
            path: path.to_path_buf(),
        }
    }
}

pub fn load_core_settings(root: &Path) -> LanpathResult<Option<CoreSettings>> {
    let mut settings = load_app_section::<CoreSettings>(root, "core")?;
    if let Some(settings) = &mut settings {
        if let Some(current) = &mut settings.current {
            current.path = root.join(&current.path);
        }
        if let Some(previous) = &mut settings.previous {
            previous.path = root.join(&previous.path);
        }
    }
    Ok(settings)
}

fn persist_core_current(root: &Path, path: Option<&Path>) -> LanpathResult<()> {
    let mut settings = load_core_settings(root)?.unwrap_or_default();
    let Some(path) = path.filter(|path| !path.as_os_str().is_empty()) else {
        return Ok(());
    };
    let current = CoreVersionSettings::from_path(root, path);
    if settings
        .current
        .as_ref()
        .is_none_or(|old| old.path != current.path)
    {
        settings.previous = settings.current.take();
    }
    settings.kind = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("sing-box")
        .to_string();
    settings.channel = crate::cores::channel_of_version(&current.version)
        .as_str()
        .to_string();
    settings.current = Some(current);
    for version in [&mut settings.current, &mut settings.previous]
        .into_iter()
        .flatten()
    {
        if let Ok(relative) = version.path.strip_prefix(root) {
            version.path = relative.to_path_buf();
        }
    }
    persist_app_section(root, "core", &settings)
}

pub fn has_flat_core_binary(root: &Path) -> LanpathResult<bool> {
    let path = app_settings_path(root);
    if !path.is_file() {
        return Ok(false);
    }
    Ok(load_client(&path)?.get("core_binary").is_some())
}

pub fn migrate_core_settings(root: &Path) -> LanpathResult<()> {
    let path = app_settings_path(root);
    let mut raw = load_client(&path)?;
    let Some(binary) = raw
        .get("core_binary")
        .and_then(|value| value.as_str())
        .map(PathBuf::from)
    else {
        return Ok(());
    };
    persist_core_current(root, Some(&binary))?;
    if let Some(object) = raw.as_object_mut() {
        object.remove("core_binary");
    }
    persist_client(&path, &raw)
}

pub fn app_settings_path(root: &Path) -> PathBuf {
    root.join(APP_SETTINGS_FILE)
}

pub fn client_settings_path(root: &Path) -> PathBuf {
    let app = app_settings_path(root);
    if app.exists() {
        app
    } else {
        root.join(LEGACY_CLIENT_FILE)
    }
}

pub fn is_app_settings(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name == APP_SETTINGS_FILE)
}

/// Create `app.toml` from legacy sections. Existing files are never removed or
/// rewritten, and an existing unified document always takes precedence.
pub fn ensure_app_settings(root: &Path) -> LanpathResult<()> {
    let path = app_settings_path(root);
    if path.exists() {
        return Ok(());
    }
    let mut document = DocumentMut::new();
    for (name, section) in [
        (LEGACY_CLIENT_FILE, "client"),
        (LEGACY_APPEARANCE_FILE, "appearance"),
    ] {
        match std::fs::read(root.join(name)) {
            Ok(bytes) => {
                let mut value: serde_json::Value = serde_json::from_slice(&bytes)?;
                if section == "client"
                    && let Some(binary) = value
                        .get("core_binary")
                        .and_then(|value| value.as_str())
                        .map(PathBuf::from)
                {
                    let current = CoreVersionSettings::from_path(root, &binary);
                    let core = CoreSettings {
                        kind: binary
                            .file_stem()
                            .and_then(|name| name.to_str())
                            .unwrap_or("sing-box")
                            .to_string(),
                        channel: crate::cores::channel_of_version(&current.version)
                            .as_str()
                            .to_string(),
                        current: Some(current),
                        previous: None,
                    };
                    document["core"] = serialize_item(&core)?;
                    if let Some(object) = value.as_object_mut() {
                        object.remove("core_binary");
                    }
                }
                document[section] = serialize_item(&value)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    atomic_write(&path, &document.to_string())
}

pub fn load_client(path: &Path) -> LanpathResult<serde_json::Value> {
    let text = std::fs::read_to_string(path)?;
    client_from_text(path, &text)
}

pub fn client_from_text(path: &Path, text: &str) -> LanpathResult<serde_json::Value> {
    if is_app_settings(path) {
        let document =
            toml::from_str::<toml::Value>(text).map_err(|error| config_error(path, error))?;
        let client = document
            .get("client")
            .cloned()
            .unwrap_or_else(|| toml::Value::Table(Default::default()));
        let mut value = serde_json::to_value(client)?;
        if let Some(root) = path.parent()
            && let Some(current) = load_core_settings(root)?.and_then(|core| core.current)
        {
            value["core_binary"] = serde_json::to_value(current.path)?;
        }
        Ok(value)
    } else {
        Ok(serde_json::from_str(text)?)
    }
}

pub fn persist_client(path: &Path, value: &serde_json::Value) -> LanpathResult<()> {
    if is_app_settings(path) {
        if let Some(binary) = value.get("core_binary").and_then(|value| value.as_str()) {
            persist_core_current(
                path.parent().unwrap_or_else(|| Path::new(".")),
                Some(Path::new(binary)),
            )?;
        }
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let mut value = value.clone();
        if let Some(object) = value.as_object_mut() {
            object.remove("core_binary");
        }
        let merged = merge_app_section_text(&text, "client", &value)?;
        atomic_write(path, &merged)
    } else {
        let mut text = serde_json::to_string_pretty(value)?;
        text.push('\n');
        atomic_write(path, &text)
    }
}

pub fn load_app_section<T: DeserializeOwned>(
    root: &Path,
    section: &str,
) -> LanpathResult<Option<T>> {
    let path = app_settings_path(root);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let document =
        toml::from_str::<toml::Value>(&text).map_err(|error| config_error(&path, error))?;
    document
        .get(section)
        .cloned()
        .map(|value| {
            let value = serde_json::to_value(value)?;
            Ok(serde_json::from_value(value)?)
        })
        .transpose()
}

pub fn persist_app_section<T: Serialize>(
    root: &Path,
    section: &str,
    value: &T,
) -> LanpathResult<()> {
    let path = app_settings_path(root);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let merged = merge_app_section_text(&text, section, value)?;
    atomic_write(&path, &merged)
}

/// Replace one section while preserving unrelated sections and their comments.
pub fn merge_app_section_text<T: Serialize>(
    original: &str,
    section: &str,
    value: &T,
) -> LanpathResult<String> {
    let path = Path::new(APP_SETTINGS_FILE);
    let mut document = if original.trim().is_empty() {
        DocumentMut::new()
    } else {
        original
            .parse::<DocumentMut>()
            .map_err(|error| config_error(path, error))?
    };
    document[section] = serialize_item(value)?;
    Ok(document.to_string())
}

fn serialize_item<T: Serialize>(value: &T) -> LanpathResult<Item> {
    let value = without_nulls(serde_json::to_value(value)?);
    toml_edit::ser::to_document(&value)
        .map(DocumentMut::into_item)
        .map_err(|error| config_error(Path::new(APP_SETTINGS_FILE), error))
}

fn without_nulls(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(object) => object
            .into_iter()
            .filter(|(_, value)| !value.is_null())
            .map(|(key, value)| (key, without_nulls(value)))
            .collect::<serde_json::Map<_, _>>()
            .into(),
        serde_json::Value::Array(values) => values
            .into_iter()
            .filter(|value| !value.is_null())
            .map(without_nulls)
            .collect::<Vec<_>>()
            .into(),
        scalar => scalar,
    }
}

fn atomic_write(path: &Path, text: &str) -> LanpathResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| LanpathError::Config(format!("{} has no parent", path.display())))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(text.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn config_error(path: &Path, error: impl std::fmt::Display) -> LanpathError {
    LanpathError::Config(format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_preserves_legacy_files_and_section_writes_preserve_other_content() {
        let root = tempfile::tempdir().unwrap();
        let legacy_client = br#"{"mixed_port":17899}"#;
        let legacy_appearance = br#"{"scale_percent":117}"#;
        std::fs::write(root.path().join(LEGACY_CLIENT_FILE), legacy_client).unwrap();
        std::fs::write(root.path().join(LEGACY_APPEARANCE_FILE), legacy_appearance).unwrap();

        ensure_app_settings(root.path()).unwrap();
        assert_eq!(
            std::fs::read(root.path().join(LEGACY_CLIENT_FILE)).unwrap(),
            legacy_client
        );
        assert_eq!(
            std::fs::read(root.path().join(LEGACY_APPEARANCE_FILE)).unwrap(),
            legacy_appearance
        );
        assert_eq!(
            load_client(&app_settings_path(root.path())).unwrap()["mixed_port"],
            17899
        );

        let original = std::fs::read_to_string(app_settings_path(root.path()))
            .unwrap()
            .replace("[appearance]", "# retained comment\n[appearance]");
        std::fs::write(app_settings_path(root.path()), original).unwrap();
        persist_app_section(
            root.path(),
            "client",
            &serde_json::json!({"mixed_port": 18000}),
        )
        .unwrap();

        let text = std::fs::read_to_string(app_settings_path(root.path())).unwrap();
        assert!(text.contains("# retained comment"));
        let appearance: serde_json::Value = load_app_section(root.path(), "appearance")
            .unwrap()
            .unwrap();
        assert_eq!(appearance["scale_percent"], 117);
        assert_eq!(
            load_client(&app_settings_path(root.path())).unwrap()["mixed_port"],
            18000
        );
    }

    #[test]
    fn nested_core_selection_tracks_current_and_previous_versions() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("cores/sing-box/1.2.3/sing-box");
        let second = root.path().join("cores/sing-box/1.2.4/sing-box");
        std::fs::write(
            root.path().join(LEGACY_CLIENT_FILE),
            serde_json::to_vec(&serde_json::json!({"core_binary": first})).unwrap(),
        )
        .unwrap();

        ensure_app_settings(root.path()).unwrap();
        let core = load_core_settings(root.path()).unwrap().unwrap();
        assert_eq!(core.current.as_ref().unwrap().version, "1.2.3");
        assert!(core.previous.is_none());

        persist_client(
            &app_settings_path(root.path()),
            &serde_json::json!({"core_binary": second}),
        )
        .unwrap();
        let persisted = load_app_section::<CoreSettings>(root.path(), "core")
            .unwrap()
            .unwrap();
        assert_eq!(
            persisted.current.as_ref().unwrap().path,
            Path::new("cores/sing-box/1.2.4/sing-box")
        );
        assert_eq!(
            persisted.previous.as_ref().unwrap().path,
            Path::new("cores/sing-box/1.2.3/sing-box")
        );
        let core = load_core_settings(root.path()).unwrap().unwrap();
        assert_eq!(core.previous.as_ref().unwrap().version, "1.2.3");
        assert_eq!(core.kind, "sing-box");
        assert_eq!(core.channel, "stable");
    }
}
