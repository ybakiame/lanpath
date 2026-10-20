use std::path::PathBuf;

use lanpath_common::{LanpathError, LanpathResult};
use lanpath_script::ScriptDialect;
use tokio::sync::Mutex;

use crate::{normalize_resource_url, parse_config_meta, validate_remote_url};

#[cfg(feature = "mitm")]
use super::ImportSummary;
use super::{
    ConfigMeta, FetchReport, RemoteKind, RemoteManager, RemoteResource, detect_resource_from_url,
    prefill_argument_values,
};

static WRITE_LOCK: Mutex<()> = Mutex::const_new(());

/// Metadata detected from a remote resource URL.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct RemoteDetection {
    pub normalized_url: String,
    pub kind: Option<RemoteKind>,
    pub dialect: Option<ScriptDialect>,
    pub meta: Option<ConfigMeta>,
}

/// Shared remote-resource business service used by native shells.
#[derive(Debug, Clone)]
pub struct RemoteResourceService {
    data_dir: PathBuf,
}

impl RemoteResourceService {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    pub fn load(&self) -> LanpathResult<Vec<RemoteResource>> {
        self.manager().load()
    }

    pub async fn add(&self, mut resource: RemoteResource) -> LanpathResult<RemoteResource> {
        normalize_resource(&mut resource)?;
        let _guard = WRITE_LOCK.lock().await;
        let manager = self.manager();
        let mut resources = manager.load()?;
        if resources.iter().any(|item| item.name == resource.name) {
            return Err(LanpathError::Client(format!(
                "远程资源 '{}' 已存在",
                resource.name
            )));
        }
        resources.push(resource.clone());
        manager.save(&resources)?;
        drop(_guard);
        self.cache_icon_warn_only(&resource).await;
        Ok(resource)
    }

    pub async fn update(
        &self,
        name: &str,
        mut resource: RemoteResource,
    ) -> LanpathResult<RemoteResource> {
        let name = required_name(name)?;
        normalize_resource(&mut resource)?;
        resource.name = name.clone();
        let _guard = WRITE_LOCK.lock().await;
        self.manager().update_resource(&name, resource.clone())?;
        drop(_guard);
        self.cache_icon_warn_only(&resource).await;
        Ok(resource)
    }

    pub async fn set_enabled(&self, name: &str, enabled: bool) -> LanpathResult<RemoteResource> {
        let name = required_name(name)?;
        let _guard = WRITE_LOCK.lock().await;
        let manager = self.manager();
        let mut resources = manager.load()?;
        let resource = resources
            .iter_mut()
            .find(|item| item.name == name)
            .ok_or_else(|| LanpathError::Client(format!("远程资源 '{name}' 不存在")))?;
        resource.enabled = enabled;
        let updated = resource.clone();
        manager.save(&resources)?;
        Ok(updated)
    }

    pub async fn remove(&self, name: &str) -> LanpathResult<()> {
        let name = required_name(name)?;
        let _guard = WRITE_LOCK.lock().await;
        let manager = self.manager();
        let mut resources = manager.load()?;
        let before = resources.len();
        resources.retain(|item| item.name != name);
        if resources.len() == before {
            return Err(LanpathError::Client(format!("远程资源 '{name}' 不存在")));
        }
        manager.save(&resources)
    }

    pub async fn detect(&self, url: &str) -> LanpathResult<RemoteDetection> {
        let normalized_url = normalize_remote_url(url)?;
        let detected = detect_resource_from_url(&normalized_url);
        let (kind, dialect) = detected.unzip();
        let meta = if kind == Some(RemoteKind::Snippet) {
            crate::fetch_resource_text(
                &self.data_dir,
                &normalized_url,
                std::time::Duration::from_secs(15),
            )
            .await
            .ok()
            .map(|content| parse_config_meta(&content))
            .filter(|meta| meta != &ConfigMeta::default())
        } else {
            None
        };
        Ok(RemoteDetection {
            normalized_url,
            kind,
            dialect,
            meta,
        })
    }

    pub async fn fetch_all(&self) -> LanpathResult<FetchReport> {
        let _guard = WRITE_LOCK.lock().await;
        let manager = self.manager();
        let resources = manager.load()?;
        Ok(manager.fetch_all(&resources).await)
    }

    #[cfg(feature = "mitm")]
    pub async fn import_content(
        &self,
        content: &str,
        dialect: ScriptDialect,
    ) -> LanpathResult<ImportSummary> {
        if content.trim().is_empty() {
            return Err(LanpathError::Validation("导入内容不能为空".into()));
        }
        let _guard = WRITE_LOCK.lock().await;
        self.manager().import_content(content, dialect).await
    }

    fn manager(&self) -> RemoteManager {
        RemoteManager::new(self.data_dir.clone())
    }

    async fn cache_icon_warn_only(&self, resource: &RemoteResource) {
        if let Some(icon) = resource.icon.as_deref()
            && let Err(error) = self.manager().cache_icon(&resource.name, icon).await
        {
            tracing::warn!(name = %resource.name, %error, "icon cache warmup failed");
        }
    }
}

fn normalize_resource(resource: &mut RemoteResource) -> LanpathResult<()> {
    resource.name = required_name(&resource.name)?;
    resource.url = normalize_remote_url(&resource.url)?;
    if resource.update_interval_secs == 0 {
        resource.update_interval_secs = 86_400;
    }
    resource.description = resource
        .description
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    resource.icon = resource
        .icon
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    prefill_argument_values(resource);
    Ok(())
}

fn required_name(name: &str) -> LanpathResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(LanpathError::Validation("资源名不能为空".into()));
    }
    Ok(name.to_string())
}

fn normalize_remote_url(url: &str) -> LanpathResult<String> {
    let normalized = normalize_resource_url(url.trim());
    if normalized.is_empty() {
        return Err(LanpathError::Validation("URL 不能为空".into()));
    }
    validate_remote_url(&Some(normalized.clone())).map_err(LanpathError::Validation)?;
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource(name: &str) -> RemoteResource {
        RemoteResource {
            name: name.into(),
            url: "https://example.com/demo.js".into(),
            ..RemoteResource::default()
        }
    }

    #[tokio::test]
    async fn crud_normalizes_and_preserves_resource_identity() {
        let root = tempfile::tempdir().unwrap();
        let service = RemoteResourceService::new(root.path());
        let added = service.add(resource(" demo ")).await.unwrap();
        assert_eq!(added.name, "demo");
        assert_eq!(service.load().unwrap(), vec![added.clone()]);

        let mut updated = added;
        updated.name = "ignored rename".into();
        updated.description = Some(" updated ".into());
        let updated = service.update("demo", updated).await.unwrap();
        assert_eq!(updated.name, "demo");
        assert_eq!(updated.description.as_deref(), Some("updated"));

        let disabled = service.set_enabled("demo", false).await.unwrap();
        assert!(!disabled.enabled);
        service.remove("demo").await.unwrap();
        assert!(service.load().unwrap().is_empty());
    }

    #[tokio::test]
    async fn rejects_duplicates_and_non_http_urls_without_mutation() {
        let root = tempfile::tempdir().unwrap();
        let service = RemoteResourceService::new(root.path());
        service.add(resource("demo")).await.unwrap();
        assert!(service.add(resource("demo")).await.is_err());
        let mut invalid = resource("invalid");
        invalid.url = "file:///tmp/script.js".into();
        assert!(service.add(invalid).await.is_err());
        assert_eq!(service.load().unwrap().len(), 1);
    }
}
