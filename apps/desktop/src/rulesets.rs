//! 桌面端规则集与分组业务服务：封装 `<data_root>/rulesets/` 清单与本地缓存文件的
//! 读写、校验、分组管理、市场添加与条件 HTTP 更新（与订阅/规则/备份共用业务锁）。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use lanpath_client::rulesets::{
    MarketMirror, MarketRuleSetSpec, RuleSetEntry, RuleSetGroup, RuleSetManifest,
    RuleSetRemoteState, RuleSetSource, RuleSetStore, RuleSetUpdateOutcome, RuleSetUpdateStatus,
    update_all, update_one,
};
use lanpath_common::{LanpathError, LanpathResult};

use crate::data_root::{DataRoot, open_root, resolve_data_location};
use crate::request_settings::load_request_config;

/// UI 展示用规则集行数据（清单条目 + 本地缓存状态）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuleSetRow {
    pub(crate) entry: RuleSetEntry,
    pub(crate) has_file: bool,
    pub(crate) file_size: u64,
    pub(crate) rel_path: String,
}

/// 规则集与分组完整快照（供规则集页、规则页与配置切片页共享读取）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RuleSetsSnapshot {
    pub(crate) items: Vec<RuleSetRow>,
    pub(crate) groups: Vec<RuleSetGroup>,
}

#[derive(Clone)]
pub(crate) struct RuleSetsService {
    root: Arc<DataRoot>,
}

impl RuleSetsService {
    pub(crate) fn open() -> LanpathResult<Self> {
        Self::for_root(&resolve_data_location()?.path)
    }

    pub(crate) fn for_root(path: &Path) -> LanpathResult<Self> {
        Ok(Self {
            root: open_root(path)?,
        })
    }

    fn store(&self) -> RuleSetStore {
        RuleSetStore::new(self.root.path().to_path_buf())
    }

    pub(crate) fn rulesets_dir_display(&self) -> String {
        self.store().root_dir().to_string_lossy().into_owned()
    }

    pub(crate) fn list_snapshot(&self) -> LanpathResult<RuleSetsSnapshot> {
        let _guard = self.root.business_guard();
        let store = self.store();
        let manifest = store.load_manifest()?;
        Ok(to_snapshot(&store, manifest))
    }

    pub(crate) fn upsert(&self, mut entry: RuleSetEntry) -> LanpathResult<RuleSetsSnapshot> {
        let _guard = self.root.business_guard();
        self.root.ensure_writable()?;
        let store = self.store();
        let mut manifest = store.load_manifest()?;
        entry.tag = entry.tag.trim().to_owned();
        entry.name = entry.name.trim().to_owned();
        entry.category = entry.category.trim().to_owned();
        if let RuleSetSource::Remote { url, .. } = &mut entry.source {
            *url = url.trim().to_owned();
        }
        if let Some(existing) = manifest.items.iter_mut().find(|item| item.id == entry.id) {
            let old_tag = existing.tag.clone();
            let source_changed = existing.source != entry.source;
            if source_changed {
                entry.remote_state = RuleSetRemoteState::default();
            } else {
                entry.remote_state = existing.remote_state.clone();
            }
            entry.created_at = existing.created_at;
            let new_tag = entry.tag.clone();
            *existing = entry;
            if old_tag != new_tag {
                for group in &mut manifest.groups {
                    for member in &mut group.member_tags {
                        if *member == old_tag {
                            *member = new_tag.clone();
                        }
                    }
                }
            }
        } else {
            if entry.id.trim().is_empty() {
                entry.id = uuid::Uuid::new_v4().to_string();
            }
            if entry.created_at == 0 {
                entry.created_at = unix_now();
            }
            manifest.items.push(entry);
        }
        store.save_manifest(&manifest)?;
        Ok(to_snapshot(&store, manifest))
    }

    pub(crate) fn delete(&self, id: &str) -> LanpathResult<RuleSetsSnapshot> {
        let _guard = self.root.business_guard();
        self.root.ensure_writable()?;
        let store = self.store();
        let mut manifest = store.load_manifest()?;
        let removed_tag = manifest
            .items
            .iter()
            .find(|item| item.id == id)
            .map(|item| item.tag.clone())
            .ok_or_else(|| LanpathError::Validation("规则集已不存在".into()))?;
        manifest.items.retain(|item| item.id != id);
        for group in &mut manifest.groups {
            group
                .member_tags
                .retain(|tag| !tag.eq_ignore_ascii_case(&removed_tag));
        }
        store.save_manifest(&manifest)?;
        Ok(to_snapshot(&store, manifest))
    }

    pub(crate) fn upsert_group(&self, mut group: RuleSetGroup) -> LanpathResult<RuleSetsSnapshot> {
        let _guard = self.root.business_guard();
        self.root.ensure_writable()?;
        let store = self.store();
        let mut manifest = store.load_manifest()?;
        group.tag = group.tag.trim().trim_start_matches('@').to_owned();
        group.name = group.name.trim().to_owned();
        group.description = group.description.trim().to_owned();
        let mut dedup = Vec::new();
        for tag in group.member_tags {
            let trimmed = tag.trim().to_owned();
            if !trimmed.is_empty()
                && !dedup
                    .iter()
                    .any(|t: &String| t.eq_ignore_ascii_case(&trimmed))
            {
                dedup.push(trimmed);
            }
        }
        group.member_tags = dedup;
        if let Some(existing) = manifest.groups.iter_mut().find(|g| g.id == group.id) {
            group.created_at = existing.created_at;
            *existing = group;
        } else {
            if group.id.trim().is_empty() {
                group.id = uuid::Uuid::new_v4().to_string();
            }
            if group.created_at == 0 {
                group.created_at = unix_now();
            }
            manifest.groups.push(group);
        }
        store.save_manifest(&manifest)?;
        Ok(to_snapshot(&store, manifest))
    }

    pub(crate) fn delete_group(&self, id: &str) -> LanpathResult<RuleSetsSnapshot> {
        let _guard = self.root.business_guard();
        self.root.ensure_writable()?;
        let store = self.store();
        let mut manifest = store.load_manifest()?;
        let before = manifest.groups.len();
        manifest.groups.retain(|g| g.id != id);
        if manifest.groups.len() == before {
            return Err(LanpathError::Validation("规则集分组已不存在".into()));
        }
        store.save_manifest(&manifest)?;
        Ok(to_snapshot(&store, manifest))
    }

    pub(crate) fn add_from_market(
        &self,
        spec: MarketRuleSetSpec,
        mirror: MarketMirror,
        zh: bool,
    ) -> LanpathResult<(RuleSetEntry, RuleSetsSnapshot)> {
        let _guard = self.root.business_guard();
        self.root.ensure_writable()?;
        let store = self.store();
        let mut manifest = store.load_manifest()?;
        if manifest
            .items
            .iter()
            .any(|item| item.tag.eq_ignore_ascii_case(spec.tag))
        {
            return Err(LanpathError::Validation(format!(
                "规则集 `{}` 已存在",
                spec.tag
            )));
        }
        let entry = RuleSetEntry {
            id: uuid::Uuid::new_v4().to_string(),
            tag: spec.tag.to_owned(),
            name: if zh { spec.name_zh } else { spec.name_en }.to_owned(),
            category: spec.category.to_owned(),
            source: RuleSetSource::Remote {
                url: spec.url_for_mirror(mirror),
                format: spec.format,
            },
            remote_state: RuleSetRemoteState::default(),
            created_at: unix_now(),
        };
        manifest.items.push(entry.clone());
        store.save_manifest(&manifest)?;
        Ok((entry, to_snapshot(&store, manifest)))
    }

    pub(crate) async fn update_one_remote(
        &self,
        id: &str,
    ) -> LanpathResult<(RuleSetUpdateStatus, RuleSetsSnapshot)> {
        let _activity = self.root.activity().read().await;
        let (mut entry, cfg, root_path) = {
            let _guard = self.root.business_guard();
            self.root.ensure_writable()?;
            let store = self.store();
            let manifest = store.load_manifest()?;
            let entry = manifest
                .items
                .into_iter()
                .find(|item| item.id == id)
                .ok_or_else(|| LanpathError::Validation("规则集已不存在".into()))?;
            let cfg = load_request_config(self.root.path())?;
            (entry, cfg, self.root.path().to_path_buf())
        };

        let status = update_one(&root_path, &mut entry, Some(&cfg)).await?;

        let snapshot = {
            let _guard = self.root.business_guard();
            self.root.ensure_writable()?;
            let store = self.store();
            let mut manifest = store.load_manifest()?;
            if let Some(current) = manifest.items.iter_mut().find(|item| item.id == id) {
                current.remote_state = entry.remote_state;
                store.save_manifest(&manifest)?;
            }
            to_snapshot(&store, manifest)
        };
        Ok((status, snapshot))
    }

    pub(crate) async fn update_all_remote(
        &self,
    ) -> LanpathResult<(RuleSetUpdateOutcome, RuleSetsSnapshot)> {
        let _activity = self.root.activity().read().await;
        let (mut items, cfg, root_path): (Vec<RuleSetEntry>, _, PathBuf) = {
            let _guard = self.root.business_guard();
            self.root.ensure_writable()?;
            let store = self.store();
            let manifest = store.load_manifest()?;
            let cfg = load_request_config(self.root.path())?;
            (manifest.items, cfg, self.root.path().to_path_buf())
        };

        let outcome = update_all(&root_path, &mut items, Some(&cfg)).await;

        let snapshot = {
            let _guard = self.root.business_guard();
            self.root.ensure_writable()?;
            let store = self.store();
            let mut manifest = store.load_manifest()?;
            for updated in &items {
                if let Some(current) = manifest.items.iter_mut().find(|item| item.id == updated.id)
                {
                    current.remote_state = updated.remote_state.clone();
                }
            }
            store.save_manifest(&manifest)?;
            to_snapshot(&store, manifest)
        };
        Ok((outcome, snapshot))
    }
}

fn to_snapshot(store: &RuleSetStore, manifest: RuleSetManifest) -> RuleSetsSnapshot {
    let items = manifest
        .items
        .into_iter()
        .map(|entry| {
            let format = entry.file_format();
            let path = store.backing_file_path(&entry.id, format);
            let file_size = std::fs::metadata(&path)
                .ok()
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .unwrap_or(0);
            let rel_path = RuleSetStore::backing_file_rel_path(&entry.id, format);
            RuleSetRow {
                has_file: file_size > 0,
                file_size,
                rel_path,
                entry,
            }
        })
        .collect();
    RuleSetsSnapshot {
        items,
        groups: manifest.groups,
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
