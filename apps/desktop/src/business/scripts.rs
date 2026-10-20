use lanpath_client::remote::{
    FetchReport, ImportSummary, RemoteDetection, RemoteResource, RemoteResourceService,
    ScriptDialect,
};
use lanpath_common::LanpathResult;

use super::BusinessService;

impl BusinessService {
    fn remote_service(&self) -> RemoteResourceService {
        RemoteResourceService::new(self.root.clone())
    }

    pub(crate) fn remote_resources(&self) -> LanpathResult<Vec<RemoteResource>> {
        self.remote_service().load()
    }

    pub(crate) async fn add_remote_resource(
        &self,
        resource: RemoteResource,
    ) -> LanpathResult<RemoteResource> {
        self.remote_service().add(resource).await
    }

    pub(crate) async fn update_remote_resource(
        &self,
        name: &str,
        resource: RemoteResource,
    ) -> LanpathResult<RemoteResource> {
        self.remote_service().update(name, resource).await
    }

    pub(crate) async fn set_remote_resource_enabled(
        &self,
        name: &str,
        enabled: bool,
    ) -> LanpathResult<RemoteResource> {
        self.remote_service().set_enabled(name, enabled).await
    }

    pub(crate) async fn remove_remote_resource(&self, name: &str) -> LanpathResult<()> {
        self.remote_service().remove(name).await
    }

    pub(crate) async fn detect_remote_resource(&self, url: &str) -> LanpathResult<RemoteDetection> {
        self.remote_service().detect(url).await
    }

    pub(crate) async fn fetch_remote_resources(&self) -> LanpathResult<FetchReport> {
        self.remote_service().fetch_all().await
    }

    pub(crate) async fn import_script_config(
        &self,
        content: &str,
        dialect: ScriptDialect,
    ) -> LanpathResult<ImportSummary> {
        self.remote_service().import_content(content, dialect).await
    }
}
