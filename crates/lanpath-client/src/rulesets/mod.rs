//! 全新独立规则集管理模块（`<data_root>/rulesets/`）。
//!
//! 目录布局：
//! - `<data_root>/rulesets/manifest.json`：用户与市场规则集清单及应用内分组；
//! - `<data_root>/rulesets/files/<id>.{srs,json}`：本地规则集实体文件。

mod compose;
mod market;
#[cfg(test)]
mod tests;
mod updater;

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use lanpath_common::{LanpathError, LanpathResult};
use serde::{Deserialize, Serialize};

pub use compose::{
    apply_managed_rule_sets, collect_referenced_rule_set_tags, expand_rule_set_groups,
};
pub use market::{MarketMirror, MarketRuleSetSpec, market_catalog};
pub use updater::{prepare_referenced_missing, update_all, update_one};

/// 规则集根目录名。
pub const RULESETS_DIR: &str = "rulesets";
/// 规则集元数据清单文件名。
pub const MANIFEST_FILE: &str = "manifest.json";
/// 规则集实体文件子目录名。
pub const FILES_SUBDIR: &str = "files";
/// 清单格式版本。
pub const MANIFEST_VERSION: u32 = 1;
/// 规则集分组引用前缀（例如 `@direct-cn`）。
pub const GROUP_REF_PREFIX: &str = "@";

/// 规则集文件格式（与 sing-box `rule_set.format` 对齐）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleSetFormat {
    Binary,
    Source,
}

impl RuleSetFormat {
    #[must_use]
    pub const fn ext(self) -> &'static str {
        match self {
            Self::Binary => "srs",
            Self::Source => "json",
        }
    }

    #[must_use]
    pub const fn as_singbox_str(self) -> &'static str {
        match self {
            Self::Binary => "binary",
            Self::Source => "source",
        }
    }
}

/// 规则集来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuleSetSource {
    /// 远程 URL 下载缓存至 `rulesets/files/<id>.<ext>`。
    Remote { url: String, format: RuleSetFormat },
    /// 手动编辑的 sing-box 规则集源 JSON，落盘至 `rulesets/files/<id>.json`。
    Manual { content: String },
}

/// 远端 HTTP 条件校验状态（按来源 URL 绑定）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSetRemoteState {
    #[serde(default)]
    pub content_sha256: String,
    #[serde(default)]
    pub checked_at: u64,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub remote_updated_at: u64,
    #[serde(default)]
    pub validator_url: String,
    #[serde(default)]
    pub etag: Option<String>,
    #[serde(default)]
    pub last_modified: Option<String>,
}

/// 单条受管规则集定义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSetEntry {
    pub id: String,
    pub tag: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub category: String,
    pub source: RuleSetSource,
    #[serde(default)]
    pub remote_state: RuleSetRemoteState,
    #[serde(default)]
    pub created_at: u64,
}

impl RuleSetEntry {
    #[must_use]
    pub fn file_format(&self) -> RuleSetFormat {
        match self.source {
            RuleSetSource::Remote { format, .. } => format,
            RuleSetSource::Manual { .. } => RuleSetFormat::Source,
        }
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        if self.name.trim().is_empty() {
            &self.tag
        } else {
            &self.name
        }
    }
}

/// 应用内规则集分组：将多个受管规则集聚合为一个 `@<tag>` 引用，
/// 在规则页及 `route.json` / `dns.json` 中可直接按 `@<tag>` 整体引用，合成时自动展开。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSetGroup {
    pub id: String,
    pub tag: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub member_tags: Vec<String>,
    #[serde(default)]
    pub created_at: u64,
}

impl RuleSetGroup {
    #[must_use]
    pub fn display_name(&self) -> &str {
        if self.name.trim().is_empty() {
            &self.tag
        } else {
            &self.name
        }
    }

    #[must_use]
    pub fn ref_token(&self) -> String {
        format!("{GROUP_REF_PREFIX}{}", self.tag)
    }
}

/// `rulesets/manifest.json` 持久化结构。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSetManifest {
    #[serde(default = "default_manifest_version")]
    pub version: u32,
    #[serde(default)]
    pub items: Vec<RuleSetEntry>,
    #[serde(default)]
    pub groups: Vec<RuleSetGroup>,
}

impl Default for RuleSetManifest {
    fn default() -> Self {
        Self {
            version: MANIFEST_VERSION,
            items: Vec::new(),
            groups: Vec::new(),
        }
    }
}

const fn default_manifest_version() -> u32 {
    MANIFEST_VERSION
}

/// 单条远程规则集更新状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSetUpdateStatus {
    Updated,
    Skipped,
    Failed,
}

/// 批量规则集更新汇总。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RuleSetUpdateOutcome {
    pub updated: usize,
    pub skipped: usize,
    pub failed: usize,
}

/// 规则集清单与本地文件存储。
#[derive(Debug, Clone)]
pub struct RuleSetStore {
    data_dir: PathBuf,
}

impl RuleSetStore {
    #[must_use]
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    #[must_use]
    pub fn root_dir(&self) -> PathBuf {
        self.data_dir.join(RULESETS_DIR)
    }

    #[must_use]
    pub fn manifest_path(&self) -> PathBuf {
        self.root_dir().join(MANIFEST_FILE)
    }

    #[must_use]
    pub fn files_dir(&self) -> PathBuf {
        self.root_dir().join(FILES_SUBDIR)
    }

    /// 相对 `<data_root>` 的可移植路径（供 sing-box `-D <data_root>` 与备份校验）。
    #[must_use]
    pub fn backing_file_rel_path(id: &str, format: RuleSetFormat) -> String {
        format!("{RULESETS_DIR}/{FILES_SUBDIR}/{id}.{}", format.ext())
    }

    #[must_use]
    pub fn backing_file_path(&self, id: &str, format: RuleSetFormat) -> PathBuf {
        self.files_dir().join(format!("{id}.{}", format.ext()))
    }

    #[must_use]
    pub fn has_backing_file(&self, entry: &RuleSetEntry) -> bool {
        let path = self.backing_file_path(&entry.id, entry.file_format());
        std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.len() > 0)
    }

    /// 加载完整规则集清单（条目 + 分组）；不存在时返回空清单且不落盘。
    pub fn load_manifest(&self) -> LanpathResult<RuleSetManifest> {
        let path = self.manifest_path();
        match std::fs::read(&path) {
            Ok(bytes) => Self::validate_manifest(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(RuleSetManifest::default())
            }
            Err(error) => Err(error.into()),
        }
    }

    /// 加载规则集条目列表；不存在时返回空列表且不落盘。
    pub fn load(&self) -> LanpathResult<Vec<RuleSetEntry>> {
        Ok(self.load_manifest()?.items)
    }

    /// 校验备份或磁盘上的 `rulesets/manifest.json` 字节内容并返回完整清单。
    pub fn validate_manifest(bytes: &[u8]) -> LanpathResult<RuleSetManifest> {
        let manifest: RuleSetManifest = serde_json::from_slice(bytes)
            .map_err(|error| LanpathError::Config(format!("规则集清单解析失败：{error}")))?;
        if manifest.version != MANIFEST_VERSION {
            return Err(LanpathError::Config(format!(
                "不支持的规则集清单版本：{}",
                manifest.version
            )));
        }
        validate_entries(&manifest.items)?;
        validate_groups(&manifest.groups, &manifest.items)?;
        Ok(manifest)
    }

    /// 校验备份或磁盘上的 `rulesets/manifest.json` 字节内容。
    pub fn validate_manifest_bytes(bytes: &[u8]) -> LanpathResult<Vec<RuleSetEntry>> {
        Ok(Self::validate_manifest(bytes)?.items)
    }

    /// 校验并原子保存完整清单（条目 + 分组），同时同步手动 JSON 实体文件并清理孤立文件。
    pub fn save_manifest(&self, manifest: &RuleSetManifest) -> LanpathResult<()> {
        validate_entries(&manifest.items)?;
        validate_groups(&manifest.groups, &manifest.items)?;
        let root = self.root_dir();
        let files_dir = self.files_dir();
        std::fs::create_dir_all(&files_dir)?;

        let normalized = RuleSetManifest {
            version: MANIFEST_VERSION,
            items: manifest.items.clone(),
            groups: manifest.groups.clone(),
        };
        let mut bytes = serde_json::to_vec_pretty(&normalized)?;
        bytes.push(b'\n');
        let mut temp = tempfile::NamedTempFile::new_in(&root)?;
        temp.write_all(&bytes)?;
        temp.as_file().sync_all()?;
        temp.persist(self.manifest_path())
            .map_err(|error| error.error)?;

        self.sync_files(&normalized.items)?;
        Ok(())
    }

    /// 校验并原子保存规则集条目（保留既有分组定义）。
    pub fn save(&self, items: &[RuleSetEntry]) -> LanpathResult<()> {
        let mut manifest = self.load_manifest().unwrap_or_default();
        manifest.items = items.to_vec();
        self.save_manifest(&manifest)
    }

    /// 将手动规则集正文写入磁盘并清理已删除或切换格式残留的孤立文件。
    pub fn sync_files(&self, items: &[RuleSetEntry]) -> LanpathResult<()> {
        let dir = self.files_dir();
        std::fs::create_dir_all(&dir)?;

        let keep: HashSet<String> = items
            .iter()
            .map(|entry| format!("{}.{}", entry.id, entry.file_format().ext()))
            .collect();

        for entry in items {
            if let RuleSetSource::Manual { content } = &entry.source {
                let path = self.backing_file_path(&entry.id, RuleSetFormat::Source);
                let mut temp = tempfile::NamedTempFile::new_in(&dir)?;
                temp.write_all(content.as_bytes())?;
                temp.as_file().sync_all()?;
                temp.persist(path).map_err(|error| error.error)?;
            }
        }

        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !keep.contains(&name) && entry.path().is_file() {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
    }
}

/// 校验单个规则集 tag 合法性（允许字母、数字、`-`、`_`、`!`）。
pub fn validate_rule_set_tag(tag: &str) -> LanpathResult<()> {
    let trimmed = tag.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(LanpathError::Validation(
            "规则集标签不能为空且长度不得超过 64 个字符".into(),
        ));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '!' | '.'))
    {
        return Err(LanpathError::Validation(format!(
            "规则集标签 `{trimmed}` 仅支持字母、数字及 - _ . !"
        )));
    }
    Ok(())
}

/// 校验整份规则集清单（ID 唯一、tag 合法唯一、来源内容合法）。
pub fn validate_entries(items: &[RuleSetEntry]) -> LanpathResult<()> {
    let mut ids = HashSet::new();
    let mut tags = HashSet::new();
    for entry in items {
        let id = entry.id.trim();
        if id.is_empty()
            || id.contains(['/', '\\', '.'])
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        {
            return Err(LanpathError::Validation(format!("规则集 ID 无效：{id}")));
        }
        if !ids.insert(id.to_owned()) {
            return Err(LanpathError::Validation(format!("规则集 ID 重复：{id}")));
        }
        validate_rule_set_tag(&entry.tag)?;
        if !tags.insert(entry.tag.to_lowercase()) {
            return Err(LanpathError::Validation(format!(
                "规则集标签重复：{}",
                entry.tag
            )));
        }
        match &entry.source {
            RuleSetSource::Remote { url, .. } => {
                let url = url.trim();
                if !(url.starts_with("https://") || url.starts_with("http://")) {
                    return Err(LanpathError::Validation(format!(
                        "规则集 `{}` 的远程地址必须以 http:// 或 https:// 开头",
                        entry.tag
                    )));
                }
            }
            RuleSetSource::Manual { content } => {
                let parsed: serde_json::Value = serde_json::from_str(content).map_err(|error| {
                    LanpathError::Validation(format!(
                        "规则集 `{}` 的手动 JSON 无效：{error}",
                        entry.tag
                    ))
                })?;
                if !parsed.is_object() {
                    return Err(LanpathError::Validation(format!(
                        "规则集 `{}` 的手动 JSON 顶层必须是对象",
                        entry.tag
                    )));
                }
            }
        }
    }
    Ok(())
}

/// 校验规则集分组列表（ID 唯一、tag 合法唯一且不与规则集标签冲突）。
pub fn validate_groups(groups: &[RuleSetGroup], items: &[RuleSetEntry]) -> LanpathResult<()> {
    let mut ids = HashSet::new();
    let mut tags = HashSet::new();
    let item_tags: HashSet<String> = items.iter().map(|i| i.tag.to_lowercase()).collect();

    for group in groups {
        let id = group.id.trim();
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        {
            return Err(LanpathError::Validation(format!(
                "规则集分组 ID 无效：{id}"
            )));
        }
        if !ids.insert(id.to_owned()) {
            return Err(LanpathError::Validation(format!(
                "规则集分组 ID 重复：{id}"
            )));
        }
        validate_rule_set_tag(&group.tag)?;
        let lower = group.tag.to_lowercase();
        if !tags.insert(lower.clone()) {
            return Err(LanpathError::Validation(format!(
                "规则集分组标签重复：{}",
                group.tag
            )));
        }
        if item_tags.contains(&lower) {
            return Err(LanpathError::Validation(format!(
                "分组标签 `{}` 与同名规则集冲突，请换用不同标签",
                group.tag
            )));
        }
        for member in &group.member_tags {
            validate_rule_set_tag(member)?;
        }
    }
    Ok(())
}
