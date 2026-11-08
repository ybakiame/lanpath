use super::{LocalDocument, LocalSource};
use crate::remote::{CachedRemoteConfig, service::WRITE_LOCK};
use lanpath_common::{LanpathError, LanpathResult};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct LocalResourceStore {
    root: PathBuf,
}

impl LocalResourceStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    fn directory(&self) -> LanpathResult<PathBuf> {
        if !crate::resource_storage::unified(&self.root) {
            return Err(invalid("本地资源管理仅用于 GPUI 独立数据目录"));
        }
        Ok(crate::resource_storage::imports_path(&self.root))
    }
    fn ids(&self) -> LanpathResult<Vec<String>> {
        let path = self.directory()?.join("local_imports.json");
        let ids: Vec<String> = if path.exists() {
            serde_json::from_slice(&std::fs::read(path)?)?
        } else {
            Vec::new()
        };
        let mut seen = HashSet::new();
        for id in &ids {
            check_id(id)?;
            if !seen.insert(id) {
                return Err(invalid("本地资源索引包含重复标识"));
            }
        }
        Ok(ids)
    }
    pub fn load(&self) -> LanpathResult<Vec<LocalDocument>> {
        let directory = self.directory()?;
        self.ids()?
            .into_iter()
            .map(|id| {
                let cached: CachedRemoteConfig = serde_json::from_slice(&std::fs::read(
                    directory.join("local").join(format!("{id}.json")),
                )?)?;
                if cached.module_id != format!("local:{id}") {
                    return Err(invalid("本地资源身份不匹配，请恢复备份"));
                }
                Ok(LocalDocument { id, cached })
            })
            .collect()
    }
    /// Parse without network access. URLs in a snippet identify bundled JS, never implicit downloads.
    pub fn prepare(
        &self,
        previous: Option<&LocalDocument>,
        mut source: LocalSource,
        assets: &[(String, String)],
    ) -> LanpathResult<LocalDocument> {
        self.directory()?;
        source.name = source.name.trim().to_owned();
        if source.name.is_empty() {
            return Err(invalid("资源名称不能为空"));
        }
        let id = previous
            .map(|doc| doc.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        check_id(&id)?;
        let mut cached = if let Some(content) = source
            .content
            .as_ref()
            .filter(|text| !text.trim().is_empty())
        {
            if !matches!(
                source.dialect,
                super::ScriptDialect::Surge | super::ScriptDialect::QuantumultX
            ) {
                return Err(invalid("新片段仅支持 Surge / QX"));
            }
            let parsed = crate::import::parse_local_resource(content, source.dialect)?;
            let blocking: Vec<_> = parsed.blocking_warnings().map(String::as_str).collect();
            if !blocking.is_empty() {
                return Err(invalid(&format!(
                    "片段未完整识别，请先修正：{}",
                    blocking.join("；")
                )));
            }
            if parsed.rewrites.is_empty()
                && parsed.scripts.is_empty()
                && parsed.task_scripts.is_empty()
                && parsed.hostnames.is_empty()
                && parsed.mitm_templates.is_empty()
            {
                return Err(invalid("片段中没有可识别的规则、脚本、任务或主机名"));
            }
            let mut cached = CachedRemoteConfig::from_imported(&parsed);
            cached.bind_module(&format!("local:{id}"));
            cached
        } else if let Some(previous) = previous.filter(|doc| doc.source().content.is_none()) {
            source.content = None;
            source.dialect = previous.source().dialect;
            previous.cached.clone()
        } else {
            return Err(invalid("配置片段不能为空"));
        };
        let mut supplied = BTreeMap::new();
        for (reference, text) in assets {
            if reference.is_empty() || supplied.insert(reference.as_str(), text.as_str()).is_some()
            {
                return Err(invalid("关联脚本引用为空或重复"));
            }
        }
        let mut used = HashSet::new();
        for (reference, text) in cached
            .scripts
            .iter_mut()
            .map(|rule| (&rule.source_id, &mut rule.source))
            .chain(
                cached
                    .task_scripts
                    .iter_mut()
                    .map(|task| (&task.source_id, &mut task.source)),
            )
        {
            let body = supplied
                .get(reference.as_str())
                .filter(|body| !body.trim().is_empty())
                .ok_or_else(|| invalid("请为每个片段引用填写关联 JS，或先识别片段刷新脚本列表"))?;
            *text = (*body).to_owned();
            used.insert(reference.as_str());
        }
        if supplied.keys().any(|reference| !used.contains(reference)) {
            return Err(invalid("存在未被片段引用的 JS，请先识别片段刷新关联列表"));
        }
        if let Some(icon) = &source.icon {
            use base64::Engine as _;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&icon.base64)
                .map_err(|_| invalid("图标数据损坏"))?;
            if super::LocalIcon::from_bytes(&icon.name, &bytes)?.mime != icon.mime {
                return Err(invalid("图标格式不匹配"));
            }
        }
        cached.local = Some(source);
        cached.clone().into_merged()?;
        Ok(LocalDocument { id, cached })
    }
    pub async fn save(&self, document: LocalDocument, creating: bool) -> LanpathResult<()> {
        let _guard = WRITE_LOCK.lock().await;
        let directory = self.directory()?;
        check_id(&document.id)?;
        let mut ids = self.ids()?;
        if ids.contains(&document.id) == creating {
            return Err(invalid("本地资源状态已变化，请重新打开编辑"));
        }
        if document.cached.module_id != format!("local:{}", document.id) {
            return Err(invalid("本地资源身份不匹配"));
        }
        let path = directory
            .join("local")
            .join(format!("{}.json", document.id));
        write(&path, &serde_json::to_vec_pretty(&document.cached)?)?;
        if creating {
            ids.push(document.id.clone());
            if let Err(error) = write(
                &directory.join("local_imports.json"),
                &serde_json::to_vec(&ids)?,
            ) {
                let _ = std::fs::remove_file(path);
                return Err(error);
            }
        }
        Ok(())
    }
    pub async fn set_enabled(&self, id: &str, enabled: bool) -> LanpathResult<()> {
        let _guard = WRITE_LOCK.lock().await;
        let mut document = self
            .load()?
            .into_iter()
            .find(|doc| doc.id == id)
            .ok_or_else(|| invalid("资源不存在"))?;
        let mut source = document.source();
        source.enabled = enabled;
        document.cached.local = Some(source);
        write(
            &self.directory()?.join("local").join(format!("{id}.json")),
            &serde_json::to_vec_pretty(&document.cached)?,
        )
    }
    pub async fn remove(&self, id: &str) -> LanpathResult<()> {
        let _guard = WRITE_LOCK.lock().await;
        check_id(id)?;
        let mut ids = self.ids()?;
        if !ids.iter().any(|existing| existing == id) {
            return Err(invalid("资源不存在"));
        }
        ids.retain(|existing| existing != id);
        let directory = self.directory()?;
        // Remove activation authority first. File failure leaves an inactive, recoverable bundle.
        write(
            &directory.join("local_imports.json"),
            &serde_json::to_vec(&ids)?,
        )?;
        std::fs::remove_file(directory.join("local").join(format!("{id}.json")))?;
        Ok(())
    }
}
fn check_id(id: &str) -> LanpathResult<()> {
    if uuid::Uuid::parse_str(id)
        .ok()
        .is_none_or(|uuid| uuid.to_string() != id)
    {
        return Err(invalid("无效本地资源标识"));
    }
    Ok(())
}
fn invalid(message: &str) -> LanpathError {
    LanpathError::Validation(message.into())
}
fn write(path: &Path, bytes: &[u8]) -> LanpathResult<()> {
    let parent = path.parent().ok_or_else(|| invalid("无效资源路径"))?;
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    use std::io::Write as _;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| LanpathError::Io(error.error))?;
    Ok(())
}
