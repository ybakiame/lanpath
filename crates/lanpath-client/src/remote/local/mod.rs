//! Durable local bundles: snippet, associated JavaScript and icon share one owner.
#[cfg(feature = "mitm")]
use super::CachedRemoteConfig;
use super::ScriptDialect;
use serde::{Deserialize, Serialize};

#[cfg(feature = "mitm")]
mod store;
#[cfg(feature = "mitm")]
pub use store::LocalResourceStore;
#[cfg(all(test, feature = "mitm"))]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSource {
    pub name: String,
    pub enabled: bool,
    pub dialect: ScriptDialect,
    /// Old imports did not retain the original snippet. Never invent it from parsed rules.
    pub content: Option<String>,
    pub icon: Option<LocalIcon>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalIcon {
    pub name: String,
    pub mime: String,
    pub base64: String,
}

impl LocalIcon {
    /// Copy the selected image into the bundle, without persisting a host path.
    pub fn from_bytes(name: &str, bytes: &[u8]) -> lanpath_common::LanpathResult<Self> {
        use base64::Engine as _;
        if bytes.len() > 512 * 1024 {
            return Err(lanpath_common::LanpathError::Validation(
                "图标不能超过 512 KiB".into(),
            ));
        }
        let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            "image/jpeg"
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            "image/webp"
        } else {
            return Err(lanpath_common::LanpathError::Validation(
                "请选择 PNG、JPEG 或 WebP 图标".into(),
            ));
        };
        Ok(Self {
            name: name.into(),
            mime: mime.into(),
            base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        })
    }
    pub fn data_uri(&self) -> String {
        format!("data:{};base64,{}", self.mime, self.base64)
    }
}

#[cfg(feature = "mitm")]
#[derive(Debug, Clone)]
pub struct LocalDocument {
    pub id: String,
    pub cached: CachedRemoteConfig,
}

#[cfg(feature = "mitm")]
impl LocalDocument {
    pub fn source(&self) -> LocalSource {
        self.cached.local.clone().unwrap_or_else(|| LocalSource {
            name: self
                .cached
                .meta
                .as_ref()
                .and_then(|meta| meta.name.clone())
                .unwrap_or_else(|| "历史本地片段".into()),
            enabled: true,
            dialect: self
                .cached
                .scripts
                .first()
                .and_then(|rule| rule.dialect)
                .or_else(|| self.cached.task_scripts.first().map(|task| task.dialect))
                .unwrap_or(ScriptDialect::Surge),
            content: None,
            icon: None,
        })
    }
    pub fn assets(&self) -> Vec<(String, String)> {
        let mut assets = std::collections::BTreeMap::new();
        for rule in &self.cached.scripts {
            assets.insert(rule.source_id.clone(), rule.source.clone());
        }
        for task in &self.cached.task_scripts {
            assets.insert(task.source_id.clone(), task.source.clone());
        }
        assets.into_iter().collect()
    }
}
