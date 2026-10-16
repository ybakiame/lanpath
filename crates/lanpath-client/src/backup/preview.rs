//! One-use, process-local preview. Neither plaintext nor password is persisted.
use super::{Appearance, BackupSummary, Snapshot, decode, invalid, prepare_restore};
use age::secrecy::{ExposeSecret, SecretString};
use lanpath_common::LanpathResult;
use serde::Serialize;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Serialize)]
pub struct BackupPreview {
    #[serde(flatten)]
    pub summary: BackupSummary,
    pub preview_id: String,
}

struct Cached {
    id: String,
    created: Instant,
    snapshot: Snapshot,
    password: SecretString,
}

pub struct RestoreInput {
    snapshot: Snapshot,
    password: SecretString,
}
impl RestoreInput {
    pub fn prepare(
        self,
        dir: &std::path::Path,
        appearance: Option<Appearance>,
    ) -> LanpathResult<String> {
        prepare_restore(
            dir,
            self.snapshot,
            self.password.expose_secret(),
            appearance,
        )
    }
}

#[derive(Default)]
pub struct PreviewCache {
    inner: Mutex<Option<Cached>>,
}

impl PreviewCache {
    pub fn inspect(&self, encrypted: &[u8], password: String) -> LanpathResult<BackupPreview> {
        let mut cache = self.inner.lock().map_err(|_| invalid("预览服务暂不可用"))?;
        *cache = None;
        let snapshot = decode(encrypted, &password)?;
        let id = uuid::Uuid::new_v4().to_string();
        let preview = BackupPreview {
            summary: snapshot.summary.clone(),
            preview_id: id.clone(),
        };
        *cache = Some(Cached {
            id,
            created: Instant::now(),
            snapshot,
            password: password.into(),
        });
        Ok(preview)
    }

    pub fn take(&self, id: &str) -> LanpathResult<RestoreInput> {
        let cached = self.consume(id)?;
        Ok(RestoreInput {
            snapshot: cached.snapshot,
            password: cached.password,
        })
    }

    pub fn discard(&self, id: &str) {
        if let Ok(mut cache) = self.inner.lock()
            && cache.as_ref().is_some_and(|p| p.id == id)
        {
            *cache = None;
        }
    }

    fn consume(&self, id: &str) -> LanpathResult<Cached> {
        let mut cache = self.inner.lock().map_err(|_| invalid("预览服务暂不可用"))?;
        if cache
            .as_ref()
            .is_some_and(|p| p.created.elapsed() > Duration::from_secs(300))
        {
            *cache = None;
        }
        if !cache.as_ref().is_some_and(|p| p.id == id) {
            return Err(invalid("备份预览已过期或已使用，请重新解密检查"));
        }
        cache.take().ok_or_else(|| invalid("请重新解密检查备份"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_tokens_are_bound_expiring_and_single_use() {
        let cache = PreviewCache::default();
        let fixture = || Cached {
            id: "valid".into(),
            created: Instant::now(),
            password: "fixture password".to_owned().into(),
            snapshot: super::super::capture(tempfile::tempdir().unwrap().path(), None).unwrap(),
        };
        *cache.inner.lock().unwrap() = Some(fixture());
        assert!(cache.consume("wrong").is_err());
        assert!(cache.consume("valid").is_ok());
        assert!(cache.consume("valid").is_err());
        let mut expired = fixture();
        expired.created = Instant::now() - Duration::from_secs(301);
        *cache.inner.lock().unwrap() = Some(expired);
        assert!(cache.consume("valid").is_err());
        assert!(cache.inner.lock().unwrap().is_none());
    }
}
