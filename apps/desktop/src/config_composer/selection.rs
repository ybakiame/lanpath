//! Overview preview reads persisted source unless an associated script requires composition.
use super::*;

#[derive(Clone, PartialEq)]
pub(crate) struct SelectedPreview {
    pub name: String,
    pub content: String,
    pub language: String,
    pub generated: bool,
    pub warnings: Vec<String>,
}

impl ConfigComposer {
    pub(crate) async fn selected_preview(&self) -> LanpathResult<SelectedPreview> {
        let _activity = self.root.activity().read().await;
        let (config, selected, source) = {
            let _guard = self.root.business_guard();
            let config = load_request_config(self.root.path())?;
            let selected = self
                .store()
                .load()?
                .into_iter()
                .find(|s| Some(s.id) == config.active_subscription_id && s.enabled)
                .ok_or_else(|| LanpathError::Validation("请先选择一个启用的订阅".into()))?;
            let source = self.store().load_source(selected.id)?;
            (config, selected, source)
        };
        if selected.profile_id.is_none() {
            let source = source.ok_or_else(|| {
                LanpathError::Validation("所选订阅尚无已保存原文，请先刷新订阅".into())
            })?;
            let language = if source.content.trim_start().starts_with(['{', '[']) {
                "json"
            } else {
                "yaml"
            };
            return Ok(SelectedPreview {
                name: selected.name,
                content: source.content,
                language: language.into(),
                generated: false,
                warnings: Vec::new(),
            });
        }
        self.root.ensure_writable()?;
        let preview = self.compose_with_config(&config).await?;
        Ok(SelectedPreview {
            name: selected.name,
            content: preview.content().into(),
            language: if preview.format() == PreviewFormat::Json {
                "json"
            } else {
                "yaml"
            }
            .into(),
            generated: true,
            warnings: preview.warnings().to_vec(),
        })
    }
}
