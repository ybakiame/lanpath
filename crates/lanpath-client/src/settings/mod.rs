//! Application-owned settings stored as sections in `app.toml`.
//!
//! Legacy `client.json` remains the default for other shells. GPUI explicitly
//! calls [`ensure_app_settings`], which publishes a unified document without
//! deleting the legacy files. Section writes preserve unrelated TOML content.

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use lanpath_common::{CoreType, LanpathError, LanpathResult};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use toml_edit::{DocumentMut, Item};

static APP_WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn app_write_lock() -> LanpathResult<std::sync::MutexGuard<'static, ()>> {
    APP_WRITE_LOCK
        .lock()
        .map_err(|_| LanpathError::Config("应用配置写入锁不可用".into()))
}

pub const APP_SETTINGS_FILE: &str = "app.toml";
pub const LEGACY_CLIENT_FILE: &str = "client.json";
pub const LEGACY_APPEARANCE_FILE: &str = "appearance.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CoreSettings {
    #[serde(rename = "type")]
    pub kind: CoreType,
    pub channel: String,
    pub current: Option<CoreVersionSettings>,
    pub previous: Option<CoreVersionSettings>,
}

impl Default for CoreSettings {
    fn default() -> Self {
        Self {
            kind: CoreType::SingBox,
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
        Self { version, path }
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
    let same_as_current = settings
        .current
        .as_ref()
        .is_some_and(|old| old.path.strip_prefix(root).unwrap_or(&old.path) == current.path);
    if !same_as_current {
        settings.previous = settings.current.take();
    }
    settings.kind = crate::cores::infer_core_type(path).unwrap_or(settings.kind);
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
    Ok(load_app_section::<serde_json::Value>(root, "client")?
        .as_ref()
        .and_then(|client| client.get("core_binary"))
        .is_some())
}

pub fn migrate_core_settings(root: &Path) -> LanpathResult<()> {
    let path = app_settings_path(root);
    let Some(mut raw) = load_app_section::<serde_json::Value>(root, "client")? else {
        return Ok(());
    };
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
                    let kind = crate::cores::infer_core_type(&binary).unwrap_or_default();
                    let core = CoreSettings {
                        kind,
                        channel: crate::cores::channel_of_version(&current.version)
                            .as_str()
                            .to_string(),
                        current: Some(current),
                        previous: None,
                    };
                    document["core"] = serialize_item(&core)?;
                    if let Some(object) = value.as_object_mut() {
                        object.remove("core_binary");
                        object.remove("core_type");
                    }
                }
                if section == "client"
                    && let Some(object) = value.as_object_mut()
                {
                    object.remove("core_type");
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
            && let Some(core) = load_core_settings(root)?
        {
            value["core_type"] = serde_json::to_value(core.kind)?;
            if let Some(current) = core.current {
                value["core_binary"] = serde_json::to_value(current.path)?;
            }
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
        let _guard = app_write_lock()?;
        let text = read_app_text(path)?;
        let mut value = value.clone();
        if let Some(object) = value.as_object_mut() {
            object.remove("core_binary");
            object.remove("core_type");
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
    let _guard = app_write_lock()?;
    persist_section_unlocked(root, section, value)
}

pub(crate) fn update_app_section<T: Serialize + DeserializeOwned + Default>(
    root: &Path,
    section: &str,
    update: impl FnOnce(&mut T) -> LanpathResult<()>,
    check: impl Fn() -> LanpathResult<()>,
) -> LanpathResult<()> {
    let _guard = app_write_lock()?;
    check()?;
    let mut value: T = load_app_section(root, section)?.unwrap_or_default();
    update(&mut value)?;
    let path = app_settings_path(root);
    let text = read_app_text(&path)?;
    let merged = merge_app_section_text(&text, section, &value)?;
    atomic_write_checked(&path, &merged, check)
}

/// Merge top-level fields in a standalone document, preserving other keys/comments.
pub(crate) fn persist_document_fields<T: Serialize>(path: &Path, value: &T) -> LanpathResult<()> {
    let _guard = app_write_lock()?;
    let text = read_app_text(path)?;
    let mut document = text
        .parse::<DocumentMut>()
        .map_err(|error| config_error(path, error))?;
    let fields = serialize_item(value)?;
    if let Some(table) = fields.as_table() {
        for (key, item) in table.iter() {
            document[key] = item.clone();
        }
    }
    atomic_write(path, &document.to_string())
}

fn read_app_text(path: &Path) -> LanpathResult<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.into()),
    }
}

fn persist_section_unlocked<T: Serialize>(
    root: &Path,
    section: &str,
    value: &T,
) -> LanpathResult<()> {
    let path = app_settings_path(root);
    let text = read_app_text(&path)?;
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
    atomic_write_checked(path, text, || Ok(()))
}

fn atomic_write_checked(
    path: &Path,
    text: &str,
    check: impl Fn() -> LanpathResult<()>,
) -> LanpathResult<()> {
    check()?;
    let parent = path
        .parent()
        .ok_or_else(|| LanpathError::Config(format!("{} has no parent", path.display())))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(text.as_bytes())?;
    file.as_file().sync_all()?;
    check()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn config_error(path: &Path, error: impl std::fmt::Display) -> LanpathError {
    LanpathError::Config(format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests;
